//! Selects the presentation without changing the shared window contract.
use super::*;

pub(super) fn enabled() -> bool {
    !std::env::var("YAPFAST_UI").is_ok_and(|mode| mode == "classic")
}

/// Used at startup and when the tray recreates the window.
pub(super) fn window() -> Result<YapfastWindow, slint::PlatformError> {
    let window = YapfastWindow::new()?;
    let style = window.global::<ClientStyle>();
    style.set_enabled(enabled());
    let build = crate::build_info::current();
    let shown = window.global::<BuildInfo>();
    shown.set_headline(build.headline().into());
    shown.set_channel(build.channel().label().into());
    shown.set_details(build.details().into());
    shown.set_tone(build.channel().tone());
    shown.set_summary(build.summary().into());
    Ok(window)
}
