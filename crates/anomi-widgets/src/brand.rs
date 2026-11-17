//! Anomi's own pictures: the logo on the rail's home item, and the app's
//! icon the sign-in screen and About show.

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

/// The app's icon, 256 pixels square (`packaging/icon.png`, as Slint's
/// `SignInScreen` and About show it); decoded once, on first draw.
pub fn icon() -> image::Handle {
    static ICON: OnceLock<image::Handle> = OnceLock::new();
    ICON.get_or_init(|| {
        image::Handle::from_bytes(&include_bytes!("../../../packaging/icon.png")[..])
    })
    .clone()
}
