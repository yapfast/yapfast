// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! MSC4195 LiveKit transport for MatrixRTC.
//!
//! This crate is the "LiveKit SDK" layer that turns the membership and key
//! outputs of [`matrix-rtc-core`] into a live SFU media session. It is
//! deliberately a **native-only** crate (the LiveKit client pulls in
//! `libwebrtc`) and is responsible for:
//!
//! - obtaining a LiveKit JWT through the flow the transport selects: the
//!   legacy JWT service's `/sfu/get` ([`token`]) or the homeserver's MSC4195
//!   `get_token`, through the host's backend;
//! - deriving the MSC4195 `livekit_alias` and participant identities
//!   ([`identity`]);
//! - connecting to the SFU with per-participant GCM frame E2EE ([`session`],
//!   [`connect_e2ee`]);
//! - bridging `matrix-rtc-core`'s media keys into LiveKit frame encryption
//!   ([`keys`]).
//!
//! It implements `matrix-rtc-transport`'s contract
//! ([`LiveKitMediaTransport`]) and knows no call: the call's roster, attaching
//! this transport to a joined call, and the `LiveKitCall` join/leave facade
//! are `matrix-rtc-call-sdk`'s, behind its `livekit` and `matrix-sdk`
//! features — start there.
//!
//! [`matrix-rtc-core`]: matrix_rtc_core

// The pure control plane — hash derivations, token shapes, dialect choices —
// lives in `matrix_rtc_transport::livekit`, which the web binding shares; this crate
// re-exports it under the paths it always had.
pub use matrix_rtc_transport::livekit::identity;
pub use matrix_rtc_transport::livekit::{LiveKitTarget, LiveKitTransportConfig, identity_mapper};

pub mod keys;
// Audio helpers. Recording a subscribed track and writing WAVs is shipped API
// (the recording-bot use case); the synthetic-tone generator and frequency
// detector inside are test utilities gated on `cfg(test)`/the `testing` feature.
pub mod media;
pub mod session;
pub mod token;
pub mod transport_impl;
pub mod video_encoding;

pub use keys::{
    ConnectionKeyProvider, KeyDiscardListener, KeyImportListener, LocalKeyIndexHook,
    MediaKeyBridge, Msc4195KeyProviders, NATIVE_KEY_RING_MAX, ParticipantKey,
    SwitchCompleteListener, msc4195_key_provider, msc4195_key_provider_options,
    msc4195_media_key_bridge,
};
pub use session::{LiveKitConnection, LiveKitSession};
pub use token::SfuToken;
// The OpenID token is a Client-Server API concern, so it belongs to the bridge.
// Re-exported because `LiveKitTransportConfig` and `connect`/`connect_e2ee` name
// these in this crate's own signatures — a host implementing the token source
// should not need a second dependency to do it.
pub use matrix_rtc_core::{LiveKitTokenRequest, MatrixBackend, OpenIdToken};
pub use transport_impl::{LiveKitMediaTransport, LiveKitTransportConnection};
pub use video_encoding::supported_video_codecs;

/// Android initialisation, re-exported so consumers (e.g. the FFI crate)
/// don't need a direct `livekit`/`libwebrtc` dependency for it.
#[cfg(target_os = "android")]
pub mod android {
    /// Initialise libwebrtc's JVM hooks. Must run before any peer
    /// connection is created — typically from `JNI_OnLoad`.
    pub fn initialize_android(vm: &jni::JavaVM) {
        livekit::webrtc::android::initialize_android(vm);
    }
}

/// Obtain an SFU token through the flow the target's transport selects: an
/// OpenID token exchanged at the legacy JWT service's `/sfu/get`, or the
/// homeserver's MSC4195 `get_token` through the host.
///
/// The one place the flow is acted on, so the two `connect` entry points
/// cannot drift apart. There is no fallback between the flows: each mints a
/// different participant identity, and peers map us by the one our published
/// transport implies.
pub(crate) async fn acquire_token(
    http: &reqwest::Client,
    config: &LiveKitTransportConfig,
    backend: &dyn MatrixBackend,
) -> Result<SfuToken, Error> {
    match &config.target {
        // `room` is the room id, because that is the `livekit_alias` this
        // generation announces on a focus; the two must agree or the clients
        // land in different LiveKit rooms. See
        // `compat::room_state::RoomStateDialect::member_content`.
        LiveKitTarget::SfuGet { service_url } => {
            let openid_token = backend.openid_token().await?;
            token::get_legacy_token(
                http,
                service_url,
                &config.room_id,
                &config.device_id,
                &openid_token,
            )
            .await
        }
        LiveKitTarget::CsApi { url, server_name } => {
            let request = LiveKitTokenRequest {
                server_name: Some(server_name.clone()),
                url: url.clone(),
                room_id: config.room_id.clone(),
                slot_id: config.slot_id.clone(),
                member_id: config.member_id.clone(),
            };
            log::debug!(
                "[{}/{}] requesting an SFU token for {url} from the homeserver ({server_name})",
                config.room_id,
                config.slot_id
            );
            let jwt = backend
                .livekit_token(&request)
                .await
                .inspect_err(|error| {
                    log::warn!("homeserver SFU token request for {url} failed: {error}");
                })
                .map_err(|error| {
                    if error.is_unrecognized_endpoint() {
                        Error::TokenEndpointUnsupported(error)
                    } else {
                        Error::Backend(error)
                    }
                })?;
            Ok(SfuToken {
                jwt,
                url: url.clone(),
            })
        }
    }
}

