//! What is docked at the conversation's right, below the header: search
//! results (C21's `search`, 420 px), or the thread beside a channel or the
//! form starting one (C24's `threads::panel`, C25's `threads::new`), or the
//! member list (P06's Activity over P05's list, on the chat's ground with
//! its 1 px edge) and a DM's profile (P07's card, 8 px in from its sides).
//! The thread's and the member list's left edges resize them. The Slint
//! side: `ui/yapfast.slint:4325`-`:4483` and the edges at `:4517`, `:4538`.
//!
//! A panel takes its room at once. Slint fades what's in it in
//! (`Popup.opacity`), but only on the GPU; tiny-skia, as Skia Software,
//! shows it whole, as `anomi_theme::motion::Popup` says of every card.

use iced::widget::{Row, column, container, row, rule, space};
use iced::{Fill, Point, Rectangle, Size};

use anomi_chat as chat;
use anomi_people as people;
use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::conversation::Aside;
use anomi_vm::shell::Panel;
use anomi_vm::{Intent, Vm};

use crate::shell::{Docking, Grows, SEARCH_WIDTH, STRIP, Strip, edge};

/// The docked profile's inset (`ClientStyle.profile-inset`).
const PROFILE_INSET: f32 = 8.0;

/// The docked panels, as `docking` lays them out.
pub fn view<'a>(vm: &'a Vm, motion: Motion, docking: &Docking) -> Element<'a, Intent> {
    if docking.search {
        return container(chat::search::view(&vm.chat.search, motion).map(Intent::from))
            .width(SEARCH_WIDTH)
            .height(Fill)
            .boxed();
    }
    if let Some((aside, width)) = docking.aside {
        let panel = match aside {
            Aside::Thread => chat::threads::panel::view(&vm.chat.thread_panel, motion)
                .map(Intent::from)
                .boxed(),
            Aside::NewThread => chat::threads::new::view(&vm.chat.new_thread, motion)
                .map(Intent::from)
                .boxed(),
        };
        return container(panel).width(width).height(Fill).boxed();
    }
    let members = docking.members.map(|width| {
        let list = column![
            people::activity::view(&vm.people.activity, motion).map(Intent::from),
            container(people::members::view(&vm.people.members, motion).map(Intent::from))
                .height(Fill),
        ]
        .width(Fill)
        .height(Fill);
        container(row![
            rule::vertical(1).style(catalog::rule::outline_chat),
            list
        ])
        .width(width)
        .height(Fill)
        .style(catalog::container::chat)
    });
    let profile = docking.profile.then(|| {
        container(people::profile::card::view(&vm.people.profile, motion).map(Intent::from))
            .padding(PROFILE_INSET)
            .width(metrics::PROFILE_WIDTH)
            .height(Fill)
    });
    Row::with_children(
        members
            .map(|members| members.boxed())
            .into_iter()
            .chain(profile.map(|profile| profile.boxed())),
    )
    .height(Fill)
    .boxed()
}

/// The edge of the thread beside or the member list, over the area under
/// the header, `height` tall at most.
pub fn edges(docking: &Docking, motion: Motion, height: f32) -> Element<'static, Intent> {
    let (panel, width, right, bounds) = if let Some((_, width)) = docking.aside {
        (Panel::Aside, width, 0.0, docking.aside_bounds())
    } else if let Some(width) = docking.members {
        (
            Panel::Members,
            width,
            docking.profile_width(),
            docking.members_bounds(),
        )
    } else {
        return space().boxed();
    };
    let x = docking.room - right - width - STRIP / 2.0;
    let strip = Rectangle::new(Point::new(x, 0.0), Size::new(STRIP, height));
    edge(panel, Grows::Left, Strip::At(strip, width), bounds, motion).boxed()
}
