// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! The legacy LiveKit JWT service's token exchange (`/sfu/get`) — the IO half.
//!
//! The endpoint URLs, request bodies, and response decoding live in
//! [`matrix_rtc_transport::livekit::token`], where the web binding shares them; this
//! module only POSTs what that one builds. The homeserver's MSC4195
//! `get_token` is the host's call (`MatrixBackend::livekit_token`), not ours. Obtaining the OpenID token itself is
//! a Client-Server API concern and belongs to the backend: the host
//! supplies one through
//! [`MatrixBackend`](matrix_rtc_core::MatrixBackend), so nothing here
//! is wired to a particular Matrix SDK.
//!

use matrix_rtc_core::OpenIdToken;
pub use matrix_rtc_transport::livekit::token::SfuToken;
use matrix_rtc_transport::livekit::token::{legacy_token_request, parse_sfu_token};

use crate::Error;

/// Exchange an OpenID token for a LiveKit SFU JWT via the legacy JWT service's
/// `/sfu/get` endpoint, for every transport naming a `livekit_service_url`.
///
/// There is deliberately no fallback to the homeserver's `get_token` on a 404 —
/// see [`matrix_rtc_transport::livekit::token::legacy_token_request`].
pub async fn get_legacy_token(
    http: &reqwest::Client,
    livekit_service_url: &str,
    room: &str,
    device_id: &str,
    openid_token: &OpenIdToken,
) -> Result<SfuToken, Error> {
    let (endpoint, body) = legacy_token_request(livekit_service_url, room, device_id, openid_token);
    post_for_token(http, &endpoint, &body, room).await
}

/// POST a built request to a token endpoint and decode the `{jwt, url}`
/// response. `log_tag` is what prefixes the log lines; the JWT itself is never
/// logged.
async fn post_for_token(
    http: &reqwest::Client,
    endpoint: &str,
    body: &serde_json::Value,
    log_tag: &str,
) -> Result<SfuToken, Error> {
    log::debug!("[{log_tag}] requesting an SFU token from {endpoint}");

    let response = http
        .post(endpoint)
        .json(body)
        .send()
        .await
        .inspect_err(|error| log::warn!("SFU token request to {endpoint} failed: {error:?}"))?;

    let status = response.status().as_u16();
    let body = response.text().await?;
    let token = parse_sfu_token(status, &body).inspect_err(|error| {
        log::warn!("SFU token request to {endpoint} failed: {error}");
    })?;
    // Never the JWT itself.
    log::debug!(
        "[{log_tag}] SFU token granted for {} (jwt {} chars)",
        token.url,
        token.jwt.len(),
    );
    Ok(token)
}