/// Connect to the LiveKit SFU for a MatrixRTC slot.
///
/// Obtains an SFU token through the target's flow (see [`acquire_token`]'s
/// callers) and connects to the SFU. The connection is subscribe-only.
pub async fn connect(
    http: &reqwest::Client,
    config: &LiveKitTransportConfig,
    backend: &dyn MatrixBackend,
) -> Result<LiveKitConnection, Error> {
    let sfu_token = acquire_token(http, config, backend).await?;
    LiveKitSession::connect(&sfu_token).await
}

/// Like [`connect`], but enables MSC4195 per-participant GCM frame E2EE on the
/// LiveKit room using the supplied `key_provider`.
///
/// `key_provider` MUST be the same handle also given to
/// [`MediaKeyBridge::with_provider`] (a [`livekit::e2ee::key_provider::KeyProvider`]
/// is a cheap shared handle — clone it), so keys signalled by `matrix-rtc-core`
/// and imported through the bridge reach the frame cryptor of this room.
///
/// `auto_subscribe` false makes the connection publish-only: peers' tracks are
/// never subscribed, so no remote media (and no decoder) exists for them. Only
/// a load generator wants this — a real client leaves it `true`.
pub async fn connect_e2ee(
    http: &reqwest::Client,
    config: &LiveKitTransportConfig,
    backend: &dyn MatrixBackend,
    key_provider: livekit::e2ee::key_provider::KeyProvider,
    auto_subscribe: bool,
) -> Result<LiveKitConnection, Error> {
    let sfu_token = acquire_token(http, config, backend).await?;
    LiveKitSession::connect_with_options(
        &sfu_token,
        room_options(Some(key_provider), auto_subscribe),
    )
    .await
}

/// The room options of a connection: GCM frame encryption through
/// `key_provider` when there is one (none in a room whose encryption is off,
/// spec 038 R11), dynacast, and `auto_subscribe` as given.
pub(crate) fn room_options(
    key_provider: Option<livekit::e2ee::key_provider::KeyProvider>,
    auto_subscribe: bool,
) -> livekit::RoomOptions {
    use livekit::RoomOptions;
    use livekit::e2ee::{E2eeOptions, EncryptionType};

    // RoomOptions is #[non_exhaustive]; mutate a default instance.
    let mut options = RoomOptions::default();
    options.encryption = key_provider.map(|key_provider| E2eeOptions {
        encryption_type: EncryptionType::Gcm,
        key_provider,
    });
    // Publisher-side layer control: the SFU tells us which simulcast layers
    // are actually subscribed and unneeded ones stop being encoded.
    options.dynacast = true;
    options.auto_subscribe = auto_subscribe;
    // Two peer connections per SFU, as before livekit 0.7.51 made one the
    // default: in single-PC mode, remote video arriving on the connection we
    // also publish on decodes but never reaches its track's sink (black tiles).
    options.single_peer_connection = false;
    options
}

/// Errors produced by the LiveKit transport.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A transport-level HTTP error while talking to the authorisation service.
    #[error("HTTP error talking to the LiveKit authorisation service: {0}")]
    Http(#[from] reqwest::Error),

    /// The authorisation service rejected the request or answered with a body
    /// that does not decode.
    #[error(transparent)]
    Service(#[from] matrix_rtc_transport::livekit::TokenServiceError),

    /// The host failed to obtain the OpenID token, or the homeserver's SFU
    /// token.
    #[error(transparent)]
    Backend(#[from] matrix_rtc_core::BackendError),

    /// Our homeserver lacks MSC4195's `get_token` (spec 038 R44).
    #[error("the homeserver does not support MSC4195 get_token: {0}")]
    TokenEndpointUnsupported(matrix_rtc_core::BackendError),

    /// Connecting to or operating the LiveKit SFU room failed.
    #[error("LiveKit room error: {0}")]
    Room(#[from] livekit::RoomError),
}
