//! A tag's emoji in the form (`PostEmoji`, `ui/forum.slint:61`): a Unicode
//! one's text a little smaller than its box, or a custom one's picture, or
//! the room its picture will take.

use iced::widget::{space, text};

use anomi_theme::prelude::*;
use anomi_vm::chat::forum::Mark;
use anomi_vm::kind::Expected;
use anomi_widgets::picture::{Fit, picture};

/// An emoji's text is a little smaller than its box (`font-size: size * 0.95`).
const TEXT: f32 = 0.95;

/// `mark` in a box `size` wide; nothing for none.
pub(super) fn mark<'a, M: 'a>(mark: &'a Mark, size: f32, motion: Motion) -> Option<Element<'a, M>> {
    match mark {
        Mark::None => None,
        Mark::Unicode(emoji) if emoji.is_empty() => None,
        Mark::Unicode(emoji) => Some(
            text(emoji.as_str())
                .size(size * TEXT)
                .wrapping(text::Wrapping::None)
                .boxed(),
        ),
        Mark::Custom(Expected::Ready(handle)) => Some(
            picture(handle.clone())
                .side(size)
                .fit(Fit::Contain)
                .motion(motion)
                .boxed(),
        ),
        Mark::Custom(_) => Some(space().width(size).height(size).boxed()),
    }
}
