//! The main window: the rail and the sidebar on the left with the account
//! card across their foot, the conversation beside them. The Slint side: the
//! root of `ui/yapfast.slint`. Task N01 (docs/iced/tasks/shell.md) takes it
//! further: panels whose edges drag, and the layers every area opens.
//!
//! Each area's view is mapped with `.map(Intent::from)`, which takes an
//! area's own `Intent` or any parent's, so an area may change which it
//! returns without this file changing.

use iced::widget::{column, container, row, rule, stack};
use iced::{Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::{Intent, Vm};

use crate::{account, conversation, rail, sidebar};

/// The whole window, from the view-model and whether things may move.
pub fn window(vm: &Vm, motion: Motion) -> impl View<Intent> + '_ {
    let rail = rail::view(&vm.rail, motion).map(Intent::from);
    let sidebar = container(sidebar::view(&vm.sidebar, motion).map(Intent::from))
        .width(vm.shell.sidebar_width)
        .height(Fill)
        .style(catalog::container::sidebar);
    let left = row![rail, sidebar].height(Fill);
    // The account card floats over the foot of the rail and the sidebar,
    // inset from their sides, as Discord's user panel does.
    let card = container(account::view(&vm.account, motion).map(Intent::from))
        .padding(Padding::new(metrics::ACCOUNT_INSET).top(0.0))
        .width(metrics::RAIL_WIDTH + vm.shell.sidebar_width)
        .height(Fill)
        .align_bottom(Fill);
    let left = stack![left, card]
        .width(Length::Fixed(metrics::RAIL_WIDTH + vm.shell.sidebar_width))
        .height(Fill);
    let edge = rule::vertical(1).style(catalog::rule::outline);
    row![left, edge, column![conversation::view(vm, motion)]]
        .width(Fill)
        .height(Fill)
}

/// The shell's shots: the rail, sidebar, account card and an empty
/// conversation, on the DM home and a server, in two themes and sizes.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::shell;

    use crate::shots::{GOLDEN, SMALL, Shot};

    let shot = |name, size, theme, vm| Shot {
        area: "shell",
        name,
        size,
        theme,
        vm,
    };
    vec![
        shot("dm_home", GOLDEN, "bonfire", shell::dm_home as fn() -> Vm),
        shot("server_home", GOLDEN, "bonfire", shell::server_home),
        shot("dm_home_latte", GOLDEN, "catppuccin-latte", shell::dm_home),
        shot("dm_home_small", SMALL, "bonfire", shell::dm_home),
        shot("connecting", GOLDEN, "bonfire", shell::connecting),
    ]
}
