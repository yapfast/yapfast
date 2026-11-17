//! A group's heading: "Mods \u{2014} 5" in the channel grey, with the role's
//! picture before it when it has one, and the note shown above the list
//! while it loads. The Slint side: the heading of the list's rows
//! (`ui/people.slint:257`) and `RoleIcon` (`:46`).

use iced::widget::{container, row, space, text};
use iced::{Alignment, Color, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::kind::Expected;
use anomi_vm::people::members::{Heading, Intent};
use anomi_widgets::picture::{Fit, picture};

/// The heading's height, with 20 px above its words and 4 below
/// (`height: 42px`, `padding-top: 20px`, `padding-bottom: 4px`).
const HEIGHT: f32 = 42.0;
const ABOVE: f32 = 20.0;
const BELOW: f32 = 4.0;
/// The heading's words and picture (`:261`, `RoleIcon size: 20px`).
const GAP: f32 = 8.0;
const PICTURE: f32 = 20.0;
/// Discord's channel grey (`color: #81828a`), the same in every theme.
const GREY: Color = Color::from_rgb(
    0x81 as f32 / 255.0,
    0x82 as f32 / 255.0,
    0x8a as f32 / 255.0,
);
/// The note's padding (`padding: 16px`).
const NOTE: f32 = 16.0;

/// A picture of a role, `side` across: its room kept while it is on its way,
/// nothing for none (`RoleIcon`).
pub fn role_picture<'a>(
    expected: &'a Expected,
    side: f32,
    opacity: f32,
) -> Option<Element<'a, Intent>> {
    match expected {
        Expected::None => None,
        Expected::Waiting => Some(space().width(side).height(side).boxed()),
        Expected::Ready(handle) => Some(
            picture(handle.clone())
                .side(side)
                .fit(Fit::Contain)
                .opacity(opacity)
                .boxed(),
        ),
    }
}

pub fn heading<'a>(heading: &'a Heading) -> Element<'a, Intent> {
    let mut line = row![].spacing(GAP).align_y(Alignment::Center);
    if let Some(icon) = role_picture(&heading.icon, PICTURE, 1.0) {
        line = line.push(icon);
    }
    line = line.push(
        text(heading.text.as_str())
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .color(GREY)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed(),
    );
    container(line)
        .padding(Padding {
            top: ABOVE,
            right: 0.0,
            bottom: BELOW,
            left: GAP,
        })
        .width(Length::Fill)
        .height(HEIGHT)
        .align_y(Alignment::Center)
        .boxed()
}

/// What stands in for the list while it loads or when it can't.
pub fn note(note: &str) -> Element<'_, Intent> {
    container(typography::label(note))
        .padding(NOTE)
        .width(Length::Fill)
        .boxed()
}
