//! One Yapfast for each data folder: the shared `anomi_desktop::instance`,
//! and the activation token a later start brought given to the window Slint
//! makes next. See docs/tray.md.

pub use anomi_desktop::instance::*;

/// Gives the window Slint makes the token a later start brought, if one waits.
pub fn attributes(
    attributes: slint::winit_030::winit::window::WindowAttributes,
) -> slint::winit_030::winit::window::WindowAttributes {
    #[cfg(target_os = "linux")]
    if let Some(token) = take_next_token() {
        use slint::winit_030::winit::{
            platform::startup_notify::WindowAttributesExtStartupNotify, window::ActivationToken,
        };
        return attributes.with_activation_token(ActivationToken::from_raw(token));
    }
    attributes
}
