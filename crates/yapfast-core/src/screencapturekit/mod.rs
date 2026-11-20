//! Screen sharing on macOS through ScreenCaptureKit: which screens and
//! windows there are ([`content`]), capturing one as pictures ([`video`]) and
//! capturing what the other applications play ([`audio`]). Both capture
//! through an `SCStream` ([`stream`]), which needs the Screen Recording
//! permission ([`permission`]). libwebrtc has a screen capturer on macOS too,
//! but ScreenCaptureKit is what the system keeps maintained: it captures one
//! window alone, gives sound, and shows the person its own sharing indicator.
//!
//! Frames and sound arrive on a dispatch queue of the stream's own. A panic
//! must never unwind into the system, so every callback catches it.

pub(crate) mod audio;
pub(crate) mod content;
pub(crate) mod permission;
mod stream;
pub(crate) mod video;
