// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! The call's media model, over any transport.
//!
//! [`matrix-rtc-core`] answers *who is in the call* (memberships, keys);
//! `matrix-rtc-transport` answers *how bytes flow*. This crate sits between
//! them and gives applications one vocabulary for both:
//!
//! - [`Participant`]s keyed by MatrixRTC `member_id`, each with media
//!   [`StreamState`]s (microphone, camera, screenshare, ...), and the
//!   [`TileRoster`] a UI draws;
//! - a unified [`CallEvent`] stream merging membership signalling and media
//!   transport state;
//! - [`attach_media`], the one wiring of media onto a joined call.
//!
//! The [`CallEngine`] ties these together: it watches the core's membership
//! snapshots, maps transport-level participant identities back to memberships,
//! and (from Phase 1) maintains one connection per focus so that MSC4195
//! multi-SFU calls look like a single flat participant set.
//!
//! Transports implement `matrix-rtc-transport`'s traits; everything
//! in this crate is `Send`, deliberately on the other side of a channel
//! boundary from the core's `?Send` command futures (the only core input is
//! the `watch` membership snapshot channel, whose payload is plain data).
//!
//! [`matrix-rtc-core`]: matrix_rtc_core

pub mod attach;
pub mod engine;
pub mod event;
pub mod interruptions;
#[cfg(feature = "livekit")]
pub mod livekit;
pub mod participant;
pub mod tile;

pub use attach::{AttachError, AttachOptions, MediaAttachment, attach_media};
pub use engine::{CallEngine, EngineConfig, EngineHandle, KEY_GRACE, StabilityConfig};
pub use event::{CallEvent, FrameEncryptionDiagnostic};
pub use interruptions::Interruptions;
#[cfg(feature = "livekit")]
pub use livekit::{LiveKitAttachOptions, LiveKitAttachment, attach_livekit};
#[cfg(feature = "matrix-sdk")]
pub use livekit::{LiveKitCall, LiveKitCallError, LiveKitCallOptions, close_slot, open_slot};
#[cfg(feature = "livekit")]
pub use matrix_rtc_livekit::supported_video_codecs;
pub use matrix_rtc_transport::ConnectionState;
pub use participant::{Participant, StreamState, UnableToDecrypt};
pub use tile::{
    CallTile, DetailWindow, LocalState, TileId, TileKind, TileRef, TileRoster, Tiles, derive_tiles,
    window,
};
// The SDK backend lives in `matrix-rtc-matrix-sdk`; re-exported so a host
// driving a call keeps one dependency.
#[cfg(feature = "matrix-sdk")]
pub use matrix_rtc_matrix_sdk::SdkMatrixBackend;
