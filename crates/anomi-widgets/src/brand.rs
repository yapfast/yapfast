//! Anomi's own pictures: the logo on the rail's home item.

use std::sync::OnceLock;

use iced::widget::image;

/// The logo, 64 pixels square; decoded once, on first draw.
pub fn logo() -> image::Handle {
    static LOGO: OnceLock<image::Handle> = OnceLock::new();
    LOGO.get_or_init(|| {
        image::Handle::from_bytes(&include_bytes!("../../../ui/icons/anomi-small.png")[..])
    })
    .clone()
}
