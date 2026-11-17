//! A permission's line: its title, description and Deny, Passthrough and
//! Allow buttons, with a lock whose tooltip says why it can't change.
//! `ThreeState` and `ThreeStateLine` (`ui/channel_settings.slint:80`,
//! `:113`).

use iced::widget::{column, container, row, rule, svg, text};
use iced::{Border, Color, Fill, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::permissions::{Intent, Line};
use anomi_widgets::icon::icon;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use crate::kit::{Ink, Leaf, mix};

/// The three buttons' box, their glyph and the pad that makes each a third
/// of the 108 px (less the border).
const BOX_RADIUS: f32 = 6.0;
const BUTTON_RADIUS: f32 = 5.0;
const BUTTON_HEIGHT: f32 = 26.0;
const GLYPH: f32 = 16.0;
const PAD: f32 = (108.0 - 2.0) / 3.0 / 2.0 - GLYPH / 2.0;

/// The line's spacing (`ThreeStateLine`).
const GAP: f32 = 4.0;
const ACROSS: f32 = 12.0;
const BELOW: f32 = 12.0;
const LOCK: f32 = 6.0;
const LOCK_ICON: f32 = 14.0;

/// Deny, neither, allow: the chosen one filled.
pub fn three_state<'a>(line: &'a Line, enabled: bool, motion: Motion) -> Element<'a, Intent> {
    let choices = [Icon::X, Icon::Slash, Icon::Check];
    let buttons = choices.into_iter().enumerate().map(|(state, glyph)| {
        let chosen = usize::from(line.state) == state;
        let mut button = Leaf::new("", motion, move |theme: &Theme, lit: f32| {
            let palette = theme.palette();
            let fill = if chosen {
                [palette.danger, palette.faint, palette.live][state]
            } else if enabled {
                mix(Color::TRANSPARENT, palette.hover, lit)
            } else {
                Color::TRANSPARENT
            };
            let glyph = if chosen {
                Color::WHITE
            } else if enabled {
                palette.secondary
            } else {
                palette.faint
            };
            Ink {
                fill,
                text: glyph,
                glyph,
            }
        })
        .enabled(enabled)
        .on_press(Intent::Set(line.bit, state as u8));
        button.icon = Some(glyph);
        button.glyph = GLYPH;
        button.pad = PAD;
        button.height = BUTTON_HEIGHT;
        button.radius = BUTTON_RADIUS;
        let word = match state {
            0 => tr!("Deny"),
            1 => tr!("Passthrough"),
            _ => tr!("Allow"),
        };
        tip(button, word, Side::Above, motion).boxed()
    });
    container(row(buttons))
        .padding(1)
        .style(|theme: &Theme| container::Style {
            border: Border::default()
                .rounded(BOX_RADIUS)
                .width(1)
                .color(theme.palette().line),
            ..container::Style::default()
        })
        .boxed()
}

/// The permission's line; `enabled` is the page's (the account may edit and
/// nothing is on its way).
pub fn line<'a>(line: &'a Line, enabled: bool, motion: Motion) -> Element<'a, Intent> {
    let locked = !line.lock.is_empty();
    let live = enabled && !locked;
    let title = text(&line.title)
        .size(typography::size::BODY)
        .font(typography::MEDIUM)
        .style(if live {
            catalog::text::primary
        } else {
            catalog::text::muted
        });
    let mut name = row![title.boxed()]
        .spacing(LOCK)
        .align_y(alignment::Vertical::Center);
    if locked {
        name = name.push(
            tip(
                icon(Icon::Lock, LOCK_ICON).style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().muted),
                }),
                &line.lock,
                Side::Above,
                motion,
            )
            .boxed(),
        );
    }
    let top = row![
        container(name).width(Fill),
        container(three_state(line, live, motion)).align_y(alignment::Vertical::Center)
    ]
    .spacing(ACROSS)
    .align_y(alignment::Vertical::Center);
    column![
        top,
        text(&line.description)
            .size(typography::size::LABEL)
            .style(catalog::text::secondary),
        rule::horizontal(1).style(catalog::rule::line),
    ]
    .spacing(GAP)
    .padding(Padding::ZERO.bottom(BELOW).right(ACROSS))
    .boxed()
}
