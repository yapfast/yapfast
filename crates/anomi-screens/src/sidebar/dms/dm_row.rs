//! One DM in the list: the avatar with its presence, decoration and Discord
//! mark, the name with its tag, the line under it, and at its end a Wave
//! button, a call mark, a pin and the mentions. The Slint side: `ChannelRow`
//! with `person` (`ui/rail.slint:397`).
//!
//! A row draws twice, as iced's `hover` lays one over the other: at rest,
//! and as it looks under the pointer (the hover fill, the avatar's ring cut
//! from it, the Wave button, its nameplate and avatar playing). The one
//! under the pointer is the one that answers.

use iced::widget::{button, container, hover, pin, row, space, stack};
use iced::{Alignment, Fill};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::sidebar::dms::{DmRow, Intent};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::badge::{badge, discord_badge};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::nameplate::{Place, nameplate};
use yapfast_lang::tr;

use super::gutter::gutter;
use super::lines::text_column;
use super::look::{Look, style};

/// The call mark's and the pin's width (`ui/rail.slint:668`, `:671`).
const CALL: f32 = 16.0;
const PIN: f32 = 14.0;
/// The Discord mark's disc, at the avatar's top left (`:602`).
const DISCORD: f32 = 13.0;
const DISCORD_AT: f32 = -2.0;

/// The row of `dm`, with its strip at the sidebar's edge.
pub fn view<'a>(dm: &'a DmRow, motion: Motion) -> Element<'a, Intent> {
    let rest = body(dm, false, Motion::default());
    // Under the pointer it lies over the row at rest, so it is painted on
    // the sidebar first, whatever its fill lets through.
    let over = container(body(dm, true, motion)).style(|theme: &Theme| container::Style {
        background: Some(iced::Background::Color(theme.palette().sidebar)),
        border: iced::Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    });
    let id = dm.id.clone();
    let face = opener(hover(rest, over))
        .on_right_press(move |at| Intent::Menu(id.clone(), anomi_vm::menu::Spot::pointer(at)));
    row![
        gutter(dm.unread && !dm.selected),
        container(face).width(Fill),
        space().width(metrics::ROW_INSET),
    ]
    .boxed()
}

/// The row in its look at rest or under the pointer.
fn body<'a>(dm: &'a DmRow, hovered: bool, motion: Motion) -> Element<'a, Intent> {
    let look = Look {
        selected: dm.selected,
        hovered,
        unread: dm.unread,
    };
    let lit = dm.selected || hovered;
    let ring = if lit { Ground::Active } else { Ground::Sidebar };
    let face = portrait(&dm.avatar, metrics::AVATAR_ROW)
        .presence(dm.presence)
        .ground(ring)
        .decoration(dm.decoration.as_ref())
        .motion(motion)
        .view();
    let face: Element<'a, Intent> = if dm.discord {
        stack![
            face,
            pin(discord_badge(DISCORD, ring))
                .x(DISCORD_AT)
                .y(DISCORD_AT)
        ]
        .boxed()
    } else {
        face.boxed()
    };
    let faded = dm.muted && !lit;
    let mut content = row![face, text_column(dm, look, faded, motion)]
        .spacing(metrics::SPACE_M)
        .align_y(Alignment::Center);
    // Discord offers a wave in a DM nobody has written in yet.
    if lit && !dm.wave.is_empty() {
        content = content.push(
            action_button(tr!("Wave"), Kind::Primary, motion)
                .small()
                .leading(Icon::Hand)
                .on_press(Intent::Wave(dm.wave.clone()))
                .boxed(),
        );
    }
    if dm.call {
        content = content.push(
            icon(Icon::Phone, CALL)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().live),
                })
                .boxed(),
        );
    }
    if dm.pinned {
        content = content.push(
            icon(Icon::Pin, PIN)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().faint),
                })
                .boxed(),
        );
    }
    if !dm.badge.is_empty() {
        content = content.push(badge(&dm.badge).boxed());
    }
    let content = nameplate(
        container(content)
            .padding([0.0, metrics::SPACE_S])
            .center_y(metrics::DM_ROW_HEIGHT),
        Place::Dm,
        motion,
    )
    .plate(&dm.plate)
    .selected(dm.selected)
    .radius(metrics::RADIUS_ROW);
    button(content)
        .padding(0)
        .width(Fill)
        .height(metrics::DM_ROW_HEIGHT)
        .on_press(Intent::Open(dm.id.clone()))
        .style(move |theme: &Theme, _status| style(theme, look))
        .boxed()
}
