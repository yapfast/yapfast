// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! High-level "join a call" facade over the whole stack.
//!
//! [`LiveKitCall::join`] wires together everything a MatrixRTC participant needs —
//! an [`RtcClient`] over a [`SdkMatrixBackend`], the room it opens and the call
//! joined in it, the MSC4195 token exchange, and an E2EE-enabled SFU
//! connection driven through the transport-agnostic [`matrix_rtc_call_sdk`]
//! layer. [`LiveKitCall::leave`] tears
//! all of it down in the right order.
//!
//! Consume the call through the unified stream
//! ([`LiveKitCall::subscribe_call_events`]) and the [`LiveKitCall::participants`] roster;
//! the raw LiveKit accessors ([`LiveKitCall::events`], [`LiveKitCall::session`]) remain for
//! the transition and will go away once frame streams cover their uses.
//!
//! Requires the `matrix-sdk` feature.
//!
//! # Runtime requirements
//!
//! [`LiveKitCall::join`] must be called from within a tokio runtime: the
//! library spawns the room's feeds and the session's keep-alive onto the
//! current one (`matrix_rtc_core::executor`). See `examples/join_and_record.rs`
//! for the runtime skeleton.
//!
//! # Preconditions
//!
//! - the client is logged in and syncing (e.g. `matrix_sdk_ui::sync_service::SyncService`
//!   is running — it enables the sticky-events extension the feeder relies
//!   on);
//! - the user has joined `room`;
//! - the slot is open (an `m.rtc.slot` state event; see [`open_slot`]) —
//!   MSC4143 counts nobody as joined against a closed slot.

use std::sync::Arc;

use livekit::RoomEvent;
use matrix_sdk::{Client, Room};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{broadcast, watch};

use crate::{CallEngine, CallEvent, MediaAttachment, Participant, StabilityConfig};
use matrix_rtc_call::{
    CallJoinOptions, CallKind, CallStatus, JoinOptions, JoinTransport, RaisedHand, ReactionError,
    ReactionsConfig, RtcCall, RtcClient, RtcError, RtcRoom,
};
use matrix_rtc_core::RoomOptions;
use matrix_rtc_core::compat::{self, MembershipFormat};
use matrix_rtc_core::{
    BaseRtcRoom, EncryptionConfig, LeaveError, LiveKitTransport, SlotEncryption,
};
use matrix_rtc_matrix_sdk::SdkMatrixBackend;
use matrix_rtc_transport::{
    LocalTrackHandle, MediaConstraints, MediaStreamKind, PublishOptions, ReceiveStats,
    RemoteTrackHandle,
};

use matrix_rtc_livekit::{LiveKitSession, LiveKitTransportConnection, MediaKeyBridge};

use super::{LiveKitAttachOptions, LiveKitAttachment, attach_livekit};

