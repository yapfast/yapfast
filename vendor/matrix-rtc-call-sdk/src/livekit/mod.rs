// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! Native LiveKit media for a call, behind the `livekit` feature: attaching
//! `matrix-rtc-livekit`'s transport to a joined call ([`attach_livekit`]),
//! and with `matrix-sdk` the [`LiveKitCall`] join/leave facade over a
//! `matrix_sdk::Client`. The transport itself knows no call; this is where the
//! two meet.

mod attach;
#[cfg(feature = "matrix-sdk")]
mod call;

pub use attach::{LiveKitAttachOptions, LiveKitAttachment, attach_livekit};
#[cfg(feature = "matrix-sdk")]
pub use call::{LiveKitCall, LiveKitCallError, LiveKitCallOptions, close_slot, open_slot};
