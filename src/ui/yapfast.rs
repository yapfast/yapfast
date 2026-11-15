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
    // The window names this fork; `--version` keeps the code's own name.
    let named = |line: String| match line.strip_prefix("Yapfast") {
        Some(rest) => format!("{}{rest}", yapfast_lang::PRODUCT),
        None => line,
    };
    shown.set_product(yapfast_lang::PRODUCT.into());
    shown.set_headline(named(build.headline()).into());
    shown.set_channel(yapfast_lang::text(build.channel().label()).into());
    shown.set_details(build.details().into());
    shown.set_tone(build.channel().tone());
    shown.set_summary(named(build.summary()).into());
    Ok(window)
}