/// Errors produced when joining, operating, or leaving a [`LiveKitCall`].
#[derive(Debug, thiserror::Error)]
pub enum LiveKitCallError {
    /// A Matrix client error (login state, room access, ...).
    #[error(transparent)]
    Sdk(#[from] matrix_sdk::Error),

    /// A LiveKit transport error (token exchange, SFU connection).
    #[error(transparent)]
    Transport(#[from] matrix_rtc_livekit::Error),

    /// A media transport error surfaced through the media layer.
    #[error(transparent)]
    Media(#[from] matrix_rtc_transport::TransportError),

    /// Attaching media to the joined call failed.
    #[error(transparent)]
    Attach(#[from] crate::AttachError),

    /// MatrixRTC signalling through the core failed (membership, slot, keys).
    #[error("MatrixRTC signalling failed: {0}")]
    Signalling(String),

    /// A reaction or raised hand could not be sent (see
    /// [`matrix_rtc_call::reactions`]).
    #[error(transparent)]
    Reaction(#[from] ReactionError),
}

fn signalling_error(error: impl std::fmt::Display) -> LiveKitCallError {
    LiveKitCallError::Signalling(error.to_string())
}

impl From<RtcError> for LiveKitCallError {
    fn from(error: RtcError) -> Self {
        match error {
            RtcError::Reaction(error) => LiveKitCallError::Reaction(error),
            error => signalling_error(error),
        }
    }
}

/// Options for [`LiveKitCall::join`]. `LiveKitCallOptions::default()` matches the common
/// case: the `m.call#room` slot of the `m.call` application, transport
/// discovery via the homeserver, and the core's default encryption policy.
#[derive(Clone, Debug)]
pub struct LiveKitCallOptions {
    /// MatrixRTC slot to join.
    pub slot_id: String,
    /// MatrixRTC application of the slot.
    pub application: String,
    /// The LiveKit transport to publish on, overriding the homeserver's
    /// advertised transports (MSC4143 `GET /rtc/transports`). `None` takes the
    /// first LiveKit one advertised; joining fails if there is none.
    pub livekit_transport: Option<LiveKitTransport>,
    /// Override for the core's media-key policy. `None` keeps the core's
    /// default, which requires key senders to be cross-signed (MSC4153) —
    /// only relax this for test setups whose users have no cross-signing.
    pub encryption_config: Option<EncryptionConfig>,
    /// How often to refresh the dead man's switch delayed leave; `None` is
    /// [`DEFAULT_KEEP_ALIVE_INTERVAL_MS`](matrix_rtc_core::DEFAULT_KEEP_ALIVE_INTERVAL_MS).
    pub keep_alive_interval_ms: Option<u64>,
    /// How long the homeserver keeps our membership in the sticky map. `None`
    /// keeps the core's default of an hour.
    ///
    /// This is what governs how long a **crashed** client lingers as a ghost.
    /// The dead man's switch does not help there: its delayed leave is a plain
    /// event, so it never replaces the sticky entry, and the membership stands
    /// until this elapses. A tool that expects to be killed — a load generator,
    /// a test — wants it short.
    ///
    /// Not free: the keep-alive re-sends the membership once a third of it has
    /// passed, so halving this doubles that signalling rate. Keep it well
    /// above twice [`keep_alive_interval_ms`](Self::keep_alive_interval_ms), or the
    /// entry can lapse between ticks.
    pub sticky_duration_ms: Option<u64>,
    /// The membership lifetime to publish instead of
    /// [`sticky_duration_ms`](Self::sticky_duration_ms) when the homeserver
    /// refuses to arm a delayed leave. `None` keeps the core's default of five
    /// minutes, which is also the floor MSC4354 states.
    ///
    /// Only ever reached on a homeserver without MSC4140, where the join
    /// degrades rather than failing. Subject to the same rule as
    /// `sticky_duration_ms`: keep it well above twice
    /// [`keep_alive_interval_ms`](Self::keep_alive_interval_ms).
    pub degraded_lifetime_ms: Option<u64>,
    /// HTTP client used for the token exchange with the authorisation
    /// service. Supply one to control TLS behaviour (e.g. self-signed dev
    /// certs); `None` builds a default client.
    pub http: Option<reqwest::Client>,
    /// Whether to subscribe to peers' media. `false` joins publish-only: the
    /// roster still fills from membership signalling, but no remote track is
    /// ever subscribed, so [`CallEvent::StreamStarted`] and
    /// [`LiveKitCall::remote_track`] never produce anything. Only a load generator
    /// wants this.
    pub auto_subscribe: bool,
    /// Render this call for an older MatrixRTC generation, for interoperating
    /// with Element Call builds that have not caught up with the 2026 MSC4143
    /// rewrite.
    ///
    /// [`MembershipFormat::RoomState`] is not additive at all: the membership moves to `org.matrix.msc3401.call.member` room state,
    /// the SFU participant identity becomes the plain `{user}:{device}` string,
    /// and the token comes from the pre-MSC4195 `/sfu/get` endpoint. Nothing
    /// about such a call is visible to a spec-current peer.
    ///
    /// Element Call's 2025 sticky dialect is read in the room-state format
    /// only. See [`matrix_rtc_core::compat`].
    pub format: MembershipFormat,
    /// A group call, with or without a notification, or a DM call, with or
    /// without a ring. The default is a quiet group join.
    pub kind: CallKind,
    /// How this call handles Element Call reactions and the raised hand.
    ///
    /// `None` — the default — is [`ReactionsConfig::default`]: enabled, with
    /// Element Call's three-second window. Reactions arrive as
    /// [`CallEvent::Reaction`], hands as [`CallEvent::HandRaised`] and on the
    /// roster; send with [`LiveKitCall::send_reaction`], [`LiveKitCall::raise_hand`] and
    /// [`LiveKitCall::lower_hand`].
    pub reactions: Option<ReactionsConfig>,
    /// How much the tile order is damped: how long sustained voice must last
    /// to count as speaking, how long silence must last to stop, and the
    /// window reorders are coalesced into. A product decision rather than a
    /// protocol one; see [`StabilityConfig`].
    pub stability: StabilityConfig,
    /// Whether our media may move to another member's SFU while ours is
    /// down (034 R25); see
    /// [`AttachOptions::sfu_fallback`](crate::AttachOptions::sfu_fallback).
    /// Off by default.
    pub sfu_fallback: bool,
}

impl Default for LiveKitCallOptions {
    fn default() -> Self {
        Self {
            slot_id: "m.call#room".to_owned(),
            application: "m.call".to_owned(),
            livekit_transport: None,
            encryption_config: None,
            keep_alive_interval_ms: None,
            sticky_duration_ms: None,
            degraded_lifetime_ms: None,
            http: None,
            auto_subscribe: true,
            format: MembershipFormat::default(),
            kind: CallKind::default(),
            reactions: None,
            stability: StabilityConfig::default(),
            sfu_fallback: false,
        }
    }
}

/// A joined MatrixRTC call: live membership signalling plus an E2EE SFU
/// connection.
///
/// Obtained from [`LiveKitCall::join`]; end it with [`LiveKitCall::leave`]. Dropping a
/// `LiveKitCall` without leaving stops the background tasks and the sync-side key
/// handler, but sends no leave event — peers then see this member disappear
/// only when the dead man's switch fires.
pub struct LiveKitCall {
    /// Keeps itself alive and rotates its keys while it lives.
    call: Arc<RtcCall<SdkMatrixBackend>>,
    /// Holds the room's subscription; dropping it detaches.
    room: Arc<RtcRoom<SdkMatrixBackend>>,
    engine: CallEngine,
    connection: LiveKitTransportConnection,
    raw_events: UnboundedReceiver<RoomEvent>,
    bridge: Arc<MediaKeyBridge>,
    own_identity: String,
}

/// Keeps signalling reachable after a join future is cancelled.
#[derive(Default)]
struct JoinCleanup {
    room: Option<Arc<RtcRoom<SdkMatrixBackend>>>,
    call: Option<Arc<RtcCall<SdkMatrixBackend>>>,
}

impl LiveKitCall {
    /// Join the MatrixRTC call on `room` and connect to the SFU with
    /// per-participant frame E2EE.
    ///
    /// Publishes this device's `m.rtc.member` membership as a sticky event,
    /// arms the dead man's switch delayed leave (kept alive by the session),
    /// starts distributing/ingesting media keys over Olm-encrypted
    /// to-device messages, discovers the LiveKit transport, and connects.
    ///
    /// Must run inside a tokio runtime; see the module docs for this and the
    /// other preconditions.
    pub async fn join(
        room: &Room,
        options: LiveKitCallOptions,
    ) -> Result<LiveKitCall, LiveKitCallError> {
        Self::join_inner(room, options, None).await
    }

    /// Cancel by sending true; keep polling until partial membership cleanup completes.
    pub async fn join_cancellable(
        room: &Room,
        options: LiveKitCallOptions,
        mut cancellation: watch::Receiver<bool>,
    ) -> Result<Option<Self>, LiveKitCallError> {
        if *cancellation.borrow() {
            return Ok(None);
        }
        let cleanup = Arc::new(std::sync::Mutex::new(JoinCleanup::default()));
        let result = {
            let joining = Self::join_inner(room, options, Some(cleanup.clone()));
            tokio::pin!(joining);
            tokio::select! {
                biased;
                _ = async {
                    loop {
                        if *cancellation.borrow_and_update() || cancellation.changed().await.is_err() { break; }
                    }
                } => None,
                result = &mut joining => Some(result),
            }
        };
        match result {
            Some(Ok(call)) => Ok(Some(call)),
            other => {
                let JoinCleanup { room, call } =
                    std::mem::take(&mut *cleanup.lock().expect("join cleanup lock"));
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    if let Some(call) = call {
                        match call.leave(Default::default()).await {
                            Ok(())
                            | Err(RtcError::SessionOver | RtcError::Leave(LeaveError::NotJoined)) =>
                                {}
                            Err(error) => return Err(LiveKitCallError::from(error)),
                        }
                    }
                    if let Some(room) = room {
                        let room = Arc::try_unwrap(room)
                            .map_err(|_| signalling_error("cancelled join retained its room"))?;
                        room.close().await;
                    }
                    Ok::<_, LiveKitCallError>(())
                })
                .await
                .map_err(|_| signalling_error("cancelled join departure timed out"))??;
                match other {
                    Some(Err(error)) => Err(error),
                    None => Ok(None),
                    Some(Ok(_)) => unreachable!(),
                }
            }
        }
    }

    async fn join_inner(
        room: &Room,
        options: LiveKitCallOptions,
        cleanup: Option<Arc<std::sync::Mutex<JoinCleanup>>>,
    ) -> Result<Self, LiveKitCallError> {
        let client = room.client();
        let user_id = client
            .user_id()
            .ok_or_else(|| {
                LiveKitCallError::Signalling("client has no user id (not logged in)".into())
            })?
            .to_string();
        let device_id = client
            .device_id()
            .ok_or_else(|| {
                LiveKitCallError::Signalling("client has no device id (not logged in)".into())
            })?
            .to_string();
        let room_id = room.room_id().to_string();

        match options.format {
            MembershipFormat::Msc => {}
            MembershipFormat::RoomState => log::warn!(
                "[{room_id}/{}] joining in pre-sticky Element Call compatibility mode: our \
                 membership goes out as {} room state, our SFU identity is the plain \
                 {{user}}:{{device}} string, and the token comes from /sfu/get. Nothing about \
                 this call is visible to a spec-current peer.",
                options.slot_id,
                compat::STATE_MEMBER_EVENT_TYPE,
            ),
        }

        // One client for this call: it opens the room (the feeder subscribes to
        // what the mode needs and seeds room state before membership) and the
        // to-device key subscription; the library runs both feeds.
        let client = RtcClient::new(Arc::new(SdkMatrixBackend::new(client.clone())));
        let room = client
            .room(
                room_id.clone(),
                RoomOptions {
                    format: options.format,
                },
            )
            .await?;
        let room = Arc::new(room);
        if let Some(cleanup) = &cleanup {
            cleanup.lock().expect("join cleanup lock").room = Some(room.clone());
        }
        room.seeded().await;

        // The join picks the `member.id` this mode joins with (a fresh one per
        // join, except in the pre-sticky generation; see
        // `compat::ingest::member_id`) and renders our sends in the room's
        // dialect.
        let mut join = JoinOptions {
            slot_id: options.slot_id.clone(),
            ..JoinOptions::application(options.application.clone())
        };
        // The join's own choice, else the first LiveKit focus the homeserver
        // advertises; media reads it back from the call.
        join.transport = options
            .livekit_transport
            .clone()
            .map_or(JoinTransport::Advertised, JoinTransport::Publish);
        join.encryption_config = options.encryption_config.clone();
        join.sticky_duration_ms = options.sticky_duration_ms;
        join.degraded_lifetime_ms = options.degraded_lifetime_ms;
        join.keep_alive_interval_ms = options.keep_alive_interval_ms;
        let call = Arc::new(
            room.join_call(CallJoinOptions {
                join,
                kind: options.kind.clone(),
                reactions: options.reactions.clone(),
                // Follows the call's voice or video intent; video when it
                // starts one.
                intent: None,
            })
            .await?,
        );
        if let Some(cleanup) = &cleanup {
            cleanup.lock().expect("join cleanup lock").call = Some(call.clone());
        }
        let membership_id = call.member_id().to_owned();
        log::info!(
            "[{room_id}/{}] join: user={user_id} device={device_id} member={membership_id}",
            options.slot_id,
        );

        // The media layer: the engine reconciling memberships with connection
        // events, and the own focus connected synchronously so a failed join
        // can be reported (and signalled away) immediately.
        let attached = attach_livekit(
            &call,
            room.backend().clone(),
            LiveKitAttachOptions {
                http: options.http.clone(),
                auto_subscribe: options.auto_subscribe,
                stability: options.stability.clone(),
                own_reconnect_budget: None,
                sfu_fallback: options.sfu_fallback,
            },
        )
        .await;
        let attached = match attached {
            Ok(LiveKitAttachment {
                media:
                    MediaAttachment {
                        engine,
                        own_connection: Some(connection),
                        own_identity,
                        ..
                    },
                key_bridge,
            }) => Ok((engine, connection, own_identity, key_bridge)),
            // The join publishes on a LiveKit focus, so attaching connects it.
            Ok(_) => Err(LiveKitCallError::Signalling(
                "the joined call publishes on no LiveKit focus".into(),
            )),
            Err(error) => Err(error.into()),
        };
        let (engine, connection, own_identity, bridge) = match attached {
            Ok(attached) => attached,
            Err(error) => {
                log::warn!(
                    "[{room_id}/{}] join: attaching media failed ({error}); leaving the slot again",
                    options.slot_id,
                );
                // We are signalled as joined but have no media path; leave so
                // peers don't wait on the dead man's switch to notice.
                if let Err(leave_error) = call.leave(Default::default()).await {
                    log::warn!("leave after failed media attach also failed: {leave_error}");
                }
                return Err(error);
            }
        };

        log::info!(
            "[{room_id}/{}] join: complete, identity {own_identity}",
            options.slot_id
        );

        // This facade hands out the connected room, so it waits for our own
        // focus, which the attach no longer does (034 R24); a focus given up
        // has ended the call, and its leave is already issued.
        let mut status = call.subscribe_status();
        let session = tokio::select! {
            session = connection.wait_connected() => session,
            _ = status.wait_for(|status| matches!(status, CallStatus::Ended { .. })) => {
                return Err(LiveKitCallError::Signalling(format!(
                    "our own focus was given up: {:?}",
                    call.status(),
                )));
            }
        };

        // Transition-period raw stream; subscribed immediately after connect,
        // so only events racing the connect itself can be missed here.
        let raw_events = session.room().subscribe();

        Ok(LiveKitCall {
            call,
            room,
            engine,
            connection,
            raw_events,
            bridge,
            own_identity,
        })
    }

    /// Sends an Element Call emoji reaction. `name` is what peers pick a sound
    /// by (see [`matrix_rtc_call::KNOWN_REACTIONS`]); only the first grapheme
    /// of `emoji` is sent. Returns the event id.
    ///
    /// Fails with [`ReactionError::Cooldown`] inside the send cooldown, since
    /// peers would drop the reaction anyway.
    pub async fn send_reaction(&self, emoji: &str, name: &str) -> Result<String, LiveKitCallError> {
        Ok(self.call.send_reaction(emoji, name).await?)
    }

    /// Raises our hand. Idempotent while it is up; it follows our membership
    /// across sticky refreshes on its own. Shows on our roster entry at once.
    pub async fn raise_hand(&self) -> Result<(), LiveKitCallError> {
        Ok(self.call.raise_hand().await?)
    }

    /// Lowers our hand by redacting the annotation. A no-op when it is down.
    pub async fn lower_hand(&self) -> Result<(), LiveKitCallError> {
        Ok(self.call.lower_hand().await?)
    }

    /// The raised hands right now, oldest first. The same information is on
    /// each [`Participant::hand_raised_at_ms`] and arrives as
    /// [`CallEvent::HandRaised`] / [`CallEvent::HandLowered`].
    pub async fn raised_hands(&self) -> Vec<RaisedHand> {
        self.call.raised_hands().await
    }

    /// Where the call is: connecting, calling, connected or ended, and why.
    pub fn status(&self) -> CallStatus {
        self.call.status()
    }

    /// The call's status as it changes, current value first.
    pub fn subscribe_status(&self) -> watch::Receiver<CallStatus> {
        self.call.subscribe_status()
    }

    /// The unified call event stream: membership changes, media streams
    /// starting/stopping, key imports and the like.
    ///
    /// This is the transport-agnostic replacement for [`LiveKitCall::events`]. Any
    /// number of subscribers may exist; a subscriber that falls far behind
    /// observes a `Lagged` error and should resynchronise from
    /// [`LiveKitCall::participants`].
    pub fn subscribe_call_events(&self) -> broadcast::Receiver<CallEvent> {
        self.engine.subscribe_events()
    }

    /// The current participant roster (including ourselves), derived from
    /// membership signalling and enriched with live media streams.
    pub fn participants(&self) -> Vec<Participant> {
        self.engine.participants()
    }

    /// Watch the participant roster; the receiver always holds the latest
    /// snapshot.
    pub fn subscribe_participants(&self) -> watch::Receiver<Vec<Participant>> {
        self.engine.subscribe_participants()
    }

    /// The frame-stream handle for a participant's subscribed stream, once
    /// [`CallEvent::StreamStarted`] announced it.
    pub fn remote_track(
        &self,
        member_id: &str,
        kind: MediaStreamKind,
    ) -> Option<Arc<dyn RemoteTrackHandle>> {
        self.engine.remote_track(member_id, kind)
    }

    /// Cumulative receive-side RTP counters for a participant's stream, or
    /// `None` while it is not subscribed / before the first RTCP report.
    ///
    /// The only way to distinguish "no RTP arriving" from "RTP arriving that
    /// does not decode": the receive path produces frames at a fixed cadence
    /// either way. See [`ReceiveStats`].
    pub async fn receive_stats(
        &self,
        member_id: &str,
        kind: MediaStreamKind,
    ) -> Option<ReceiveStats> {
        self.engine.receive_stats(member_id, kind).await
    }

    /// Publish a local track (microphone, camera, screenshare) on our focus;
    /// push captured frames into the returned handle.
    pub async fn publish(
        &self,
        options: PublishOptions,
    ) -> Result<Arc<dyn LocalTrackHandle>, LiveKitCallError> {
        Ok(self.engine.publish(options).await?)
    }

    /// Publish native audio and move its new cryptor onto the current sender key.
    pub async fn publish_audio_track(
        &self,
        track: livekit::prelude::LocalAudioTrack,
        options: livekit::options::TrackPublishOptions,
    ) -> Result<livekit::prelude::LocalTrackPublication, LiveKitCallError> {
        let publication = self
            .session()
            .room()
            .local_participant()
            .publish_track(livekit::prelude::LocalTrack::Audio(track), options)
            .await
            .map_err(matrix_rtc_livekit::Error::from)?;
        self.reassert_local_key_index();
        Ok(publication)
    }

    /// Reapply the sender key after native publication or LiveKit republication.
    pub fn reassert_local_key_index(&self) {
        self.connection.reassert_local_key_index();
    }

    /// Set subscription constraints (visibility, rendered size, quality cap,
    /// low-bandwidth mode) for one stream of one participant. Applied after a
    /// short debounce and re-applied whenever the stream (re)appears.
    pub fn set_constraints(
        &self,
        member_id: &str,
        kind: MediaStreamKind,
        constraints: MediaConstraints,
    ) {
        self.engine.set_constraints(member_id, kind, constraints);
    }

    /// The media engine driving this call's roster and event stream.
    pub fn engine(&self) -> &CallEngine {
        &self.engine
    }

    /// The raw LiveKit room event stream (participants joining, tracks
    /// subscribed, disconnects, ...).
    ///
    /// Transition API: prefer [`LiveKitCall::subscribe_call_events`]; this accessor
    /// goes away once frame-level consumers are served by [`LiveKitCall::remote_track`].
    ///
    /// The stream ending (`recv()` returning `None`) means the call is over:
    /// the room closes its event channel on [`LiveKitCall::leave`] and after any
    /// unrecoverable disconnect (server eviction, reconnects exhausted, ...) —
    /// in the latter case a [`RoomEvent::Disconnected`] carrying the reason is
    /// delivered first, so match it only if the reason matters. Transient
    /// network drops are resumed internally (`Reconnecting`/`Reconnected`
    /// events) and do not end the stream. There is no built-in deadline:
    /// waiting for an event that may never come (e.g. a track from a peer who
    /// never publishes) should be wrapped in a timeout by the caller.
    pub fn events(&mut self) -> &mut UnboundedReceiver<RoomEvent> {
        &mut self.raw_events
    }

    /// The connected SFU session (access the LiveKit room to publish, ...).
    ///
    /// Transition API: media access moves behind [`LiveKitCall::remote_track`] and
    /// the upcoming publish surface.
    pub fn session(&self) -> Arc<LiveKitSession> {
        self.connection
            .session()
            .expect("connected before the join returned")
    }

    /// Our MSC4195 pseudonymous LiveKit identity (the JWT `sub`). Peers see
    /// this as our participant identity and import our media key under it.
    pub fn local_identity(&self) -> &str {
        &self.own_identity
    }

    /// The `m.rtc.member` membership id of this join.
    pub fn membership_id(&self) -> &str {
        self.call.member_id()
    }

    /// SFU identities keyed by membership, using each publisher's token flow.
    pub async fn participant_identities(&self) -> std::collections::HashMap<String, String> {
        let memberships = self.call.session().subscribe_memberships().await;
        let mapper = matrix_rtc_livekit::identity_mapper();
        memberships
            .borrow()
            .iter()
            .filter_map(|member| {
                mapper(&matrix_rtc_core::IdentitySubject {
                    user_id: &member.sender,
                    device_id: member.origin.sender_device_id(),
                    member_id: &member.member_id,
                    transports: &member.transports,
                })
                .map(|identity| (member.member_id.clone(), identity))
            })
            .collect()
    }

    /// Number of members (including ourselves) currently joined to the slot,
    /// as signalled over sticky membership events.
    pub async fn member_count(&self) -> usize {
        self.call.member_count().await
    }

    /// Whether a media key for the given MSC4195 participant identity has been
    /// received and imported into this call's frame decryptor. See
    /// [`LiveKitCall::local_identity`] for the identity peers know us by.
    pub fn imported_key_for(&self, identity: &str) -> bool {
        self.bridge.key_for(identity).is_some()
    }

    /// Leave the call cleanly: send the leave event (cancelling the delayed
    /// leave) and close the SFU connection.
    ///
    /// The SFU connection is closed even if the Matrix-side leave fails; the
    /// first error wins.
    ///
    /// A call whose slot closed has already left and ended itself
    /// ([`CallStatus::Ended`] with `CallEnd::SlotClosed`); leaving it then only
    /// releases what is left, and succeeds.
    pub async fn leave(self) -> Result<(), LiveKitCallError> {
        let LiveKitCall {
            call,
            room,
            engine,
            connection,
            ..
        } = self;
        let room_id = room.room_id().to_owned();

        // Step logs bracket every await so a wedged teardown pinpoints itself.
        log::debug!("[{room_id}] leave: sending matrix leave (membership + delayed-event cancel)");
        let leave_result = call.leave(Default::default()).await;
        // The only way a `LiveKitCall` is over before its leave: the slot
        // closed, the core left, and the engine closed the SFU connection.
        // `NotJoined` is the same, caught before the session heard of it.
        let auto_left = matches!(
            leave_result,
            Err(RtcError::SessionOver | RtcError::Leave(LeaveError::NotJoined))
        );
        let leave_result = if auto_left {
            log::debug!("[{room_id}] leave: the call already left on its own");
            Ok(())
        } else {
            leave_result.map_err(LiveKitCallError::from)
        };
        // After the leave, whose cancel of the delayed event still goes through
        // the backend; the subscriptions are not needed for that.
        drop(room);
        log::debug!(
            "[{room_id}] leave: matrix leave {}; shutting down the media engine",
            if leave_result.is_ok() {
                "sent"
            } else {
                "FAILED"
            },
        );
        // The leave ended the engine; this waits for it to finish closing.
        engine.shutdown().await;
        log::debug!("[{room_id}] leave: media engine down; closing own SFU connection");
        use matrix_rtc_transport::TransportConnection as _;
        let close_result = match connection.close().await {
            // Already closed by the engine; nothing for the caller to act on.
            Err(error) if auto_left => {
                log::debug!("[{room_id}] leave: own SFU connection already gone: {error}");
                Ok(())
            }
            result => result.map_err(LiveKitCallError::from),
        };
        log::debug!("[{room_id}] leave: complete");
        leave_result.and(close_result)
    }
}

/// Close a MatrixRTC slot in a room by publishing an empty `m.rtc.slot` state
/// event. Everyone in it — this client's own calls included — leaves, and
/// their calls end with `CallEnd::SlotClosed`.
///
/// Requires the power level for `m.rtc.slot` state, as [`open_slot`] does.
pub async fn close_slot(
    client: &Client,
    room_id: &str,
    slot_id: &str,
) -> Result<(), LiveKitCallError> {
    BaseRtcRoom::with_backend(room_id, Arc::new(SdkMatrixBackend::new(client.clone())))
        .close_slot(slot_id.to_owned())
        .await
        .map_err(signalling_error)
}

/// Open a MatrixRTC slot in a room by publishing its `m.rtc.slot` state event.
///
/// Requires the power level for `m.rtc.slot` state (by default the room
/// creator). Passing `None` for `encryption` opens an unencrypted slot; calls
/// in encrypted rooms should use `m.per_member` slot encryption.
pub async fn open_slot(
    client: &Client,
    room_id: &str,
    slot_id: &str,
    application: &str,
    encryption: Option<SlotEncryption>,
) -> Result<(), LiveKitCallError> {
    BaseRtcRoom::with_backend(room_id, Arc::new(SdkMatrixBackend::new(client.clone())))
        .open_slot(slot_id.to_owned(), application.to_owned(), encryption)
        .await
        .map_err(signalling_error)
}
