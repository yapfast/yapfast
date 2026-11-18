//! What the card is made of: its base, what lies over the picture always,
//! and what shows under the pointer (`ui/voice.slint:676`-`:748`).

use iced::widget::{Space, column, container, row, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Gradient, Padding, Radians};

use anomi_theme::prelude::*;
use anomi_theme::{rgba, typography};
use anomi_vm::voice::pip::{Intent, Pip};
use anomi_widgets::button::icon_button;
use anomi_widgets::picture::{self, Fit};

use crate::tiles::live::live;
use crate::tiles::viewers::viewers;
use crate::tiles::waiting::waiting;

use super::{EDGE, RADIUS};

/// How far the tag and the viewers sit from the card's top (`:702`, `:742`)
/// and the buttons and the title from its bottom (`:735`).
const INSET: f32 = 8.0;
/// The shade starts 55% of the way down (`:723`): black, to `#000000b0`.
const SHADE_FROM: f32 = 0.55;
const SHADE: Color = rgba(0x0000_00b0);
/// The stop button's scrim and corners (`:731`, `:733`), its side and how
/// far the title keeps from the right edge (`:726`, `:730`).
const SCRIM: Color = rgba(0x0000_00a6);
const BUTTON: f32 = 32.0;
const BUTTON_RADIUS: f32 = 16.0;
const TITLE_LEFT: f32 = 12.0;
const TITLE_HEIGHT: f32 = 24.0;
/// The title's width is the card's less this, and it lies this far above
/// the bottom (`:729`-`:730`).
const TITLE_RIGHT: f32 = 60.0;
const TITLE_BOTTOM: f32 = 10.0;
const TITLE: f32 = typography::size::SMALL;

/// The black card and the picture whole in it, at the card's full size as
/// Slint lays it; the edge is drawn over it by `marks`.
pub fn base(pip: &Pip, motion: Motion) -> impl View<Intent> + '_ {
    let inside: Element<'_, Intent> = match &pip.frame {
        Some(frames) => picture::live(frames)
            .width(Fill)
            .height(Fill)
            .fit(Fit::Contain)
            .radius(RADIUS)
            .boxed(),
        None => waiting(&pip.status, motion).boxed(),
    };
    container(inside)
        .width(Pip::WIDTH)
        .height(Pip::HEIGHT)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::BLACK)),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
}

/// The LIVE tag at the top left and who watches at the top right, over the
/// picture.
pub fn marks(pip: &Pip, motion: Motion) -> impl View<Intent> + '_ {
    let tag: Element<'_, Intent> = if pip.live {
        live(None)
    } else {
        Space::new().boxed()
    };
    let edge = container(Space::new())
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            border: Border {
                color: theme.palette().edge,
                width: EDGE,
                radius: RADIUS.into(),
            },
            ..container::Style::default()
        });
    stack![
        container(
            row![tag, Space::new().width(Fill), viewers(&pip.viewers, motion)]
                .align_y(Alignment::Start),
        )
        .padding(INSET)
        .width(Fill)
        .height(Fill),
        edge
    ]
    .width(Fill)
    .height(Fill)
}

/// Under the pointer: the shade over the lower half, the title on it and the
/// button that stops what the card shows, when something can be stopped.
pub fn hover(pip: &Pip, motion: Motion) -> impl View<Intent> + '_ {
    // The title's box is Slint's (`:729`): 12 in, 10 above the bottom and
    // a fixed width, whether or not there is a button; the button has a
    // layer of its own at the corner.
    let title = text(&pip.title)
        .size(TITLE)
        .font(typography::BOLD)
        .color(Color::WHITE)
        .width(Pip::WIDTH - TITLE_RIGHT)
        .height(TITLE_HEIGHT)
        .align_y(Alignment::Center)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let titles = column![
        Space::new().height(Fill),
        container(title).padding(Padding::ZERO.left(TITLE_LEFT).bottom(TITLE_BOTTOM))
    ]
    .width(Fill)
    .height(Fill);
    let mut layers: Vec<Element<'_, Intent>> = vec![titles.boxed()];
    if let Some(stop) = &pip.stop {
        layers.push(
            container(
                icon_button(stop.icon, &stop.label, motion)
                    .size(BUTTON)
                    .tint(|_| Color::WHITE)
                    .fill(|_| SCRIM)
                    .radius(BUTTON_RADIUS)
                    .on_press(Intent::Stop)
                    .view()
                    .boxed(),
            )
            .padding(INSET)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .width(Fill)
            .height(Fill)
            .boxed(),
        );
    }
    let shade = Gradient::Linear(
        iced::gradient::Linear::new(Radians(std::f32::consts::PI))
            .add_stop(SHADE_FROM, Color::TRANSPARENT)
            .add_stop(1.0, SHADE),
    );
    container(
        container(stack(layers))
            .width(Fill)
            .height(Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Gradient(shade)),
                border: Border::default().rounded(RADIUS - EDGE),
                ..container::Style::default()
            }),
    )
    .padding(EDGE)
    .width(Fill)
    .height(Fill)
}
