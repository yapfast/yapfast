//! The card a custom emoji or a sticker opens: what it shows and what it asks
//! for. The Slint side: `EmojiCard`, `MessageStickerCard` (`ui/messages.slint:903`,
//! `:934`), filled by `emoji_card_for` and `sticker_card` (`src/ui/refresh.rs:759`,
//! `:773`, `:948`). Task C08 (docs/iced/tasks/chat.md).
//!
//! The window holds one [`Cards`]; `Layer::MessageCard` draws the field that
//! is open and the app clears it when the layer closes.

use iced::widget::image;

use crate::kind::Expected;

/// The card's width, as the window gives it (`width: 288px`).
pub const WIDTH: f32 = 288.0;

/// The card a custom emoji or a sticker opens: what its screen draws. The
/// window holds it as `vm.chat.cards`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cards {
    pub emoji: Option<Emoji>,
    pub sticker: Option<Sticker>,
}

/// An emoji's card: its picture (a Unicode emoji Twemoji has none of shows
/// its glyph), its name between colons and what it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Emoji {
    /// ":sob:", or a custom emoji's own ":pog:".
    pub name: String,
    /// The emoji itself, for one without a picture.
    pub glyph: String,
    /// A custom emoji's picture; a moving one plays through the picture widget.
    pub picture: Expected,
    pub description: String,
    /// The server it is from, when it is one's.
    pub server: Option<Origin>,
}

/// A sticker's card: its still or moving picture, its name as the message
/// names it ("sob") and what it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sticker {
    pub name: String,
    pub picture: Option<image::Handle>,
    pub description: String,
    /// The server it is from; `None` for a default sticker.
    pub server: Option<Origin>,
}

/// The server a card's emoji or sticker is from, as far as Discord said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Origin {
    pub id: String,
    pub name: String,
    /// Its icon at 32 px, and the letter it shows without one.
    pub icon: Expected,
    pub initial: String,
    /// "3,999 Online" and a note such as "Discoverable Server"; "" for what
    /// Discord hasn't said.
    pub online: String,
    pub note: String,
}

impl Origin {
    /// The two on one line, as the Slint card has them.
    pub fn counts(&self) -> String {
        match (self.online.is_empty(), self.note.is_empty()) {
            (false, false) => format!("{} \u{b7} {}", self.online, self.note),
            _ => format!("{}{}", self.online, self.note),
        }
    }
}

/// The card a custom emoji or a sticker opens: what a click or a key on it
/// asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The server clicked (its id): the card closes and it opens.
    OpenServer(String),
}
