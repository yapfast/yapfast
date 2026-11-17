//! The expression picker above the composer, with its tabs: what it shows and
//! what it asks for. Task C18 (docs/iced/tasks/chat.md), porting the picker
//! popup (`expressions`, `ui/yapfast.slint:2543`), its tabs (`PickerTab`,
//! `ui/gifs.slint:235`) and its dragged edge (`:2617`), as `open-expressions`
//! and `show-expression-tab` (`:2505`, `:2515`) drive them.
//!
//! A card over the composer's emoji button with three tabs, GIFs, Emoji and
//! Stickers, one of which shows. The three pickers it holds are its own
//! copies: the window's `chat.gif_picker`, `chat.emoji_picker` and
//! `chat.sticker_picker` are for what opens one alone (the emoji popup of a
//! reaction). The card's left edge drags it wider, as Discord's, the right
//! edge staying on the button; the GIFs and the emoji share the width, and
//! each grid fits its columns to it.

use super::emoji_picker::EmojiPicker;
use super::gif_picker::GifPicker;
use super::sticker_picker::StickerPicker;

/// The width it opens at, and the least it takes (`picker-width: 498px`).
pub const WIDTH: f32 = 498.0;
/// The tallest it is, and the least, where the window leaves little room over
/// the composer (`ui/yapfast.slint:2544`).
pub const HEIGHT: f32 = 486.0;
pub const MIN_HEIGHT: f32 = 320.0;
/// What it leaves between its top and the window's.
const MARGIN: f32 = 16.0;

/// Which picker shows (`expression-tab`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Gifs,
    Emoji,
    Stickers,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Gifs, Tab::Emoji, Tab::Stickers];
}

/// The expression picker: what its screen draws. The window holds it as
/// `vm.chat.pickers`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pickers {
    pub tab: Tab,
    /// The width the edge was last dragged to, which the next start shows
    /// again (`picker-width`); anything under [`WIDTH`] shows as it.
    pub width: f32,
    /// The most it may take: from the emoji button to 8 px right of the
    /// server rail, or the window's edge (`picker-room`); the app keeps it as
    /// the window changes. None known shows [`WIDTH`].
    pub room: f32,
    /// How far the composer is from the window's top, which the picker
    /// leaves 16 px of (`composer-top`); 0 before it is known.
    pub above: f32,
    pub gif: GifPicker,
    pub emoji: EmojiPicker,
    pub stickers: StickerPicker,
}

impl Pickers {
    /// The width it shows at: dragged, not under [`WIDTH`], not over its
    /// room (`picker-shown`).
    pub fn shown(&self) -> f32 {
        self.width.max(WIDTH).min(self.room.max(WIDTH))
    }

    /// The height it shows at: [`HEIGHT`] unless the window leaves less.
    pub fn height(&self) -> f32 {
        if self.above <= 0.0 {
            return HEIGHT;
        }
        (self.above - MARGIN).clamp(MIN_HEIGHT, HEIGHT)
    }

    /// The width a drag asks for, as the pointer is: held between [`WIDTH`]
    /// and the room (`drag-width`).
    pub fn dragged(&self, wanted: f32) -> f32 {
        wanted.clamp(WIDTH, self.room.max(WIDTH))
    }
}

/// The expression picker: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant. What the tabs' pickers ask for travels as
/// `chat::Intent::{GifPicker, EmojiPicker, StickerPicker}`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A tab: it shows, the others let go of their pictures, and the one
    /// shown asks for its own (`show-expression-tab`).
    Tab(Tab),
    /// The edge is dragged: the width the pointer asks for, not yet held to
    /// the room; the app keeps [`Pickers::dragged`] of it as the width and
    /// the grids fit their columns to it.
    Drag(f32),
    /// The drag let go at this width, which the next start shows again
    /// (`picker-width-chosen`); the app keeps it in its layout file.
    Chosen(f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_opens_at_its_width_and_grows_only_to_its_room() {
        let mut pickers = Pickers::default();
        assert_eq!(pickers.shown(), WIDTH);
        pickers.room = 900.0;
        pickers.width = 300.0;
        assert_eq!(pickers.shown(), WIDTH, "never narrower");
        pickers.width = 1200.0;
        assert_eq!(pickers.shown(), 900.0, "never past the room");
        pickers.room = 200.0;
        assert_eq!(pickers.shown(), WIDTH, "a room under 498 still holds it");
    }

    #[test]
    fn a_drag_is_held_in_the_room() {
        let pickers = Pickers {
            room: 700.0,
            ..Pickers::default()
        };
        assert_eq!(pickers.dragged(100.0), WIDTH);
        assert_eq!(pickers.dragged(600.0), 600.0);
        assert_eq!(pickers.dragged(2000.0), 700.0);
    }

    #[test]
    fn a_short_window_shortens_it() {
        let mut pickers = Pickers::default();
        assert_eq!(pickers.height(), HEIGHT);
        pickers.above = 700.0;
        assert_eq!(pickers.height(), HEIGHT);
        pickers.above = 400.0;
        assert_eq!(pickers.height(), 384.0);
        pickers.above = 100.0;
        assert_eq!(pickers.height(), MIN_HEIGHT);
    }
}
