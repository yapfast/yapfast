//! The server's name as a button: its badge, the name, a chevron that turns
//! over while the menu is open (`ServerHeader`, `ui/rail.slint:946`). Its
//! fill is [`lit`](super::lit); a press asks for the menu below it.

use std::f32::consts::PI;

use iced::widget::{row, svg, text};
use iced::{Alignment, Color, Fill, Length, Padding, Rotation};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::server_header::{Intent, ServerHeader};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use iced::widget::text::{Ellipsis, Wrapping};

use super::lit::lit;

/// The badge, the chevron: 16 px each.
const GLYPH: f32 = 16.0;
/// The room left of a badge, and the gap between it and the name.
const BADGE_PAD: f32 = 10.0;
const BADGE_GAP: f32 = 10.0;
/// A name longer than this takes all the room and elides, which keeps its
/// chevron in view; shorter ones leave the chevron right behind them.
const LONG: usize = 24;

pub fn name(header: &ServerHeader, motion: Motion) -> impl View<Intent> + '_ {
    let on_banner = header.banner_picture().is_some();
    // A verified server's green badge takes a Community server's place.
    let badge = if header.verified {
        Some(icon(Icon::Verified, GLYPH))
    } else if header.community {
        Some(
            icon(Icon::Community, GLYPH).style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().text),
            }),
        )
    } else {
        None
    };
    let label = text(header.name.as_str())
        .size(typography::size::TITLE)
        .font(typography::SEMIBOLD)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
        .width(if header.name.chars().count() > LONG {
            Fill
        } else {
            Length::Shrink
        })
        .style(move |theme: &Theme| text::Style {
            color: Some(if on_banner {
                Color::WHITE
            } else {
                theme.palette().text
            }),
            ..Default::default()
        });
    let turn = if header.open { PI } else { 0.0 };
    let chevron = icon(Icon::ChevronDown, GLYPH)
        .rotation(Rotation::Solid(turn.into()))
        .style(move |theme: &Theme, _| svg::Style {
            color: Some(if on_banner {
                Color::WHITE
            } else {
                theme.palette().secondary
            }),
        });
    // The chevron sits right against the name.
    let title = row![label, chevron].align_y(Alignment::Center);
    let left = if badge.is_some() {
        BADGE_PAD
    } else {
        metrics::SPACE_S
    };
    let content = row![]
        .extend(badge.map(|badge| badge.boxed()))
        .push(title.boxed())
        .spacing(BADGE_GAP)
        .align_y(Alignment::Center)
        .width(Fill)
        .height(Fill)
        .padding(Padding::new(0.0).left(left).right(metrics::SPACE_S));
    lit(
        opener(content).on_press(|bounds| Intent::Menu(Spot::below(bounds))),
        header.open,
        motion,
    )
}
