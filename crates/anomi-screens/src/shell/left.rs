//! The window's left column on the rail's ground: the rail and the sidebar
//! side by side, and under both the account card, with a call's dock on
//! top of it while one goes. The Slint side: the navigation `Rectangle` of
//! `YapfastWindow` (`ui/yapfast.slint:2797`-`:3133`).
//!
//! - The sidebar is a lighter sheet held by the rail and the card: its top
//!   left corner rounds 12 px into the rail's ground, and Discord's outline
//!   runs along its top and down its left side (`:2816`, `:2978`).
//! - The card is inset 8 px from the column's sides and foot and sits right
//!   under the sheet (`:2989`); the dock and the account share its ground
//!   and corners, and an outline goes round both (`:3127`).

use iced::border::Radius;
use iced::widget::{column, container, row, space, stack};
use iced::{Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::{Intent, Vm};
use anomi_voice as voice;

use super::outline::Outline;
use super::panels::Docking;
use crate::{account, rail, sidebar};

/// The sheet's top left corner (`:2818`).
const SHEET_CORNER: f32 = 12.0;

/// The left column, as wide as the rail and the sidebar shown.
pub fn view<'a>(vm: &'a Vm, motion: Motion, docking: &Docking) -> impl View<Intent> + 'a {
    let rail = rail::view(&vm.rail, motion).map(Intent::from);
    let sheet = container(sidebar::view(&vm.sidebar, motion).map(Intent::from))
        .width(docking.sidebar)
        .height(Fill)
        .clip(true)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sidebar)),
            border: Border {
                radius: Radius::default().top_left(SHEET_CORNER),
                ..Border::default()
            },
            ..container::Style::default()
        });
    let sheet = stack![sheet, Outline].width(docking.sidebar).height(Fill);
    container(column![row![rail, sheet].height(Fill), card(vm, motion)])
        .width(docking.left())
        .height(Fill)
        .style(catalog::container::rail)
}

/// The account card, with the call's dock on top of it while one goes.
fn card(vm: &Vm, motion: Motion) -> impl View<Intent> + '_ {
    let account = account::view(&vm.account, motion).map(Intent::from);
    let parts: Element<'_, Intent> = if vm.shell.voice_dock {
        column![
            voice::dock::view(&vm.voice.dock, motion).map(Intent::from),
            account
        ]
        .boxed()
    } else {
        account.boxed()
    };
    let outline =
        container(space().width(Fill).height(Fill)).style(|theme: &Theme| container::Style {
            border: Border::default()
                .rounded(metrics::RADIUS_CARD)
                .width(1)
                .color(theme.palette().outline),
            ..container::Style::default()
        });
    container(stack![parts, outline].width(Fill))
        .padding(Padding::new(metrics::ACCOUNT_INSET).top(0.0))
}
