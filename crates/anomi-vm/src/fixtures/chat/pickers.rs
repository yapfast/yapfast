//! Samples of the expression picker above the composer, with its tabs for
//! tests and shots, after the fake Discord's data: task C18's. They hold the
//! samples of the GIF and emoji pickers, as the card does. The golden shot of
//! the GIFs tab is `picker/gif`.

use crate::chat::emoji_picker::EmojiPicker;
use crate::chat::gif_picker::GifPicker;
use crate::chat::pickers::{Pickers, Tab};

use super::{emoji_picker, gif_picker, sticker_picker};

/// The GIFs tab on its home, as the golden shot shows it.
pub fn gifs() -> Pickers {
    with_gifs(gif_picker::home())
}

/// The GIFs tab showing `gif`.
pub fn with_gifs(gif: GifPicker) -> Pickers {
    Pickers {
        tab: Tab::Gifs,
        gif,
        ..Pickers::default()
    }
}

/// The Emoji tab, as the composer's emoji button opens it.
pub fn emoji() -> Pickers {
    Pickers {
        tab: Tab::Emoji,
        emoji: EmojiPicker {
            columns: crate::chat::emoji_picker::columns(crate::chat::pickers::WIDTH),
            ..emoji_picker::composer()
        },
        ..Pickers::default()
    }
}

/// The Stickers tab (C19's picker, as it is).
pub fn stickers() -> Pickers {
    Pickers {
        tab: Tab::Stickers,
        stickers: sticker_picker::picker(),
        ..Pickers::default()
    }
}

/// The GIFs tab dragged wider, to 640 of the 900 px it has room for: its
/// grid fits three columns.
pub fn wide() -> Pickers {
    Pickers {
        width: 640.0,
        room: 900.0,
        ..gifs()
    }
}
