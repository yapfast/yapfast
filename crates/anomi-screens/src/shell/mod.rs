//! The main window: the rail and the sidebar on the left with the account
//! card across their foot, the conversation beside them. The Slint side: the
//! root of `ui/yapfast.slint`.

use iced::widget::{column, container, row, rule, stack};
use iced::{Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::{Intent, Vm};

use crate::{account, conversation, rail, sidebar};

/// The whole window, from the view-model and whether things may move.
pub fn window(vm: &Vm, motion: Motion) -> impl View<Intent> + '_ {
    let rail = rail::view(&vm.rail, motion).map(Intent::Rail);
    let sidebar = container(sidebar::view(&vm.sidebar).map(Intent::Sidebar))
        .width(vm.shell.sidebar_width)
        .height(Fill)
        .style(catalog::container::sidebar);
    let left = row![rail, sidebar].height(Fill);
    // The account card floats over the foot of the rail and the sidebar,
    // inset from their sides, as Discord's user panel does.
    let card = container(account::view(&vm.account).map(Intent::Account))
        .padding(Padding::new(metrics::ACCOUNT_INSET).top(0.0))
        .width(metrics::RAIL_WIDTH + vm.shell.sidebar_width)
        .height(Fill)
        .align_bottom(Fill);
    let left = stack![left, card]
        .width(Length::Fixed(metrics::RAIL_WIDTH + vm.shell.sidebar_width))
        .height(Fill);
    let edge = rule::vertical(1).style(catalog::rule::outline);
    row![
        left,
        edge,
        column![conversation::view(&vm.conversation, &vm.shell)]
    ]
    .width(Fill)
    .height(Fill)
}
