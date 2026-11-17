//! A member's avatar: the usual one with its presence dot and decoration, or
//! faded to 40 % while they are offline (`Avatar.opacity` of
//! `ui/people.slint:313`).
//!
//! iced at this pin has no layer opacity and `Portrait` has no opacity of
//! its own, so the faded one is drawn here from the picture widget's own
//! opacity (or the initial's colours) and has no decoration, which a layer
//! would fade with it; `Portrait::opacity` is asked for in the review.

use iced::widget::{container, text};
use iced::{Background, Border, Color};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::kind::Avatar;
use anomi_vm::people::members::{Intent, Member};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::picture::{Fit, picture};

use super::super::OFFLINE_FACE;

/// The avatar's side (`width: 32px`).
pub const SIDE: f32 = 32.0;
/// The initial's size as a share of the avatar (`ui/controls.slint:614`).
const INITIAL: f32 = 0.42;

pub fn view<'a>(member: &'a Member, motion: Motion) -> Element<'a, Intent> {
    if member.offline() {
        return faded(&member.avatar, motion);
    }
    // The dot's ring is cut from the panel's ground (`StatusDot { ring:
    // Theme.sidebar }`, `ui/people.slint:314`).
    portrait(&member.avatar, SIDE)
        .presence(member.presence)
        .ground(Ground::Sidebar)
        .decoration(member.decoration.as_ref())
        .motion(motion)
        .view()
        .boxed()
}

fn faded<'a>(avatar: &'a Avatar, motion: Motion) -> Element<'a, Intent> {
    match &avatar.picture {
        Some(handle) => picture(handle.clone())
            .side(SIDE)
            .fit(Fit::Cover)
            .round()
            .opacity(OFFLINE_FACE)
            .motion(motion)
            .boxed(),
        None => {
            let tint = avatar.tint;
            container(
                text(avatar.initial.as_str())
                    .size(SIDE * INITIAL)
                    .font(typography::BOLD)
                    .color(Color::WHITE.scale_alpha(OFFLINE_FACE)),
            )
            .center(SIDE)
            .style(move |theme: &Theme| {
                let base = if tint.a == 0.0 {
                    theme.palette().avatar
                } else {
                    tint
                };
                container::Style {
                    background: Some(Background::Color(base.scale_alpha(OFFLINE_FACE))),
                    border: Border::default().rounded(SIDE / 2.0),
                    ..container::Style::default()
                }
            })
            .boxed()
        }
    }
}
