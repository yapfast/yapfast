// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! Attaching media to a call that is already joined: the one copy of the
//! ordered wiring between the core's encryption manager, the key handler, the
//! [`CallEngine`] and the own-focus connection.
//!
//! The order is the point. Each step out of place does not fail; it silences
//! media (peers in the roster with nothing to decrypt), so every host — the
//! FFI and wasm media sessions, the native `LiveKitCall` facade — goes through
//! [`attach_media`] rather than repeating it. Transport-specific setup (the key
//! ring) stays with the caller.

use std::sync::Arc;
use std::time::Duration;

use matrix_rtc_call::RtcCall;
use matrix_rtc_core::{IdentitySubject, MatrixBackend, RtcIdentityMapper, TransportIntent};
use matrix_rtc_transport::{
    ConnectionContext, MediaKeyHandler, MediaTransport, OwnFocusTransport, OwnMemberClaims,
    TransportConnection, TransportError,
};
use tokio::sync::broadcast;

use crate::engine::{CallEngine, EngineConfig, StabilityConfig};
use crate::event::CallEvent;

/// What [`attach_media`] needs besides the call, the transport and the key
/// handler.
pub struct AttachOptions {
    /// The account and device the call was joined as.
    pub own_user_id: String,
    pub own_device_id: String,
    /// How a member becomes a transport identity. MUST be
    /// the one the transport was built with: the core's encryption manager,
    /// the transport, our own identity and the key ring all derive through
    /// it, and a skew between them is silent.
    pub identity_mapper: RtcIdentityMapper,
    /// Damping of the tile order.
    pub stability: StabilityConfig,
    /// How long our own focus may stay unreachable before the call ends
    /// (034 R24): [`OWN_RECONNECT_BUDGET`](matrix_rtc_transport::pool::OWN_RECONNECT_BUDGET)
    /// unless a host configured another.
    pub own_reconnect_budget: Duration,
    /// Whether our media may move to a peer's SFU while ours is down
    /// (034 R25, contract C11): only where that SFU accepts our token.
    pub sfu_fallback: bool,
}

/// A call with media attached.
pub struct MediaAttachment<C> {
    pub engine: CallEngine,
    /// The own-focus connection; `None` for a receive-only call, which only
    /// connects to its peers' foci (through the engine's pool).
    pub own_connection: Option<C>,
    /// Our transport identity, derived from the join's `member.id`.
    pub own_identity: String,
    /// Subscribed before any key was replayed or connection adopted, so it
    /// holds the `KeyImported` events of keys signalled before media attached.
    pub events: broadcast::Receiver<CallEvent>,
}

#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    /// The call is over, or never had an encryption manager: join again first.
    #[error("{0}")]
    NotJoined(String),
    /// The call publishes on a transport this one cannot serve.
    #[error("{0}")]
    UnsupportedTransport(String),
    /// The own focus refused the connection.
    #[error(transparent)]
    Transport(#[from] TransportError),
}

