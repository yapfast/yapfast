//! The main window: its bar when Anomi draws its frame, the left column
//! (the rail and the sidebar over the account card) beside the
//! conversation, the edges that resize the panels, and the layers that
//! open over it all. The Slint side: `YapfastWindow` (`ui/yapfast.slint:95`)
//! and its panel widths (`src/ui/callbacks.rs:1964`-`:1983`).
//!
//! The window lays itself out from its width ([`responsive`]): the sidebar
//! shows within its bounds, the member list and a DM's profile give way in
//! a narrow window ([`Docking`]), and each edge knows where it is and how
//! far it may drag.
//!
//! Each area's view is mapped with `.map(Intent::from)`, which takes an
//! area's own `Intent` or any parent's, so an area may change which it
//! returns without this file changing.

pub mod camera;
mod drop_card;
mod edge;
mod layers;
mod left;
mod notes;
mod outline;
mod panels;

pub(crate) use edge::{Grows, STRIP, Strip, edge};
pub use panels::{
    ASIDE_MAX, ASIDE_MIN, ASIDE_WIDTH, CALL_MIN, Docking, MEMBERS_MAX, MEMBERS_MIN, SEARCH_WIDTH,
    aside_width, call_limit, members_width,
};

use iced::widget::{column, responsive, row, space, stack};
use iced::{Fill, Rectangle, Size};

use anomi_theme::prelude::*;
use anomi_vm::shell::Panel;
use anomi_vm::{Intent, Vm};

use crate::{chrome, conversation, sign_in};

/// The whole window, from the view-model and whether things may move: its
/// resize edges over all of it.
pub fn window(vm: &Vm, motion: Motion) -> impl View<Intent> + '_ {
    stack![
        responsive(move |size| frame(vm, motion, size)),
        chrome::edges(&vm.chrome).map(Intent::from),
    ]
    .width(Fill)
    .height(Fill)
}

/// The window at its size: the workspace (or sign-in) below the bar's
/// room, and the layers over it, the bar among them under the popups.
fn frame(vm: &Vm, motion: Motion, window: Size) -> Element<'_, Intent> {
    let top = if vm.chrome.bar() { chrome::HEIGHT } else { 0.0 };
    let docking = Docking::new(&vm.shell, &vm.conversation.beside, window.width);
    let below = Size::new(window.width, (window.height - top).max(0.0));
    let base: Element<'_, Intent> = if vm.shell.signed_out {
        sign_in::view(&vm.sign_in, motion).map(Intent::from).boxed()
    } else {
        workspace(vm, motion, docking, below)
    };
    let aside = match docking.aside {
        Some((_, width)) if !vm.shell.signed_out => width,
        _ => 0.0,
    };
    let frame = layers::Frame {
        window,
        top,
        left: docking.left(),
        aside,
    };
    let bar = chrome::view(&vm.chrome, motion).map(Intent::from).boxed();
    layers::stack(vm, motion, frame, column![space().height(top), base], bar).boxed()
}

/// The account's screens: the left column beside the conversation, and
/// the sidebar's edge between them.
fn workspace(vm: &Vm, motion: Motion, docking: Docking, size: Size) -> Element<'_, Intent> {
    let panels = row![
        left::view(vm, motion, &docking),
        conversation::view(vm, motion, docking, size),
    ];
    let strip = Rectangle::new(
        iced::Point::new(docking.left() - STRIP / 2.0, 0.0),
        Size::new(STRIP, size.height),
    );
    let sidebar = edge(
        Panel::Sidebar,
        Grows::Right,
        Strip::At(strip, docking.sidebar),
        docking.sidebar_bounds(),
        motion,
    );
    stack![panels, sidebar].width(Fill).height(Fill).boxed()
}

/// The shell's shots: the window on the DM home and a server, in two
/// themes and sizes, connecting, signed out, and the window's own states:
/// a layer of each look, the notes, panels dragged and docked, a popup over
/// the window's bar.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::{conversation, shell};

    use crate::shots::{GOLDEN, SMALL, Shot};

    let shot = |name, size, vm| Shot {
        area: "shell",
        name,
        size,
        theme: "bonfire",
        vm,
    };
    let latte = |name, vm| Shot {
        theme: "catppuccin-latte",
        ..shot(name, GOLDEN, vm)
    };
    vec![
        shot("dm_home", GOLDEN, shell::dm_home as fn() -> Vm),
        shot("server_home", GOLDEN, shell::server_home),
        latte("dm_home_latte", shell::dm_home),
        latte("server_home_latte", shell::server_home),
        shot("dm_home_small", SMALL, shell::dm_home),
        shot("server_home_small", SMALL, shell::server_home),
        shot("connecting", GOLDEN, shell::connecting),
        shot("signed_out", GOLDEN, shell::signed_out),
        shot("dialog", GOLDEN, shell::new_dm),
        shot("popover", GOLDEN, shell::pins),
        shot("settings", GOLDEN, shell::settings),
        shot("zoom", GOLDEN, shell::zoomed),
        shot("files_over", GOLDEN, shell::dropping),
        shot("files_refused", GOLDEN, shell::drop_refused),
        shot("files_refused_beside", GOLDEN, shell::drop_refused_beside),
        shot("notice", GOLDEN, shell::notice),
        shot("dragged", GOLDEN, shell::dragged),
        shot("search_beside", GOLDEN, || {
            shell::channel_with(conversation::searching)
        }),
        shot("thread_beside_small", SMALL, || {
            shell::channel_with(conversation::thread_beside)
        }),
        shot("forum_post", GOLDEN, || {
            shell::channel_with(conversation::forum_post)
        }),
        shot("dm_call", GOLDEN, || {
            shell::dm_with(conversation::dm_call_here)
        }),
        shot("dm_call_elsewhere", GOLDEN, || {
            shell::dm_with(conversation::dm_call_elsewhere)
        }),
        shot("voice", GOLDEN, || shell::channel_with(conversation::voice)),
        shot("friends", GOLDEN, || shell::dm_with(conversation::friends)),
        shot("preview", GOLDEN, || shell::dm_with(conversation::preview)),
        shot("popup_over_bar", SMALL, shell::popup_over_bar),
    ]
}
