//! The banner above the conversation until this device is verified:
//! `VerifyBanner` (`ui/sign-in.slint:119`). A Matrix recovery key verifies
//! it, or verifying it in another client and checking again. The
//! conversation places it (`conversation::banners`, `ui/yapfast.slint:3464`)
//! over `vm.sign_in.verify`.

use iced::widget::{Id, column, container, row, text};
use iced::{Alignment, Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sign_in::{Intent, Verify};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

/// The lock's round ground and the lock (`:130`).
const DISC: f32 = 36.0;
const LOCK: f32 = 18.0;
/// The lock beside the words (`:126`).
const GAP: f32 = 14.0;
/// The recovery key's field (`min-height: 36px`).
const FIELD: f32 = metrics::CONTROL_HEIGHT;

pub fn verify_banner(verify: &Verify, motion: Motion) -> impl View<Intent> + '_ {
    let disc = container(icon(Icon::Lock, LOCK).style(|theme: &Theme, _status| {
        iced::widget::svg::Style {
            color: Some(theme.palette().accent),
        }
    }))
    .center(DISC)
    .style(|theme: &Theme| iced::widget::container::Style {
        background: Some(theme.palette().accent_soft.into()),
        border: Border::default().rounded(DISC / 2.0),
        ..Default::default()
    });
    let well = field(tr!("Recovery key"), &verify.recovery)
        .password(true)
        .height(FIELD)
        .motion(motion)
        .on_submit(Intent::Recover);
    let well = if verify.busy {
        well.view()
    } else {
        well.on_input(Intent::Recovery).view()
    };
    let actions = row![
        container(well).width(Fill),
        action_button(tr!("Recover"), Kind::Primary, motion)
            .id(Id::new(super::RECOVER))
            .enabled(!verify.busy)
            .on_press(Intent::Recover),
        action_button(tr!("Check again"), Kind::Secondary, motion)
            .id(Id::new(super::CHECK_AGAIN))
            .enabled(!verify.busy)
            .on_press(Intent::RefreshTrust),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Start);
    let words = column![
        text(tr!("Verify this device to send messages"))
            .size(typography::size::BODY)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        text(tr!(
            "Use your Matrix recovery key, or verify Yapfast in another client and check again."
        ))
        .size(typography::size::SMALL)
        .style(catalog::text::secondary),
        actions,
    ]
    .spacing(metrics::SPACE_S)
    .width(Fill);
    container(row![disc, words].spacing(GAP))
        .padding(metrics::SPACE_L)
        .width(Fill)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(theme.palette().raised.into()),
            ..Default::default()
        })
}
