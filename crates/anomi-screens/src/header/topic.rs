//! A channel's topic: its line in the header, after a dot, cut short and
//! fading out at the edge, and the popover that shows it in full. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: the topic of
//! `ui/yapfast.slint:3242`-`:3272` and `TopicPopover` (`ui/header.slint:511`).

use iced::widget::{container, hover, row, space, stack};
use iced::{Alignment, Background, Border, Fill, Gradient, Length, Radians};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::header::{Channel, Header, Title};
use anomi_vm::menu::Spot;
use anomi_vm::text::Text;
use anomi_widgets::list::scroll_view;
use anomi_widgets::menu::opener;
use anomi_widgets::paragraph::paragraph;

use super::Intent;
use super::popover::popover;

/// The line is laid out this wide and cut by its area, which ended it at a
/// word before.
const ONE_LINE: f32 = 4000.0;
/// The fade at the end of a line that doesn't fit (`:3259`).
const FADE: f32 = 40.0;
/// The popover's width (`:4879`). A topic is at most 1 024 characters, so
/// the body, which scrolls when it must, rarely has to.
const WIDTH: f32 = 440.0;

/// The dot, then the line, which opens the topic in full under it.
pub(super) fn line<'a>(topic: &'a Text, _motion: Motion) -> Element<'a, Intent> {
    let text = |ink: fn(&anomi_theme::Palette) -> iced::Color| {
        paragraph::<Intent>(topic)
            .size(typography::size::SMALL)
            .color(ink)
            .max_lines(1)
            .selectable(false)
            .width(Length::Fixed(ONE_LINE))
    };
    let area = |ink| {
        stack![
            container(text(ink)).width(Fill).clip(true),
            container(space().width(FADE).height(Fill))
                .align_right(Fill)
                .style(fade),
        ]
        .width(Fill)
        .height(24.0)
    };
    let area = opener(hover(
        area(|palette| palette.muted),
        area(|palette| palette.secondary),
    ))
    .on_press(|bounds| Intent::Topic(Spot::below(bounds)));
    row![dot(), area]
        .spacing(10.0)
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

/// Discord's round dot between the name and the topic (`:3244`).
fn dot<'a>() -> impl View<Intent> + 'a {
    container(
        container(space())
            .width(3.0)
            .height(3.0)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().faint.scale_alpha(0.75))),
                border: Border::default().rounded(1.5),
                ..container::Style::default()
            }),
    )
    .padding([0.0, 3.0])
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
                .selectable(true),
        )
        .padding(metrics::SPACE_L),
        motion,
    )
    .height(Length::Shrink);
    container(popover(*icon, name, None, Intent::Close, body, motion))
        .width(WIDTH)
        .boxed()
}
