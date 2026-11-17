//! What the kit's own tests share: a message to send, and the fonts.

use std::sync::Once;

use iced::widget::scrollable::Viewport;

/// What a widget under test sends.
#[derive(Clone, Debug)]
pub enum Message {
    Nothing,
    Scrolled(Viewport),
    /// What Ctrl+C copied of a `widgets::Selectable`.
    Copied(String),
}

/// The window's fonts, once, without Twemoji.
pub fn install_fonts() {
    static FONTS: Once = Once::new();
    FONTS.call_once(|| crate::fonts::install(&[]));
}
