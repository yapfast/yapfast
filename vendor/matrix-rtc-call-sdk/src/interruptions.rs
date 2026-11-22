// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! What is interrupting the call right now, cause by cause (034 R38): the
//! one record a host draws its connection indicator and list from. Each cause
//! is its own field and changes on its own; the record is empty when nothing
//! is interrupted. The engine publishes it after the roster and local state
//! of the same change, so a reader woken by it finds tiles that agree.

use matrix_rtc_call::SignallingHealth;
use matrix_rtc_transport::ConnectionState;

/// Every cause of an interruption, each in its own field. Member lists hold
/// `member_id`s on the roster, sorted; `unmappable` holds transport
/// identities, which map to no member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interruptions {
    /// Our homeserver, as the client's sends see it (R9).
    pub signalling: SignallingHealth,
    /// Our membership has lapsed for the other members, and our media is
    /// paused until it is republished (R17).
    pub own_membership_lapsed: bool,
    /// Our own SFU: authoritative from the first value, before our own tile
    /// exists (R23).
    pub own_media: ConnectionState,
    /// Members whose media connection is being re-established (R28).
    pub members_reconnecting: Vec<String>,
    /// Members no transport of ours can reach (R28).
    pub members_unreachable: Vec<String>,
    /// Members whose media we cannot show: a missing or bad key past
    /// [`crate::StabilityConfig::key_grace`], or sent in clear (R33). Each member's
    /// [`crate::Participant::unable_to_decrypt`] says which.
    pub unable_to_decrypt: Vec<String>,
    /// Participants on an SFU whose membership we have not seen, while our
    /// signalling is unhealthy (R16). While it is healthy they are listed
    /// apart, as unverified, after a grace (spec 038 R25).
    pub unmappable: Vec<String>,
}

impl Default for Interruptions {
    fn default() -> Self {
        Self {
            signalling: SignallingHealth::Reachable,
            own_membership_lapsed: false,
            own_media: ConnectionState::Connected,
            members_reconnecting: Vec::new(),
            members_unreachable: Vec::new(),
            unable_to_decrypt: Vec::new(),
            unmappable: Vec::new(),
        }
    }
}

impl Interruptions {
    /// Nothing is interrupted.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
