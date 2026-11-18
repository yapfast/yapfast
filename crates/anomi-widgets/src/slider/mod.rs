//! Sliders and meters, one module each: [`volume`] (`VolumeSlider`),
//! [`upright`] (`UprightVolumeSlider`), [`sensitivity`] (`SensitivitySlider`)
//! and [`level`] (`LevelBar`), the first three over one track and handle,
//! [`dial`]; `ui/controls.slint:1526`, `:1737`, `:1818`, `:1798`. Task F12
//! (docs/iced/tasks/foundation.md).
//!
//! The values are whole numbers (percent, dB) and the widgets controlled:
//! each shows the value it is given and asks for the next through its
//! callbacks, so the app's state is what is drawn. The label is the caller's,
//! translated with `tr!`; the sensitivity slider's gate words and dB unit are
//! its own.

pub mod dial;
mod header;
pub mod level;
pub mod sensitivity;
pub mod track;
pub mod upright;
pub mod volume;

pub use dial::{Backdrop, Dial, focus};
pub use level::{LevelBar, Reading, level_bar};
pub use sensitivity::{Sensitivity, sensitivity};
pub use upright::upright;
pub use volume::{Volume, volume};

#[cfg(test)]
mod tests;
