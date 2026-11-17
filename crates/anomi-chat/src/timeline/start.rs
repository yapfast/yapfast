//! What the timeline shows without messages: what to do while no place is
//! chosen, and the opening of a conversation nobody has written in yet.
//! The Slint side: `NoConversation` and `ConversationStart`
//! (`ui/messages.slint:2373`, `:2530`), shown over the whole timeline
//! (`ui/yapfast.slint:4094`, `:4100`).

use iced::font::Weight;
use iced::widget::{column, container, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::{
    self,
    timeline::{Kind, Room},
};
use anomi_widgets::avatar::portrait;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

/// No conversation: 32 px in, 10 px apart, a message's bubble 36 px in an
/// 80 px circle, a 22 px bold title (`ui/messages.slint:2374`-`:2383`).
const NOTHING_PAD: f32 = 32.0;
const NOTHING_GAP: f32 = 10.0;
const NOTHING_CIRCLE: f32 = 80.0;
const NOTHING_ICON: f32 = 36.0;
const NOTHING_TITLE: f32 = 22.0;
/// A conversation's start: 24 px in, 8 px apart, at the bottom; a DM's
/// 72 px avatar or a channel's icon, 36 px in a 68 px circle; its name in
/// 28 px black (`:2540`-`:2548`).
const START_PAD: f32 = 24.0;
const START_GAP: f32 = 8.0;
const START_AVATAR: f32 = 72.0;
const START_CIRCLE: f32 = 68.0;
const START_ICON: f32 = 36.0;
const START_TITLE: f32 = 28.0;
const START_WEIGHT: Weight = Weight::Black;

/// No place chosen: what to do.
pub fn nothing<'a>() -> impl View<chat::Intent> + 'a {
    let bubble = container(
        icon(Icon::MessageCircle, NOTHING_ICON).style(|theme: &Theme, _| {
            iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            }
        }),
    )
    .center(NOTHING_CIRCLE)
    .style(|theme: &Theme| circle(theme.palette().raised, NOTHING_CIRCLE));
    let column = column![
        bubble,
        text(tr!("Pick a conversation"))
            .size(NOTHING_TITLE)
            .font(typography::BOLD)
            .align_x(Alignment::Center)
            .style(catalog::text::primary),
        text(tr!("Choose a DM or a server channel on the left."))
            .size(typography::size::BODY)
            .width(Fill)
            .align_x(Alignment::Center)
            .style(catalog::text::secondary),
    ]
    .spacing(NOTHING_GAP)
    .align_x(Alignment::Center)
    .width(Fill);
    container(column)
        .padding(NOTHING_PAD)
        .center_y(Fill)
        .width(Fill)
}

/// The opening of `room`, which has no messages yet.
pub fn start(room: &Room) -> impl View<chat::Intent> + '_ {
    let name = room.name.as_str();
    // A DM's or a thread's name as it is; a channel's in a welcome.
    let (picture, title, says): (Element<'_, chat::Intent>, text::Fragment<'_>, String) =
        match &room.kind {
            Kind::Direct(avatar) => (
                portrait(avatar, START_AVATAR).view().boxed(),
                name.into(),
                tr!("This is the beginning of your messages with {}.", name),
            ),
            Kind::Thread(glyph) => (
                room_icon(*glyph),
                name.into(),
                tr!(
                    "This is the start of the {} thread. No messages here yet.",
                    name
                ),
            ),
            Kind::Channel(glyph) => (
                room_icon(*glyph),
                tr!("Welcome to #{}", name).into(),
                tr!("This is the start of #{}. No messages here yet.", name),
            ),
        };
    let column = column![
        picture,
        text(title)
            .size(START_TITLE)
            .font(typography::weight(START_WEIGHT))
            .style(catalog::text::primary),
        text(says)
            .size(typography::size::BODY)
            .style(catalog::text::secondary),
    ]
    .spacing(START_GAP)
    .width(Fill);
    container(column)
        .padding(Padding::new(START_PAD))
        .align_bottom(Fill)
        .width(Fill)
}

/// A channel's or a thread's icon in a raised circle.
fn room_icon<'a>(glyph: Icon) -> Element<'a, chat::Intent> {
    container(
        icon(glyph, START_ICON).style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().text),
        }),
    )
    .center(START_CIRCLE)
    .style(|theme: &Theme| circle(theme.palette().raised, START_CIRCLE))
    .boxed()
}

fn circle(fill: iced::Color, side: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        border: Border::default().rounded(side / 2.0),
        ..container::Style::default()
    }
}
