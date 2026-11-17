//! The picker's foot: the emoji or the rail's section under the pointer,
//! larger, with its name in bold, or what the picker is for, muted
//! (`ui/messages.slint:1396`-`:1414`).

use std::borrow::Cow;

use iced::border::Radius;
use iced::widget::{container, row, text};
use iced::{Alignment, Border, Fill, Size};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::emoji_picker::{EmojiPicker, Hovered, Intent, Purpose};
use yapfast_lang::tr;

use super::face;

/// Its height and insets, and the gap after the emoji (`:1398`-`:1401`).
const HEIGHT: f32 = 48.0;
const PAD: f32 = 14.0;
const GAP: f32 = 10.0;
/// The emoji at 26 px in 30 (`side: 26px; width: 30px`, `:1403`).
const FACE: f32 = 26.0;
const ROOM: f32 = 30.0;

/// What the foot says before anything is pointed at.
fn hint(purpose: Purpose) -> &'static str {
    match purpose {
        Purpose::Status => tr!("Pick an emoji for your status."),
        Purpose::Composer | Purpose::Edit => tr!("Pick an emoji for your message."),
        Purpose::Reaction | Purpose::Poll => tr!("Pick an emoji to react with."),
    }
}

/// The foot, its lower corners rounded by `radius` to sit in the picker's.
pub fn view(picker: &EmojiPicker, radius: f32) -> Element<'_, Intent> {
    let room = Size::new(ROOM, HEIGHT);
    let named: Option<(Element<'_, Intent>, Cow<'_, str>)> = match &picker.hovered {
        Some(Hovered::Emoji(emoji)) => Some((
            face::emoji(emoji, FACE, room, 1.0, Motion::default()),
            Cow::Owned(emoji.name()),
        )),
        Some(Hovered::Section(index)) => picker.sections.get(*index).map(|section| {
            (
                face::foot(section, FACE, room),
                Cow::Borrowed(section.name.as_str()),
            )
        }),
        None => None,
    };
    let strong = named.is_some();
    let (face, words) = match named {
        Some((face, name)) => (Some(face), name),
        None => (None, Cow::Borrowed(hint(picker.purpose))),
    };
    let words = text(words)
        .size(typography::size::BODY)
        .font(if strong {
            typography::BOLD
        } else {
            typography::UI
        })
        .style(if strong {
            catalog::text::primary
        } else {
            catalog::text::muted
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    let mut line = row![].spacing(GAP).align_y(Alignment::Center);
    if let Some(face) = face {
        line = line.push(face);
    }
    let line = line.push(words.boxed());
    container(line)
        .padding([0.0, PAD])
        .width(Fill)
        .height(HEIGHT)
        .center_y(HEIGHT)
        .style(move |theme: &Theme| container::Style {
            background: Some(theme.palette().panel.into()),
            border: Border::default().rounded(Radius {
                bottom_left: radius,
                bottom_right: radius,
                ..Radius::default()
            }),
            ..container::Style::default()
        })
        .boxed()
}
