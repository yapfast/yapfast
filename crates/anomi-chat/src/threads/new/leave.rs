//! The question closing a written form asks (`ui/thread.slint:561`-`:580`):
//! the panel dimmed (`rgba(0, 0, 0, 0.63)`), and a card across it, 16 px in
//! from its sides and centred, as Discord's client asks.

use iced::widget::{column, container, opaque, row, space, stack, text};
use iced::{Alignment, Border, Color, Fill, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::new_thread::{Intent, NewThread};
use anomi_widgets::button::{Kind, action_button};

/// The backdrop's strength.
const DIM: f32 = 0.63;
/// The card's inset from the panel's sides, its padding and its spacing.
const INSET: f32 = metrics::SPACE_L;
const PAD: f32 = metrics::SPACE_L;
const GAP: f32 = 10.0;
/// The title: 17 px, bold; the buttons' row has 6 px over it and 8 between.
const TITLE: f32 = 17.0;
const ABOVE_BUTTONS: f32 = 6.0;
const BUTTONS_GAP: f32 = metrics::SPACE_S;
/// `border-radius: 10px`.
const RADIUS: f32 = 10.0;

/// The ids the buttons are found by.
pub const STAY: &str = "new-thread-stay";
pub const LEAVE: &str = "new-thread-leave";

/// `panel`, with the question over it while the form is `leaving`.
pub fn over<'a>(
    panel: impl View<Intent> + 'a,
    vm: &NewThread,
    motion: Motion,
) -> impl View<Intent> + 'a {
    let panel = panel.boxed();
    if !vm.leaving {
        return panel;
    }
    let dim = container(space())
        .width(Fill)
        .height(Fill)
        .style(|_: &Theme| iced::widget::container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, DIM).into()),
            ..Default::default()
        });
    let card = container(ask(motion))
        .style(card)
        .padding(Padding::ZERO)
        .width(Fill);
    let centred = container(card)
        .padding(INSET)
        .width(Fill)
        .height(Fill)
        .align_y(alignment::Vertical::Center);
    stack![panel, opaque(stack![dim, centred])].boxed()
}

fn ask(motion: Motion) -> Element<'static, Intent> {
    column![
        text(tr!("Are you sure you want to go?"))
            .size(TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill),
        text(tr!("Any changes you made will be lost if you exit now."))
            .size(typography::size::SMALL)
            .style(catalog::text::secondary)
            .width(Fill),
        container(
            row![
                action_button(tr!("Back to editing"), Kind::Ghost, motion)
                    .id(iced::widget::Id::new(STAY))
                    .on_press(Intent::Stay),
                action_button(tr!("Exit"), Kind::Danger, motion)
                    .id(iced::widget::Id::new(LEAVE))
                    .on_press(Intent::Leave),
            ]
            .spacing(BUTTONS_GAP)
            .align_y(Alignment::Center)
        )
        .padding(Padding {
            top: ABOVE_BUTTONS,
            ..Padding::ZERO
        })
        .align_right(Fill),
    ]
    .spacing(GAP)
    .padding(PAD)
    .boxed()
}

fn card(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.palette();
    iced::widget::container::Style {
        background: Some(palette.sidebar.into()),
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(palette.edge),
        ..Default::default()
    }
}
