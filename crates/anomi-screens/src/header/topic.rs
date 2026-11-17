//! A channel's topic: its line in the header, after a dot, cut short and
//! fading out at the edge, and the popover that shows it in full. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: the topic of
//! `ui/yapfast.slint:3242`-`:3272` and `TopicPopover` (`ui/header.slint:511`).

use iced::widget::{container, hover, responsive, row, space, stack};
use iced::{Alignment, Background, Border, Fill, Gradient, Length, Padding, Radians};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::header::{Channel, Header, Title};
use anomi_vm::menu::Spot;
use anomi_vm::text::Text;
use anomi_widgets::list::scroll_view;
use anomi_widgets::menu::opener;
use anomi_widgets::paragraph::paragraph;

use super::Intent;
use super::limit::{capped, unbounded};
use super::popover::{INSET, popover};

/// The fade at the end of a line that doesn't fit (`:3259`).
const FADE: f32 = 40.0;
/// The popover's width (`:4879`), and how tall it grows before its text
/// scrolls (`most`, `:4893`). A topic is at most 1 024 characters.
const WIDTH: f32 = 440.0;
const MOST: f32 = 400.0;
/// The line's height, as the 24 px rule beside it (`:3242`).
const LINE: f32 = 24.0;
/// The dot: its 9 px box, the dot at x=2, 3 px across and round, in the
/// faint ink at 75 % (`:3244`).
const DOT: f32 = 3.0;
const DOT_BEFORE: f32 = 2.0;
const DOT_AFTER: f32 = 4.0;
const DOT_ALPHA: f32 = 0.75;

/// The dot, then the line, which opens the topic in full under it.
pub(super) fn line<'a>(topic: &'a Text, gap: f32, _motion: Motion) -> Element<'a, Intent> {
    // The line is laid out at its natural width and cut by its area, in the
    // middle of a word, under a fade over its last 40 px (or half of the
    // area, where that is less).
    let area = move |ink: fn(&anomi_theme::Palette) -> iced::Color| {
        responsive(move |size| {
            let words = paragraph::<Intent>(topic)
                .size(typography::size::SMALL)
                .color(ink)
                .max_lines(1)
                .selectable(false);
            let fade_width = FADE.min(size.width / 2.0);
            stack![
                container(unbounded(words))
                    .width(Fill)
                    .center_y(Fill)
                    .clip(true),
                container(container(space().width(fade_width).height(Fill)).style(fade))
                    .height(Fill)
                    .align_right(Fill),
            ]
            .boxed()
        })
    };
    let area = opener(hover(
        container(area(|palette| palette.muted))
            .width(Fill)
            .height(LINE),
        container(area(|palette| palette.secondary))
            .width(Fill)
            .height(LINE),
    ))
    .on_press(|bounds| Intent::Topic(Spot::below(bounds)));
    row![dot(), area]
        .spacing(gap)
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

/// Discord's round dot between the name and the topic (`:3244`), in a 9 px
/// box with the dot at x=2.
fn dot<'a>() -> impl View<Intent> + 'a {
    container(
        container(space())
            .width(DOT)
            .height(DOT)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(
                    theme.palette().faint.scale_alpha(DOT_ALPHA),
                )),
                border: Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            }),
    )
    .padding(Padding::default().left(DOT_BEFORE).right(DOT_AFTER))
}

/// The chat's ground, from nothing to all of it.
fn fade(theme: &Theme) -> container::Style {
    let chat = theme.palette().chat;
    let gradient = Gradient::Linear(
        iced::gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2))
            .add_stop(0.0, chat.scale_alpha(0.0))
            .add_stop(1.0, chat),
    );
    container::Style {
        background: Some(Background::Gradient(gradient)),
        ..container::Style::default()
    }
}

/// The topic in full, under the header's line of it: what the shell's
/// `Layer::Topic` shows (`:4879`). Nothing where the header has no topic.
pub fn topic(header: &Header, motion: Motion) -> Element<'_, Intent> {
    let Header::Open(place) = header else {
        return space().boxed();
    };
    let Title::Channel(Channel {
        icon,
        name,
        topic: Some(topic),
        ..
    }) = &place.title
    else {
        return space().boxed();
    };
    let body = scroll_view(
        container(
            paragraph::<Intent>(topic)
                .size(typography::size::BODY)
                .selectable(true)
                .on_link(Intent::Link),
        )
        // Below the line, 16 px from the card's edges top and bottom
        // (`ui/header.slint:520`); only the sides lose the border's pixel.
        .padding(
            Padding::from(INSET)
                .top(metrics::SPACE_L)
                .bottom(metrics::SPACE_L),
        ),
        motion,
    )
    .height(Length::Shrink);
    container(capped(
        popover(*icon, name, None, Intent::Close, body, motion),
        MOST,
    ))
    .width(WIDTH)
    .boxed()
}
