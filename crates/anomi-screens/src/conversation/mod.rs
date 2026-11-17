//! The conversation column: the header, its line, and under it what the
//! open place shows beside the panels docked at its right, on the chat's
//! ground, with the window's top outline running on across it. The Slint
//! side: the conversation `Rectangle` of `YapfastWindow`
//! (`ui/yapfast.slint:3135`-`:4493`).
//!
//! - [`banners`]: the local preview's line and the notice, at the top.
//! - [`body`]: a call above the messages and what fills the rest: the
//!   timeline over the composer, Friends, requests, Browse Channels, a
//!   forum, a voice channel's stage; a forum post's bar ([`forum_post`]).
//! - [`beside`]: search results, the thread beside a channel, or the
//!   member list and a DM's profile, and the edges that resize them.
//!
//! The areas inside are their tasks' views (`anomi-chat`, `anomi-people`,
//! `anomi-voice`, the header's N07), F01's stubs until those pass.

mod banners;
mod beside;
mod body;
mod call;
mod forum_post;

use iced::widget::{column, container, row, rule, stack};
use iced::{Fill, Size};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_vm::{Intent, Vm};

use crate::header;
use crate::shell::Docking;

/// The column at `size`, the window less its bar, as `docking` lays it out.
pub fn view(vm: &Vm, motion: Motion, docking: Docking, size: Size) -> impl View<Intent> + '_ {
    let main = column![
        banners::view(vm, motion),
        body::view(vm, motion, size.height),
    ]
    .width(Fill)
    .height(Fill);
    let below = stack![
        row![main, beside::view(vm, motion, &docking)].height(Fill),
        beside::edges(&docking, motion, size.height),
    ]
    .width(Fill)
    .height(Fill);
    let column = column![
        header::view(&vm.header, motion).map(Intent::from),
        rule::horizontal(1).style(catalog::rule::outline_chat),
        below,
    ];
    stack![
        container(column)
            .width(docking.room)
            .height(Fill)
            .style(catalog::container::chat),
        // The window's top outline carries on over the conversation to its
        // right edge (`:4487`).
        rule::horizontal(1).style(catalog::rule::outline),
    ]
    .width(docking.room)
    .height(Fill)
}
