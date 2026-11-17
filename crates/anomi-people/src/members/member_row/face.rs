//! A member's avatar: the usual one with its presence dot and decoration,
//! or, while they are offline, with no dot and faded to 40 % with its
//! decoration (`Avatar.opacity` of `ui/people.slint:294`).

use anomi_theme::prelude::*;
use anomi_vm::kind::Presence;
use anomi_vm::people::members::{Intent, Member};
use anomi_widgets::avatar::{Ground, portrait};

use super::super::OFFLINE_FACE;

/// The avatar's side (`width: 32px`).
pub const SIDE: f32 = 32.0;

pub fn view<'a>(member: &'a Member, motion: Motion) -> Element<'a, Intent> {
    let (presence, opacity) = if member.offline() {
        (Presence::None, OFFLINE_FACE)
    } else {
        (member.presence, 1.0)
    };
    // The dot's ring is cut from the panel's ground (`StatusDot { ring:
    // Theme.sidebar }`, `ui/people.slint:295`).
    portrait(&member.avatar, SIDE)
        .presence(presence)
        .ground(Ground::Sidebar)
        .decoration(member.decoration.as_ref())
        .opacity(opacity)
        .motion(motion)
        .view()
        .boxed()
}
