// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! The unified call event stream.
//!
//! One stream merges what used to be two worlds: membership signalling from
//! `matrix-rtc-core` (who joined, who left, whose key arrived) and media
//! transport state (streams appearing, mute changes, connection health).
//! Events reference participants by `member_id` only — obtain frames through
//! [`CallEngine::remote_track`](crate::engine::CallEngine::remote_track).
//!
//! Events are plain data (`Clone + PartialEq`) so they can be broadcast to any
//! number of subscribers and later cross the FFI boundary unchanged.

use matrix_rtc_core::KeyRejection;

use matrix_rtc_transport::{FrameEncryptionState, MediaStreamKind};

/// What the media layer can say about a frame-encryption failure.
///
/// The state itself comes from the transport's frame cryptor, which reports
/// *that* it cannot decrypt but not why. This adds the half the media layer does
/// know — which keys it has actually installed for that participant — because the
/// two cases behind a `MissingKey` need completely different investigations:
/// a key that never arrived (or arrived under an identity the transport does not
/// use) is a signalling or identity problem, while frames carrying an index we
/// have not been given yet is a rotation still in flight.
///
/// Reasons a key was *refused* live in the core, which is the only layer that
/// sees the to-device message; those surface separately as
/// [`CallEvent::KeyDiscarded`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameEncryptionDiagnostic {
    /// The state is not a failure, so there is nothing to explain.
    NotApplicable,
    /// No media key has been installed for this participant under any index.
    /// Either none ever arrived, or it arrived under a different identity.
    NoKeyInstalled,
    /// Keys are installed at these indices, so the frames must be carrying a
    /// different one — or the material itself disagrees.
    KeysInstalled {
        /// Key indices installed for this participant, in the order they were
        /// imported.
        key_indices: Vec<u8>,
    },
}

/// One speaking member and how loud they are.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeakingMember {
    pub member_id: String,
    /// `0.0` (silent) to `1.0` (loudest); `0.0` from transports that report no
    /// level.
    pub level: f32,
}

/// An event on the unified call stream.
///
/// `PartialEq` but not `Eq`: an audio level is an `f32`.
#[derive(Clone, Debug, PartialEq)]
pub enum CallEvent {
    /// A membership joined while we were in the call: one per device, our
    /// own other devices included. Never for the members already present when
    /// our own membership lands, or lands again after lapsing.
    ParticipantJoined {
        member_id: String,
        user_id: String,
        /// Memberships in the call after this change, ourselves included.
        participant_count: u32,
    },
    /// A membership left, or expired, while we were in the call. Never for
    /// the change that removes our own membership, nor after the call ended.
    ParticipantLeft {
        member_id: String,
        /// Memberships in the call after this change, ourselves included.
        participant_count: u32,
    },
    /// A participant's stream became available; frames can be obtained now.
    StreamStarted {
        member_id: String,
        kind: MediaStreamKind,
    },
    /// A participant's stream went away.
    StreamStopped {
        member_id: String,
        kind: MediaStreamKind,
    },
    /// The publisher muted the stream (it stays subscribed).
    StreamMuted {
        member_id: String,
        kind: MediaStreamKind,
    },
    /// The publisher unmuted the stream.
    StreamUnmuted {
        member_id: String,
        kind: MediaStreamKind,
    },
    /// The set of currently speaking participants changed.
    ///
    /// Carries each speaker's audio level alongside their `member_id`: "who is
    /// talking" and "how loud" arrive together from the transport, and splitting
    /// them forces a host to meter the PCM itself for the second half.
    ActiveSpeakers { speakers: Vec<SpeakingMember> },
    /// A media decryption key for this participant was imported; their frames
    /// are decryptable from here on.
    ///
    /// `identity` is the transport identity the key was installed under, so a
    /// host can match it to transport participants without a roster lookup —
    /// which it may not be able to make yet: keys held before media connects
    /// surface while the connect is still in flight.
    KeyImported {
        member_id: String,
        identity: String,
        key_index: u8,
    },
    /// Frame encryption state for a participant's media changed.
    ///
    /// Anything but [`FrameEncryptionState::Ok`] means their frames are not
    /// decoding — note that the RTP itself may be arriving perfectly well, so
    /// the receive path keeps producing frames (silence, or a frozen picture).
    /// Pair with
    /// [`CallEngine::receive_stats`](crate::engine::CallEngine::receive_stats)
    /// to tell a key failure from an empty network path.
    FrameEncryptionState {
        member_id: String,
        state: FrameEncryptionState,
        /// What the media layer knows about the failure — chiefly whether any key
        /// was installed for this participant at all.
        diagnostic: FrameEncryptionDiagnostic,
    },
    /// A media key for this participant was refused, so their frames will not
    /// decrypt. Carries the reason, which is otherwise only ever logged.
    ///
    /// Distinct from a key that never arrived: this one arrived and was rejected,
    /// which is a configuration or trust problem (an unverified device, say)
    /// rather than a delivery one.
    KeyDiscarded {
        /// The member the key claimed to be for.
        member_id: String,
        /// The rejected key's index, when the message got far enough to have one.
        key_index: Option<u8>,
        /// Who sent it, as far as decryption metadata could attribute it.
        sender_user_id: Option<String>,
        sender_device_id: Option<String>,
        /// Why it was refused. Typed rather than a message, so a host can act on
        /// it — `SenderNotConfirmed` is a "verify this device" prompt, not just a log
        /// line.
        reason: KeyRejection,
    },
    /// A participant raised their hand (Element Call's `m.reaction` annotation
    /// of their membership) while we are in the call — ours included, once the
    /// homeserver has it. An announcement: not sent for the hands up when we
    /// arrive, for a member arriving with their hand up, or when a hand that
    /// stays up moves to a refreshed membership event. The state is on the
    /// roster as
    /// [`Participant::hand_raised_at_ms`](crate::participant::Participant::hand_raised_at_ms)
    /// and [`hand_raised_position`](crate::participant::Participant::hand_raised_position).
    HandRaised {
        member_id: String,
        raised_at_ms: u64,
    },
    /// A participant lowered their hand, or left with it up, while we are in
    /// the call.
    HandLowered { member_id: String },
    /// A participant sent an emoji reaction. Transient: show `emoji` for a few
    /// seconds (Element Call uses three) and play `sound` if reaction sounds
    /// are on. The SDK does no audio; `sound` is the base name of the asset
    /// to play (`clap`, `party`, …, or `generic` for a `name` without a
    /// sound of its own), and `emoji` is the catalogue's for a catalogue
    /// `name`, as Element Call shows. Sent only while we are in the call, and
    /// only for a recent reaction (`ReactionsConfig::max_age_ms`), so a sync
    /// catching up never replays a burst of them.
    Reaction {
        member_id: String,
        emoji: String,
        /// The reaction's `name` as sent, e.g. `clapping`. Empty if none.
        name: String,
        sound: String,
    },
    /// A transport-level participant appeared that maps to no signalled
    /// membership. It gets no subscription (and could not be decrypted
    /// anyway); surfaced for diagnostics.
    UnknownParticipant { identity: String },
}
