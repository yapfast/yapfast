//! The font files built into the interface. The program loads [`ALL`] before
//! it draws anything, and so do the headless renderers of tests and shots.
//!
//! Twemoji and the name-style faces come with task F03
//! (docs/iced/tasks/foundation.md): Twemoji as the colour font the Slint
//! build makes, and the eleven display-name faces from `ui/fonts`.

/// DM Sans, upright and italic, variable in weight and optical size.
pub const DM_SANS: &[u8] = include_bytes!("../../../ui/fonts/DMSans[opsz,wght].ttf");
pub const DM_SANS_ITALIC: &[u8] = include_bytes!("../../../ui/fonts/DMSans-Italic[opsz,wght].ttf");

/// Every font the interface loads at start.
pub const ALL: [&[u8]; 2] = [DM_SANS, DM_SANS_ITALIC];
