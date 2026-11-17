//! Presence: the status dot's shapes, a voice spot, the status text. The
//! Slint side: `StatusDot` (`ui/rail.slint:360`), `StatusText` and
//! `VoiceSpot` (`ui/controls.slint:891`, `:947`).
//!
//! - [`status_dot`] draws a presence on a ring cut from the ground behind it.
//! - [`status_text`] is a custom status or what someone is doing.
//! - [`voice_spot`] is the voice channel someone is in.

mod dot;
mod fade;
mod spot;
mod status;

pub use dot::{Ring, StatusDot, status_dot};
pub use spot::{VoiceSpot, voice_spot};
pub use status::{StatusText, status_text};
