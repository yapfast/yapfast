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

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::sidebar::dms::{DmRow, Intent};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::badge::{discord_badge, faded_badge};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::nameplate::{Place, nameplate};
use yapfast_lang::tr;

use super::gutter::gutter;
use super::lines::{FAINT, fade, text_column};
use super::look::{Look, style};

/// The Wave button (ui/rail.slint:637): 24 px tall, as wide as its label
/// and 40 px, its 14 px hand 4 px before 12 px text, 4 px corners. Its 8 px
/// padding and the 6 px its content is centred in come to 11 px a side.
const WAVE: f32 = 24.0;
const WAVE_HAND: f32 = 14.0;
const WAVE_PAD: f32 = 11.0;
const WAVE_GAP: f32 = 4.0;
const WAVE_RADIUS: f32 = 4.0;
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
    // The row's fill (`ui/rail.slint:520`).
    let ring = if dm.selected {
        Ground::Active
    } else if hovered {
        Ground::Hover
    } else {
        Ground::Sidebar
    };
    // A muted row at rest is faded (`ui/rail.slint:596`): its avatar, tag
    // and badge here, its lines in `lines`; the presence dot keeps its own.
    let faded = dm.muted && !lit;
    let strength = if faded { FAINT } else { 1.0 };
    let face = portrait(&dm.avatar, metrics::AVATAR_ROW)
        .presence(dm.presence)
        .ground(ring)
        .decoration(dm.decoration.as_ref())
        .opacity(strength)
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
    let mut content = row![face, text_column(dm, look, faded, motion)]
        .spacing(metrics::SPACE_M)
        .align_y(Alignment::Center);
    // Discord offers a wave in a DM nobody has written in yet.
    if lit && !dm.wave.is_empty() {
        content = content.push(
            action_button(tr!("Wave"), Kind::Primary, motion)
                .small()
                .height(WAVE)
                .leading(Icon::Hand)
                .glyph(WAVE_HAND)
                .text_style(typography::size::CAPTION, typography::MEDIUM)
                .padding(WAVE_PAD, WAVE_GAP)
                .radius(WAVE_RADIUS)
                .on_press(Intent::Wave(dm.wave.clone()))
                .boxed(),
        );
    }
    if dm.call {
        content = content.push(
            icon(Icon::Phone, CALL)
                .style(move |theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(fade(theme.palette().live, faded)),
                })
                .boxed(),
        );
    }
    if dm.pinned {
        content = content.push(
            icon(Icon::Pin, PIN)
                .style(move |theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(fade(theme.palette().faint, faded)),
                })
                .boxed(),
        );
    }
    if !dm.badge.is_empty() {
        content = content.push(faded_badge(&dm.badge, strength).boxed());
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
