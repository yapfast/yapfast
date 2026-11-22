// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! Native LiveKit media on a joined call: the MSC4195 key provider, the
//! [`LiveKitMediaTransport`] over it, and [`attach_media`](crate::attach_media)'s
//! ordered wiring. Shared by the `LiveKitCall` facade and the FFI media session.

use std::sync::Arc;

use crate::{AttachError, AttachOptions, MediaAttachment, StabilityConfig, attach_media};
use matrix_rtc_call::RtcCall;
use matrix_rtc_core::MatrixBackend;

use matrix_rtc_livekit::{
    LiveKitMediaTransport, LiveKitTransportConnection, MediaKeyBridge, Msc4195KeyProviders,
    identity_mapper, msc4195_media_key_bridge,
};

/// What [`attach_livekit`] needs besides the call.
#[derive(Clone, Debug)]
pub struct LiveKitAttachOptions {
    /// HTTP client for the token exchange; `None` builds a default one.
    pub http: Option<reqwest::Client>,
    /// `false` never subscribes to peers' media; see
    /// [`LiveKitMediaTransport::with_auto_subscribe`].
    pub auto_subscribe: bool,
    pub stability: StabilityConfig,
    /// How long our own focus may stay unreachable before the call ends
    /// (034 R24); `None` takes
    /// [`OWN_RECONNECT_BUDGET`](matrix_rtc_transport::pool::OWN_RECONNECT_BUDGET).
    pub own_reconnect_budget: Option<std::time::Duration>,
    /// Whether our media may move to a peer's SFU while ours is down
    /// (034 R25); see [`AttachOptions::sfu_fallback`].
    pub sfu_fallback: bool,
}

/// A call with native LiveKit media attached.
pub struct LiveKitAttachment {
    pub media: MediaAttachment<LiveKitTransportConnection>,
    /// The key handler installed on the call, over the connections' shared key
    /// provider.
    pub key_bridge: Arc<MediaKeyBridge>,
}

/// Attach native LiveKit media to `call`, joined on `backend` in
/// `options.format`.
///
/// One key provider serves every SFU connection, own and peers': keys are
/// indexed by participant identity, which is globally unique per membership.
pub async fn attach_livekit<B: MatrixBackend + 'static>(
    call: &RtcCall<B>,
    backend: Arc<dyn MatrixBackend>,
    options: LiveKitAttachOptions,
) -> Result<LiveKitAttachment, AttachError> {
    // One mapper for the core, the transport, our own identity and the ring.
    let mapper = identity_mapper();
    let providers = Msc4195KeyProviders::new();
    let key_bridge = Arc::new(msc4195_media_key_bridge(providers.clone()));
    let (own_user_id, own_device_id) = (backend.own_user_id(), backend.own_device_id());
    let transport = Arc::new(
        LiveKitMediaTransport::new(options.http.unwrap_or_default(), backend, providers)
            .with_auto_subscribe(options.auto_subscribe)
            .with_identity_mapper(mapper.clone()),
    );
    let media = attach_media(
        call,
        transport,
        key_bridge.clone(),
        AttachOptions {
            own_user_id,
            own_device_id,
            identity_mapper: mapper,
            stability: options.stability,
            own_reconnect_budget: options
                .own_reconnect_budget
                .unwrap_or(matrix_rtc_transport::pool::OWN_RECONNECT_BUDGET),
            sfu_fallback: options.sfu_fallback,
        },
    )
    .await?;
    Ok(LiveKitAttachment { media, key_bridge })
}
