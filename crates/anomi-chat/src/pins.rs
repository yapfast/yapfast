//! The pins popover, under the header's pin button: a heading with its cross
//! and the pinned messages as cards, or a note in their place. Task C22
//! (docs/iced/tasks/chat.md). The Slint side: `PinsPopover` and `Popover`
//! (`ui/header.slint:275`, `:243`).

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{column, container, rule, svg, text};
use iced::{Alignment, Background, Border, Fill, Padding, Shadow, Shrink, Vector, border};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::pins::{Intent, Pins};
use anomi_vm::chat::search;
use anomi_widgets::button::{Side, icon_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list;
use yapfast_lang::tr;

use crate::search::found;

/// The popover is 420 px wide (`width: 420px`), and as tall as its pins up
/// to 560 px (the shell keeps it inside the window besides).
pub const WIDTH: f32 = 420.0;
const MOST: f32 = 560.0;
/// The corners and the border, as every header popover's.
const RADIUS: f32 = 10.0;
const BORDER: f32 = 1.0;
const HEADING: f32 = 48.0;
const SHADOW_ALPHA: f32 = 0x66 as f32 / 255.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DROP: f32 = 8.0;
/// The heading's insides: padding, the gap between parts, the glyph and the
/// cross with its glyph.
const PAD_LEFT: f32 = 16.0;
const PAD_RIGHT: f32 = 10.0;
const SPACING: f32 = 10.0;
const GLYPH: f32 = 20.0;
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The list's padding and the gap between cards.
const LIST_PAD: f32 = 12.0;
const LIST_GAP: f32 = 8.0;
/// A note: its padding, gap, picture and type.
const NOTE_PAD: f32 = 24.0;
const NOTE_GAP: f32 = 12.0;
const NOTE_ICON: f32 = 40.0;
/// How tall the note's room is, between the heading's line and the bottom
/// border (`fit`: 49 px and 172 px).
const NOTE_ROOM: f32 = 172.0;

pub fn view(pins: &Pins, motion: Motion) -> impl View<Intent> + '_ {
    let heading = container(
        iced::widget::row![
            icon(Icon::Pin, GLYPH).style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().secondary),
            }),
            text(tr!("Pinned Messages"))
                .size(typography::size::TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .width(Fill),
            icon_button(Icon::X, tr!("Close"), motion)
                .size(CLOSE)
                .glyph(CLOSE_GLYPH)
                .tip_side(Side::Below)
                .on_press(Intent::Close)
                .view(),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, PAD_RIGHT - BORDER]).left(PAD_LEFT - BORDER))
    .center_y(HEADING - 2.0 * BORDER)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().panel)),
        border: Border::default().rounded(border::top(RADIUS - BORDER)),
        ..container::Style::default()
    });
    let body = if pins.note.is_empty() {
        cards(pins, motion)
    } else {
        note(&pins.note)
    };
    container(column![
        heading,
        rule::horizontal(1).style(|theme: &Theme| iced::widget::rule::Style {
            color: theme.palette().edge,
            ..catalog::rule::line(theme)
        }),
        body
    ])
    .width(WIDTH)
    .height(Shrink.max(MOST))
    .padding(BORDER)
    .clip(true)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(Background::Color(palette.sidebar)),
            border: Border::default()
                .rounded(RADIUS)
                .width(BORDER)
                .color(palette.edge),
            shadow: Shadow {
                color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
                offset: Vector::new(0.0, SHADOW_DROP),
                blur_radius: SHADOW_BLUR,
            },
            ..container::Style::default()
        }
    })
}

/// The cards, newest first, in a list that builds only those in view.
fn cards(pins: &Pins, motion: Motion) -> Element<'_, Intent> {
    let pins = &pins.pins;
    list::list(
        pins.len(),
        move |at| {
            let mut hasher = DefaultHasher::new();
            pins[at].item.hash(&mut hasher);
            hasher.finish()
        },
        move |at| {
            found::card(&pins[at], motion).map(|asked| match asked {
                search::Intent::Jump { room, item } => Intent::Jump { room, item },
                search::Intent::Link(link) => Intent::Link(link),
                // A card asks for nothing else.
                _ => Intent::Close,
            })
        },
        motion,
    )
    // Slint lays the list over the border, so its sides sit 12 px from the
    // popover's outer edge.
    .padding(
        Padding::from(LIST_PAD)
            .left(LIST_PAD - BORDER)
            .right(LIST_PAD - BORDER),
    )
    .spacing(LIST_GAP)
    .height(Shrink)
    .boxed()
}

/// What stands for the list: the pin, and the words under it.
fn note(words: &str) -> Element<'_, Intent> {
    container(
        column![
            icon(Icon::Pin, NOTE_ICON).style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().faint),
            }),
            text(words)
                .size(typography::size::SMALL)
                .style(catalog::text::muted)
                .align_x(Alignment::Center)
        ]
        .spacing(NOTE_GAP)
        .align_x(Alignment::Center),
    )
    .padding(NOTE_PAD)
    .center_x(Fill)
    .center_y(NOTE_ROOM)
    .boxed()
}

/// Its shots for `anomi-shots`: the whole window with the popover open.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::chat::pins as sample;

    use crate::shots::{GOLDEN, Shot};
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "header",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("pins", sample::open),
        shot("pins_several", || sample::open_with(sample::several())),
        shot("pins_loading", || sample::open_with(sample::loading())),
        shot("pins_none", || sample::open_with(sample::none())),
    ]
}
