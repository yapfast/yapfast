//! `EmojiSuggestions`' rows: the emoji for the `:name` being typed, each with
//! its name (`ui/composer.slint:163`-`:195`, its face `EmojiFace`,
//! `ui/messages.slint:64`).

use iced::widget::text;
use iced::{Fill, Size};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::chat::suggestions::{Cell, Intent};

use super::heading;
use super::row;
use super::row::choice;
use crate::pickers::emoji::emoji_face;

/// The face is 20 px in a 24 px room (`side: 20px; width: 24px`).
const SIDE: f32 = 20.0;
const ROOM: f32 = 24.0;

/// Slint's `EmojiSuggestions` gives its faces no animation key, so a custom
/// emoji draws still here: `Motion::default()`.
pub fn rows<'a>(query: &str, cells: &'a [Cell], chosen: usize) -> Vec<Element<'a, Intent>> {
    let mut out = Vec::with_capacity(cells.len() + 1);
    out.push(heading(tr!("EMOJI MATCHING :{}", query)));
    for (index, cell) in cells.iter().enumerate() {
        let face = emoji_face(
            &cell.emoji,
            SIDE,
            Size::new(ROOM, row::HEIGHT),
            1.0,
            Motion::default(),
        );
        out.push(choice(index, index == chosen, [face, name(cell)]));
    }
    out
}

fn name(cell: &Cell) -> Element<'_, Intent> {
    text(cell.name.as_str())
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill)
        .boxed()
}
