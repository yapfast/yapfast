//! Choices as Discord's Safety Setup lists them: each a row with a radio
//! mark, its title in its colour and what it means, the chosen one lifted.
//! Slint's `SafetyChoices` (`ui/moderation.slint:97`): 6 px corners, 10 px
//! of padding, 12 px between the mark and the words; the mark a 20 px ring
//! (2 px, the accent when chosen, else the secondary colour) with a 10 px
//! dot; an option Discord doesn't offer, or a page saving, is half faded
//! and answers no click.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::settings::safety::{Intent, Offer};

use crate::kit::pressable;

const PAD: f32 = 10.0;
const BESIDE: f32 = 12.0;
const WORDS_GAP: f32 = 2.0;
const GAP: f32 = 4.0;
const RING: f32 = 20.0;
const RING_WIDTH: f32 = 2.0;
const DOT: f32 = 10.0;
/// Where the chosen row lifts, and a faded option's weight.
const CHOSEN: f32 = 0.08;
const FADED: f32 = 0.5;
/// DM Sans's own line, as Slint lays its text (iced's default is taller).
pub(super) const LINE: f32 = 1.3;

/// The radio mark: a ring, a dot inside when `on`.
fn mark<'a>(on: bool, dim: f32) -> Element<'a, Intent> {
    let dot: Element<'a, Intent> = if on {
        container(iced::widget::space())
            .width(DOT)
            .height(DOT)
            .style(move |theme: &Theme| container::Style {
                background: Some(theme.palette().accent.scale_alpha(dim).into()),
                border: Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            })
            .boxed()
    } else {
        iced::widget::space().boxed()
    };
    container(dot)
        .center(RING)
        .width(RING)
        .height(RING)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let ring = if on {
                palette.accent
            } else {
                palette.secondary
            };
            container::Style {
                border: Border::default()
                    .rounded(RING / 2.0)
                    .width(RING_WIDTH)
                    .color(ring.scale_alpha(dim)),
                ..container::Style::default()
            }
        })
        .boxed()
}

fn one<'a>(
    offer: &'a Offer,
    on: bool,
    usable: bool,
    pick: &impl Fn(u8) -> Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    let dim = if offer.enabled { 1.0 } else { FADED };
    let title = offer.color;
    let words = column![
        text(&offer.title)
            .size(typography::size::BODY)
            .line_height(LINE)
            .font(typography::SEMIBOLD)
            .style(move |theme: &Theme| text::Style {
                color: Some(title.unwrap_or(theme.palette().text).scale_alpha(dim)),
                ..text::Style::default()
            }),
        text(&offer.description)
            .size(typography::size::LABEL)
            .line_height(LINE)
            .width(Length::Fill)
            .style(move |theme: &Theme| text::Style {
                color: Some(theme.palette().secondary.scale_alpha(dim)),
                ..text::Style::default()
            }),
    ]
    .spacing(WORDS_GAP)
    .width(Length::Fill);
    let line = row![mark(on, dim), words]
        .spacing(BESIDE)
        .align_y(Alignment::Center)
        .padding(PAD);
    let ground = move |theme: &Theme| container::Style {
        background: on.then(|| theme.palette().lift(CHOSEN).into()),
        border: Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    };
    let content = container(line).width(Length::Fill).style(ground);
    if usable && offer.enabled {
        pressable(
            content.boxed(),
            &offer.title,
            pick(offer.value),
            metrics::RADIUS_ROW,
            |theme: &Theme, _| theme.palette().text,
            motion,
        )
    } else {
        content.boxed()
    }
}

/// The list of `offers`, `chosen` marked by its value; a click on one
/// sends `pick(value)` while the page is `usable`. `_label` names the list
/// for a screen reader.
pub fn choices<'a>(
    _label: &'a str,
    offers: &'a [Offer],
    chosen: u8,
    usable: bool,
    pick: impl Fn(u8) -> Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    column(
        offers
            .iter()
            .map(|offer| one(offer, offer.value == chosen, usable, &pick, motion)),
    )
    .spacing(GAP)
    .width(Length::Fill)
    .boxed()
}
