//! An answer's emoji: a Unicode one drawn as text (which Twemoji draws), a
//! server's as its picture, or room kept while that is on its way
//! (`EmojiFace`, `ui/messages.slint`).

use iced::Alignment;
use iced::widget::{container, text};

use anomi_theme::prelude::*;
use anomi_vm::kind::Expected;
use anomi_vm::people::status::Emoji;
use anomi_widgets::picture::{Fit, picture};

pub(super) fn face<'a, M: 'a>(emoji: &'a Emoji, side: f32) -> Element<'a, M> {
    if emoji.glyph.is_empty() {
        match &emoji.picture {
            Expected::Ready(handle) => picture(handle.clone()).side(side).fit(Fit::Contain).boxed(),
            _ => container(text("")).width(side).height(side).boxed(),
        }
    } else {
        text(emoji.glyph.as_str())
            .size(side)
            .align_x(Alignment::Center)
            .boxed()
    }
}
