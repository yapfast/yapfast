//! What the conversation says at its top, above whatever the place shows:
//! the local preview's line (`ui/yapfast.slint:3453`) and the notice with
//! Dismiss (`:3470`, F13's `banner`). The unverified device's
//! `VerifyBanner` (`:3464`) is N09's and comes with its view.

use iced::widget::{Column, container, row, text};
use iced::{Alignment, Background, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::{Intent, Vm, shell};
use anomi_widgets::cover::banner;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

/// The preview's line: 7 px above and below, its warning 16 px, 10 px
/// before its text (`:3456`-`:3460`).
const PREVIEW_PAD: f32 = 7.0;
const PREVIEW_ICON: f32 = 16.0;
const PREVIEW_GAP: f32 = 10.0;

/// The banners that show, none when nothing is to say.
pub fn view(vm: &Vm, motion: Motion) -> impl View<Intent> + '_ {
    let mut banners: Column<Element<'_, Intent>> = Column::new().width(Fill);
    if vm.conversation.preview {
        banners = banners.push(preview().boxed());
    }
    if let Some(notice) = &vm.shell.notice {
        let dismiss = Intent::Shell(shell::Intent::DismissNotice);
        banners = banners.push(banner(notice.as_str(), dismiss, motion).boxed());
    }
    banners
}

/// "Local preview: sample data. Nothing leaves this computer."
fn preview<'a>() -> impl View<Intent> + 'a {
    let warning = icon(Icon::TriangleAlert, PREVIEW_ICON).style(|theme: &Theme, _status| {
        iced::widget::svg::Style {
            color: Some(theme.palette().accent),
        }
    });
    let line = text(tr!(
        "Local preview: sample data. Nothing leaves this computer."
    ))
    .size(typography::size::SMALL)
    .font(typography::MEDIUM)
    .width(Fill)
    .style(catalog::text::secondary);
    container(
        row![warning, line]
            .spacing(PREVIEW_GAP)
            .align_y(Alignment::Center),
    )
    .padding([PREVIEW_PAD, metrics::SPACE_L])
    .width(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().raised)),
        ..container::Style::default()
    })
}
