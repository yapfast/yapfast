//! The server's name as a button: its badge, the name, a chevron that turns
//! over while the menu is open (`ServerHeader`, `ui/rail.slint:946`). Its
//! fill is [`lit`](super::lit); a press asks for the menu below it.

use iced::widget::{container, row, stack, svg, text};
use iced::{Alignment, Color, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::server_header::{Intent, ServerHeader};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use iced::widget::text::{Ellipsis, Wrapping};

use super::chevron::chevron;
use super::lit::lit;

/// The badge, the chevron: 16 px each.
const GLYPH: f32 = 16.0;
/// The room left of a badge, and the gap between it and the name.
const BADGE_PAD: f32 = 10.0;
const BADGE_GAP: f32 = 10.0;

pub fn name(header: &ServerHeader, motion: Motion) -> impl View<Intent> + '_ {
    let on_banner = header.has_banner();
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
        .size(typography::size::BODY)
        .font(typography::SEMIBOLD)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
        .width(Length::Shrink)
        .style(move |theme: &Theme| text::Style {
            color: Some(if on_banner {
                Color::WHITE
            } else {
                theme.palette().text
            }),
            ..Default::default()
        });
    // The chevron sits against the end of the name, or of what is left of
    // it when it elides: the label's box keeps the chevron's room.
    let title = stack![
        container(label).padding(Padding::ZERO.right(GLYPH)),
        container(chevron(header.open, on_banner, motion))
            .align_right(Fill)
            .center_y(Fill),
    ];
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
    .keys(|bounds| Intent::Menu(Spot::below(bounds)))
}
