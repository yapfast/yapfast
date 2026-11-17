//! What stands at an edge of the messages loaded: earlier ones on their
//! way, a way to load them, or the conversation's start over the first
//! row; under the last row of a window of older messages, the same for
//! newer ones. Each is as tall in every state, so changing from one to
//! another doesn't move the messages. The Slint side: `HistoryEdge`
//! (`ui/messages.slint:2390`-`:2463`), with Discord's welcome block that
//! opens a server channel's history (`:2411`-`:2428`).

use iced::font::Weight;
use iced::widget::{button, column, container, row, space, stack, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::{
    self,
    timeline::{Edge, Intent, Intro, Kind, Room, Timeline},
};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::skeleton::skeleton;
use yapfast_lang::tr;

/// The one line: 42 px, its 30 px of content 4 px down (`:2409`-`:2410`).
const LINE_TOP: f32 = 4.0;
const LINE_BOTTOM: f32 = 8.0;
const LINE: f32 = 30.0;
/// The button: round, 12 px in at its icon and 14 at its text's end, 6 px
/// between them, a 16 px icon (`:2436`, `:2451`, `:2453`).
const PILL_RADIUS: f32 = 15.0;
const PILL_LEFT: f32 = 12.0;
const PILL_RIGHT: f32 = 14.0;
const PILL_GAP: f32 = 6.0;
const PILL_ICON: f32 = 16.0;
/// The welcome block: 47 px down, 12 px under it, its two 40 px lines of
/// 32 px text, 8 px gaps, a 20 px line under them; a channel's hash, 40 px
/// in a 68 px circle (`:2412`-`:2427`). A server's is 167 px tall, a
/// channel's 203 (`ClientStyle.intro-*-height`).
const WELCOME_TOP: f32 = 47.0;
const WELCOME_BOTTOM: f32 = 12.0;
const WELCOME_LINE: f32 = 40.0;
const WELCOME_SIZE: f32 = 32.0;
const WELCOME_GAP: f32 = 8.0;
const WELCOME_TEXT: f32 = 20.0;
const HASH_CIRCLE: f32 = 68.0;
const HASH: f32 = 40.0;

/// Over the first row: what's before it.
pub fn earlier<'a>(
    timeline: &Timeline,
    room: &'a Room,
    motion: Motion,
) -> Element<'a, chat::Intent> {
    match (timeline.earlier, &room.intro) {
        (Edge::Start, Some(intro)) => welcome(room, intro),
        (Edge::Start, None) => start(room),
        (edge, _) => pill(edge, false, timeline.locked, motion),
    }
}

/// Under the last row of a window of older messages: what's after it.
pub fn later<'a>(edge: Edge, locked: bool, motion: Motion) -> Element<'a, chat::Intent> {
    pill(edge, true, locked, motion)
}

/// The conversation's start, in one muted line.
fn start(room: &Room) -> Element<'_, chat::Intent> {
    let name = room.name.as_str();
    let says = match room.kind {
        Kind::Direct(_) => tr!("This is the beginning of your messages with {}.", name),
        Kind::Thread(_) => tr!("This is the start of the {} thread.", name),
        Kind::Channel(_) => tr!("This is the start of #{}.", name),
    };
    line(
        text(says)
            .size(typography::size::SMALL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    )
}

