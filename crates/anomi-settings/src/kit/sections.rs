//! The sections of a server's or a channel's settings in Discord's layout:
//! each group under its heading, a line before a dangerous one, scrolling
//! once they are taller than the card. The Slint side: `SectionsNavigation`
//! with `SettingsRow` (`ui/settings.slint:368`): 266 px wide on the chat's
//! ground, 16 px in on the sides, the first heading's middle on the line of
//! the page's title.

use iced::widget::{column, container, rule};
use iced::{Length, Padding, alignment};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_widgets::list::scroll_view;

use super::heading::heading;
use super::nav::nav_row;

/// The navigation's width.
pub const WIDTH: f32 = 266.0;
const SIDES: f32 = 16.0;
const TOP: f32 = 12.0;
const BOTTOM: f32 = 16.0;
/// A heading's inset, its room above past the first, and its height.
const HEADING_INSET: f32 = 8.0;
const HEADING_GAP: f32 = 16.0;
const HEADING_FIRST: f32 = 24.0;
const HEADING_NEXT: f32 = 40.0;
/// The line between groups: inset 8 px, 12 px above and below.
const LINE_SIDES: f32 = 8.0;
const LINE_ABOVE: f32 = 12.0;

/// A row of the sections: a group's heading, a line between groups, or a
/// page, red for a dangerous one.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Heading(String),
    Line,
    Page {
        text: String,
        icon: Icon,
        id: usize,
        danger: bool,
    },
}

/// The sections `rows` with `selected` raised; a click sends `choose(id)`.
pub fn sections<'a, M: Clone + 'a>(
    rows: &'a [Row],
    selected: usize,
    choose: impl Fn(usize) -> M + 'a,
    motion: Motion,
) -> Element<'a, M> {
    let mut entries: Vec<Element<'a, M>> = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        entries.push(match row {
            Row::Heading(text) => {
                let first = index == 0;
                container(heading(text.as_str()))
                    .padding(Padding {
                        left: HEADING_INSET,
                        top: if first { 0.0 } else { HEADING_GAP },
                        ..Padding::ZERO
                    })
                    .height(if first { HEADING_FIRST } else { HEADING_NEXT })
                    .align_y(alignment::Vertical::Center)
                    .boxed()
            }
            Row::Line => container(rule::horizontal(1).style(catalog::rule::line))
                .padding(Padding {
                    top: LINE_ABOVE,
                    bottom: LINE_ABOVE,
                    left: LINE_SIDES,
                    right: LINE_SIDES,
                })
                .boxed(),
            Row::Page {
                text,
                icon,
                id,
                danger,
            } => nav_row(
                *icon,
                text.as_str(),
                *id == selected,
                *danger,
                false,
                choose(*id),
                motion,
            ),
        });
    }
    let list = column(entries).padding(Padding {
        top: TOP,
        bottom: BOTTOM,
        left: SIDES,
        right: SIDES,
    });
    container(scroll_view(list, motion))
        .width(Length::Fixed(WIDTH))
        .height(Length::Fill)
        .style(super::card::chat_ground)
        .boxed()
}
