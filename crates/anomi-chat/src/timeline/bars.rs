//! The bars over the messages. While the "New" divider is out of view and
//! the newest message unread, one hangs at the top saying how much is new,
//! and jumps to the divider; under a window of older messages, which new
//! ones don't go in, one sits at the bottom with the way back to the
//! newest. The Slint side: `UnreadBar` and `PresentBar`
//! (`ui/messages.slint:2467`, `:2497`), placed at `ui/yapfast.slint:4046`
//! and `:4053`. Both arrive as popups do ([`super::grow`]).

use iced::widget::{button, container, row, space, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::{
    self,
    timeline::{Intent, Out, Timeline},
};
use yapfast_lang::tr;

use super::chevron::chevron;
use super::grow::{Edge, grow};

/// The unread bar: 12 px under the timeline's top, 32 px and round, its
/// text 14 px in, the chevron 10 px from its end, 8 px between them
/// (`ui/messages.slint:2472`, `:2484`; `ui/yapfast.slint:4048`).
const UNREAD_TOP: f32 = 12.0;
const UNREAD_HEIGHT: f32 = 32.0;
const UNREAD_LEFT: f32 = 14.0;
const UNREAD_RIGHT: f32 = 10.0;
/// The present bar: 16 px in from the sides and 8 up from the bottom, 36 px
/// tall with 8 px corners, 14 px in at its start and 12 at its end
/// (`ui/messages.slint:2499`, `:2510`; `ui/yapfast.slint:4054`).
const PRESENT_INSET: f32 = 16.0;
const PRESENT_UNDER: f32 = 8.0;
const PRESENT_HEIGHT: f32 = 36.0;
const PRESENT_RADIUS: f32 = 8.0;
const PRESENT_LEFT: f32 = 14.0;
const PRESENT_RIGHT: f32 = 12.0;
/// Both: 8 px between their parts, a 16 px chevron, and a shadow 12 px
/// soft, 4 px down, black at 38% (`drop-shadow-*`, `:2475`, `:2501`).
const GAP: f32 = 8.0;
const CHEVRON: f32 = 16.0;
const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0x60 as f32 / 255.0),
    offset: Vector::new(0.0, 4.0),
    blur_radius: 12.0,
};

/// The bar to the "New" divider, while it's out of view.
pub fn unread(timeline: &Timeline, motion: Motion) -> Element<'_, chat::Intent> {
    let (Some(out), Some(unread)) = (timeline.unread_bar(), &timeline.unread) else {
        return space().boxed();
    };
    let content = row![
        text(unread.label.as_str())
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .style(on_accent),
        // It points to where the divider is.
        chevron(CHEVRON, out == Out::Above, motion).ink(|theme| theme.palette().on_accent),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center);
    let bar = button(
        container(content)
            .padding(Padding::ZERO.left(UNREAD_LEFT).right(UNREAD_RIGHT))
            .center_y(UNREAD_HEIGHT),
    )
    .padding(0)
    .on_press(Intent::JumpToUnread.into())
    .style(unread_style);
    container(grow(bar, Edge::Top, motion))
        .center_x(Fill)
        .align_top(Fill)
        .padding(Padding::ZERO.top(UNREAD_TOP))
        .boxed()
}

fn on_accent(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().on_accent),
        ..text::Style::default()
    }
}

fn unread_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.palette();
    button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => palette.accent_hover,
            button::Status::Active | button::Status::Disabled => palette.accent,
        })),
        border: Border::default().rounded(UNREAD_HEIGHT / 2.0),
        shadow: SHADOW,
        ..button::Style::default()
    }
}

/// The bar back to the newest messages, under a window of older ones.
pub fn present(timeline: &Timeline, motion: Motion) -> Element<'_, chat::Intent> {
    if !timeline.detached() {
        return space().boxed();
    }
    let content = row![
        text(tr!("You're viewing older messages"))
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .width(Fill)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::secondary),
        // The button's ink: the accent, brighter with the pointer anywhere
        // on the bar.
        text(tr!("Jump to present"))
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .style(catalog::text::inherit),
        chevron(CHEVRON, false, motion),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center);
    let bar = button(
        container(content)
            .padding(Padding::ZERO.left(PRESENT_LEFT).right(PRESENT_RIGHT))
            .center_y(PRESENT_HEIGHT),
    )
    .padding(0)
    .width(Fill)
    .on_press(Intent::JumpToPresent.into())
    .style(present_style);
    container(grow(bar, Edge::Bottom, motion))
        .width(Fill)
        .align_bottom(Fill)
        .padding(
            Padding::ZERO
                .left(PRESENT_INSET)
                .right(PRESENT_INSET)
                .bottom(PRESENT_UNDER),
        )
        .boxed()
}

fn present_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.palette();
    button::Style {
        background: Some(Background::Color(palette.raised)),
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => palette.accent_hover,
            button::Status::Active | button::Status::Disabled => palette.accent,
        },
        border: Border::default()
            .rounded(PRESENT_RADIUS)
            .width(1)
            .color(palette.edge),
        shadow: SHADOW,
        ..button::Style::default()
    }
}
