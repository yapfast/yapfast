//! What the open place shows under the header and the banners: a call
//! above the rest, a forum post's bar, then the timeline over the composer,
//! Friends with Active Now, message requests, Browse Channels, a forum's
//! posts, or a voice channel's stage filling it all. The Slint side: the
//! conversation's `VerticalLayout` (`ui/yapfast.slint:3451`-`:4323`).
//!
//! What sits above the composer is laid out at its size (`responsive`), as
//! the Slint layout shares the room left beside the composer between a
//! DM's call and its messages, and shows Active Now only in a wide column.

use iced::widget::{Column, column, container, responsive, row};
use iced::{Fill, Size};

use anomi_chat as chat;
use anomi_people as people;
use anomi_theme::prelude::*;
use anomi_vm::conversation::{Body, Call};
use anomi_vm::{Intent, Vm};

use super::{call, forum_post};
use crate::browse;

/// Friends show Active Now at their right from this width, 360 px wide
/// (`:4069`, `:4079`).
const ACTIVE_NOW_FROM: f32 = 1000.0;
const ACTIVE_NOW_WIDTH: f32 = 360.0;

/// The body; `workspace` is the window's height less its bar, which bounds
/// a call above the messages.
pub fn view(vm: &Vm, motion: Motion, workspace: f32) -> Element<'_, Intent> {
    let conversation = &vm.conversation;
    if conversation.body == Body::Voice {
        return call::panel(vm, motion);
    }
    let above = responsive(move |area| above(vm, motion, area, workspace));
    if conversation.body.composes() {
        column![
            above,
            chat::composer::view(&vm.chat.composer, motion).map(Intent::from),
        ]
        .width(Fill)
        .height(Fill)
        .boxed()
    } else {
        above.boxed()
    }
}

/// What sits above the composer, at `area`: the call, the forum post's
/// bar, and what fills the rest.
fn above(vm: &Vm, motion: Motion, area: Size, workspace: f32) -> Element<'_, Intent> {
    let conversation = &vm.conversation;
    let post = conversation
        .forum_post
        .filter(|_| conversation.body != Body::Forum);
    let mut parts: Column<Element<'_, Intent>> = Column::new().width(Fill).height(Fill);
    if conversation.call != Call::None {
        // The forum post's bar keeps its height; the call shares the rest.
        let room = area.height - post.map_or(0.0, |_| forum_post::BAR);
        parts = parts.push(call::above(vm, motion, room, workspace));
    }
    if let Some(post) = post {
        parts = parts.push(forum_post::view(post, motion).boxed());
    }
    parts.push(filled(place(vm, motion, area.width))).boxed()
}

/// What fills the body under its call and bar.
fn place(vm: &Vm, motion: Motion, width: f32) -> Element<'_, Intent> {
    match vm.conversation.body {
        Body::Nothing | Body::Messages => timeline(vm, motion),
        Body::Friends => {
            let friends = filled(
                people::friends::view(&vm.people.friends, motion)
                    .map(Intent::from)
                    .boxed(),
            );
            if width >= ACTIVE_NOW_FROM {
                let active = container(
                    people::active_now::view(&vm.people.active_now, motion).map(Intent::from),
                )
                .width(ACTIVE_NOW_WIDTH)
                .height(Fill);
                row![friends, active].boxed()
            } else {
                friends
            }
        }
        Body::Requests => people::friends::requests::view(&vm.people.requests, motion)
            .map(Intent::from)
            .boxed(),
        Body::Browse => browse::view(&vm.browse, motion).map(Intent::from).boxed(),
        Body::Forum => chat::forum::view(&vm.chat.forum, motion)
            .map(Intent::from)
            .boxed(),
        // A voice channel fills the body with its call; see `view`.
        Body::Voice => call::panel(vm, motion),
    }
}

/// A view filling the body.
fn filled(view: Element<'_, Intent>) -> Element<'_, Intent> {
    container(view).width(Fill).height(Fill).boxed()
}

fn timeline(vm: &Vm, motion: Motion) -> Element<'_, Intent> {
    chat::timeline::view(&vm.chat.timeline, motion)
        .map(Intent::from)
        .boxed()
}
