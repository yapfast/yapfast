//! The right side of the card: the local preview (`ui/sign-in.slint:107`).

use iced::widget::{Id, column, container, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sign_in::{Intent, SignIn};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::picture::picture;
use yapfast_lang::tr;

/// The side's width, its icon and the icon's corner (`:107`, `:110`).
pub const WIDTH: f32 = 220.0;
const ICON: f32 = 72.0;
const ICON_RADIUS: f32 = 20.0;
/// The question's size (`font-size: 20px`).
const QUESTION: f32 = 20.0;

pub fn view(sign_in: &SignIn, motion: Motion) -> Element<'_, Intent> {
    let icon = container(
        picture(super::icon())
            .side(ICON)
            .radius(ICON_RADIUS)
            .motion(motion),
    )
    .center_x(Fill);
    column![
        icon,
        text(tr!("Just looking?"))
            .size(QUESTION)
            .font(typography::BOLD)
            .width(Fill)
            .align_x(Alignment::Center)
            .style(catalog::text::primary),
        text(tr!(
            "Try the layout with sample conversations. Nothing leaves this computer."
        ))
        .size(typography::size::BODY)
        .width(Fill)
        .align_x(Alignment::Center)
        .style(catalog::text::secondary),
        action_button(tr!("Explore local preview"), Kind::Secondary, motion)
            .width(Fill)
            .height(metrics::CONTROL_HEIGHT)
            .id(Id::new(super::PREVIEW))
            .enabled(!sign_in.busy)
            .on_press(Intent::ExplorePreview),
    ]
    .spacing(metrics::SPACE_M)
    .width(WIDTH)
    .boxed()
}