/// Earlier or newer messages: a button that loads them, the same button
/// lit by the skeletons' sweep while they come, or a warning and Retry.
fn pill<'a>(edge: Edge, newer: bool, locked: bool, motion: Motion) -> Element<'a, chat::Intent> {
    let (loading, failed) = (edge == Edge::Loading, edge == Edge::Failed);
    let says = match (loading, failed, newer) {
        (true, _, true) => tr!("Loading newer messages..."),
        (true, _, false) => tr!("Loading earlier messages..."),
        (_, true, true) => tr!("Couldn't load newer messages. Retry"),
        (_, true, false) => tr!("Couldn't load earlier messages. Retry"),
        (_, _, true) => tr!("Newer messages"),
        (_, _, false) => tr!("Earlier messages"),
    };
    let glyph = if failed {
        Icon::TriangleAlert
    } else {
        Icon::History
    };
    let content = container(
        row![
            icon(glyph, PILL_ICON).style(move |theme: &Theme, _| {
                let palette = theme.palette();
                iced::widget::svg::Style {
                    color: Some(if failed {
                        palette.danger
                    } else {
                        palette.secondary
                    }),
                }
            }),
            text(says)
                .size(typography::size::SMALL)
                .font(typography::MEDIUM)
                .style(catalog::text::secondary),
        ]
        .spacing(PILL_GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.left(PILL_LEFT).right(PILL_RIGHT))
    .center_y(LINE);
    let face: Element<'a, chat::Intent> = if loading {
        // The sweep of the skeletons that stand in for other things loading,
        // which moves only while someone may be looking.
        stack![content]
            .push_under(
                skeleton(motion)
                    .width(Fill)
                    .height(Fill)
                    .radius(PILL_RADIUS)
                    .boxed(),
            )
            .boxed()
    } else {
        let asks = if newer {
            Intent::Later
        } else {
            Intent::Earlier
        };
        button(content)
            .padding(0)
            .on_press_maybe((!locked).then(|| chat::Intent::from(asks)))
            .style(pill_style)
            .boxed()
    };
    line(face)
}

/// The button's ground: raised, the active fill under the pointer.
fn pill_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.palette();
    button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => palette.active,
            button::Status::Active | button::Status::Disabled => palette.raised,
        })),
        border: Border::default().rounded(PILL_RADIUS),
        ..button::Style::default()
    }
}

/// `content` centred in the one line's 30 px, 4 px down, 8 px over the rest.
fn line<'a>(content: impl View<chat::Intent> + 'a) -> Element<'a, chat::Intent> {
    container(container(content).center_x(Fill).center_y(LINE))
        .padding(Padding::ZERO.top(LINE_TOP).bottom(LINE_BOTTOM))
        .boxed()
}

/// Discord's welcome to a server, or to one of its channels.
fn welcome<'a>(room: &'a Room, intro: &'a Intro) -> Element<'a, chat::Intent> {
    let gap = || space().height(WELCOME_GAP);
    let name = room.name.as_str();
    let block = match intro {
        Intro::Server(server) => column![
            big(tr!("Welcome to"), Weight::Semibold),
            big(server.as_str(), Weight::Semibold),
            gap(),
            small(
                tr!("This is the beginning of this server."),
                typography::size::SMALL
            ),
        ],
        Intro::Channel => column![
            container(hash()).center_x(Fill),
            gap(),
            big(tr!("Welcome to #{}!", name), Weight::Bold),
            gap(),
            small(
                tr!("This is the start of the #{} channel.", name),
                typography::size::TITLE
            ),
        ],
    };
    block
        .padding(Padding::ZERO.top(WELCOME_TOP).bottom(WELCOME_BOTTOM))
        .width(Fill)
        .boxed()
}

/// One of the welcome's big lines.
fn big<'a>(says: impl text::IntoFragment<'a>, weight: Weight) -> Element<'a, chat::Intent> {
    container(
        text(says)
            .size(WELCOME_SIZE)
            .font(typography::weight(weight))
            .width(Fill)
            .align_x(Alignment::Center)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
    )
    .center_y(WELCOME_LINE)
    .boxed()
}

/// The welcome's line under its big ones.
fn small<'a>(says: impl text::IntoFragment<'a>, size: f32) -> Element<'a, chat::Intent> {
    container(
        text(says)
            .size(size)
            .width(Fill)
            .align_x(Alignment::Center)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::secondary),
    )
    .center_y(WELCOME_TEXT)
    .boxed()
}

/// A hash in a circle, as Discord opens a channel's history.
fn hash<'a>() -> Element<'a, chat::Intent> {
    container(
        icon(Icon::Hash, HASH).style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().text),
        }),
    )
    .center(HASH_CIRCLE)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().field)),
        border: Border::default().rounded(HASH_CIRCLE / 2.0),
        ..container::Style::default()
    })
    .boxed()
}