/// Attach media to `call`: install `identity_mapper` and `key_handler` on its
/// encryption manager, start a [`CallEngine`] over `transport` (which opens
/// connections to every peer's focus), replay the keys signalled so far, and
/// connect the focus the call publishes on.
///
/// The own focus connects here rather than in the engine's pool so a broken
/// SFU fails this call instead of surfacing later as a dead session. On error
/// nothing is left running; leaving the slot stays the caller's.
pub async fn attach_media<B, T>(
    call: &RtcCall<B>,
    transport: Arc<T>,
    key_handler: Arc<MediaKeyHandler>,
    options: AttachOptions,
) -> Result<MediaAttachment<T::Connection>, AttachError>
where
    B: MatrixBackend + 'static,
    T: OwnFocusTransport + 'static,
{
    let (room_id, slot_id) = (call.room_id().to_owned(), call.slot_id().to_owned());
    if !call.is_live() {
        return Err(AttachError::NotJoined(format!(
            "{room_id}/{slot_id} is over — join the slot again first"
        )));
    }
    let own_key = match call.transport() {
        TransportIntent::Publish(published) => Some(
            transport
                .connection_key(published, &options.own_user_id)
                .ok_or_else(|| {
                    AttachError::UnsupportedTransport(format!(
                        "the call publishes on {published:?}, which is not a {} transport",
                        transport.transport_type(),
                    ))
                })?,
        ),
        TransportIntent::ReceiveOnly { .. } => None,
    };
    log::info!(
        "media: attaching to [{room_id}/{slot_id}] user={} device={} focus={}",
        options.own_user_id,
        options.own_device_id,
        own_key.as_deref().unwrap_or("none (receive only)"),
    );

    // The `member.id` is the join's rather than the host's: our transport
    // identity derives from it, so a value disagreeing with the published
    // membership would put our media on an identity no peer holds a key for.
    let member_id = call.member_id().to_owned();
    let memberships = call.subscribe_memberships().await;
    let raised_hands = call.subscribe_raised_hands().await;
    let reactions = call.subscribe_reactions().await;
    let ended = call.subscribe_ended();
    let signalling = call.subscribe_signalling_health().await;
    let own_membership_lapsed = call.subscribe_own_membership_lapsed().await;

    // Mapper before handler: identities are derived at signal time, and the
    // replay below derives them too, so a handler installed first would import
    // peer keys under the raw `member_id` fallback — an identity the SFU never
    // uses, which is indistinguishable from importing nothing.
    call.set_encryption_identity_mapper(options.identity_mapper.clone())
        .await;
    if !call
        .set_encryption_signal_handler(key_handler.clone())
        .await
    {
        return Err(AttachError::NotJoined(format!(
            "{room_id}/{slot_id} has no encryption manager — join the slot first"
        )));
    }

    let ctx = ConnectionContext {
        room_id: room_id.clone(),
        slot_id: call.format().token_slot_id(&slot_id).into_owned(),
        member: OwnMemberClaims {
            member_id: member_id.clone(),
            user_id: options.own_user_id.clone(),
            device_id: options.own_device_id.clone(),
        },
        e2ee: call.encrypts_media().await,
    };
    let pooled: Arc<dyn MediaTransport> = transport.clone();
    let engine = CallEngine::new(
        EngineConfig {
            transports: vec![pooled],
            own_member_id: member_id.clone(),
            ctx: ctx.clone(),
            own_connection_key: own_key.clone(),
            raised_hands,
            reactions,
            ended: Some(ended),
            media_status: Some(call.media_status_reporter()),
            stability: options.stability,
            signalling: Some(signalling),
            own_membership_lapsed,
            own_reconnect_budget: options.own_reconnect_budget,
            sfu_fallback: options.sfu_fallback,
        },
        memberships,
    );
    let events = engine.subscribe_events();

    // Imported keys surface as `CallEvent::KeyImported`, refused ones as
    // `CallEvent::KeyDiscarded` — the only way the reason a key was rejected
    // leaves the core; without it a host sees a `MissingKey` it cannot tell
    // from a key that never arrived.
    let engine_handle = engine.handle();
    key_handler.set_key_import_listener(Box::new(move |key| {
        engine_handle.notify_key_imported(key.rtc_backend_identity.clone(), key.key_index);
    }));
    let engine_handle = engine.handle();
    key_handler.set_key_discard_listener(Box::new(move |discarded| {
        engine_handle.notify_key_discarded(discarded);
    }));

    // Keys signalled between the join and now — our own first key, peer keys
    // already pumped in — were applied but heard by no listener. After the
    // listeners, so a host sees `KeyImported` for exactly the keys it is most
    // likely to be missing; before the connect, so the ring is populated before
    // the first frame can arrive. Idempotent at the ring, and it honours what
    // remains of a rotation's `delayBeforeUse`.
    if !call.replay_encryption_keys().await {
        log::warn!(
            "media: [{room_id}/{slot_id}] could not replay held keys; peers may stay \
             undecryptable until the next rotation",
        );
    }

    // Derived as the core derives it for our own key: from the transport we
    // publish on, falling back the same way when nothing is published.
    let own_transports = match call.transport() {
        TransportIntent::Publish(published) => std::slice::from_ref(published),
        TransportIntent::ReceiveOnly { .. } => &[],
    };
    let own_identity = (options.identity_mapper)(&IdentitySubject {
        user_id: &options.own_user_id,
        device_id: Some(&options.own_device_id),
        member_id: &member_id,
        transports: own_transports,
    })
    .unwrap_or_else(|| format!("{}:{}", options.own_user_id, options.own_device_id));
    let own_connection = match &own_key {
        Some(own_key) => {
            // Not awaited: the join already has the user in the call, and our
            // own focus connects, or is given up, within the reconnect budget
            // like any loss (034 R24, contract C10). The status stays
            // connecting until it is up.
            let connection = transport.prepare_own(own_key, &ctx);
            engine.connect_own(Box::new(connection.clone()));

            // Move our sender onto each key we rotate to. Without this we
            // advertise a rotation to peers and keep encrypting with the
            // previous key, so anyone joining after it decrypts nothing — and
            // the forward secrecy the rotation exists for is not delivered.
            // After the replay, so the first key is already in the ring; the
            // hook only ever *moves* the index, and the handle re-asserts it
            // on every connection it makes.
            let for_keys = connection.clone();
            key_handler.set_local_sender(
                own_identity.clone(),
                Box::new(move |key_index| for_keys.set_local_key_index(key_index)),
            );
            // Adopt the index we are already on rather than assuming 0: a
            // rotation between the join and here would otherwise be missed.
            if let Some(own) = key_handler.key_for(&own_identity) {
                connection.set_local_key_index(own.key_index);
            }
            Some(connection)
        }
        None => {
            // Nothing of ours to connect: a receive-only call is up once
            // attached.
            call.media_status_reporter().connected();
            None
        }
    };

    log::info!("media: attached as member {member_id}, local identity {own_identity}");
    Ok(MediaAttachment {
        engine,
        own_connection,
        own_identity,
        events,
    })
}

#[cfg(test)]
mod tests;
