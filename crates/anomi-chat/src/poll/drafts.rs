//! The answers being made (`PollDialog`'s `for answer`,
//! `ui/dialogs.slint:983`): an emoji well that opens the picker and a badge
//! that takes the emoji off, the answer's field, and its trash.

use iced::widget::{button, container, hover, row, stack};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::chat::poll::{Draft, Intent};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::icon_button;
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use yapfast_lang::tr;

use super::emoji::face;

/// The well (`40px`, `border-radius: 6px`), its emoji and plus-smile.
const WELL: f32 = 40.0;
const RADIUS: f32 = 6.0;
const FACE: f32 = 22.0;
const SMILE: f32 = 20.0;
/// The badge on the corner (`x: width - 13px; y: -5px`, 18 px, 2 px ring,
/// 10 px cross), whose overhang the row keeps room for.
const BADGE: f32 = 18.0;
const RING: f32 = 2.0;
const CROSS: f32 = 10.0;
const BADGE_LEFT: f32 = WELL - 13.0;
pub(super) const OVERHANG: f32 = 5.0;
/// Between the well and the field (`spacing: 12px`) less the overhang.
const GAP: f32 = 12.0;
const FIELD: f32 = 40.0;
const PAD: f32 = 10.0;
const TRASH: f32 = 32.0;
const TRASH_GLYPH: f32 = 18.0;

pub(super) fn view<'a>(
    index: usize,
    draft: &'a Draft,
    several: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let placeholder = tr!("Type your answer");
    let words = field(placeholder, &draft.text)
        .on_input(move |text| Intent::Answer(index, text))
        .on_submit(Intent::Post)
        .text_size(typography::size::BODY)
        .pad(PAD)
        .height(FIELD)
        .motion(motion);
    let mut line = row![well(index, draft), words.view()]
        .spacing(GAP - OVERHANG)
        .align_y(Alignment::End);
    if several {
        line = line.push(
            container(
                icon_button(Icon::Trash, tr!("Remove answer"), motion)
                    .size(TRASH)
                    .glyph(TRASH_GLYPH)
                    .on_press(Intent::RemoveAnswer(index))
                    .view(),
            )
            .align_y(Alignment::Center)
            .height(FIELD)
            .boxed(),
        );
    }
    line.boxed()
}

/// The well and its badge, in a box that keeps the badge's overhang.
fn well<'a>(index: usize, draft: &'a Draft) -> Element<'a, Intent> {
    let emoji = draft.emoji.as_ref();
    let content = |lit| -> Element<'a, Intent> {
        let inside: Element<'a, Intent> = match emoji {
            Some(emoji) => face(emoji, FACE),
            None => icon(Icon::SmilePlus, SMILE)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().secondary),
                })
                .boxed(),
        };
        container(inside)
            .center(Fill)
            .width(WELL)
            .height(WELL)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(Background::Color(if lit {
                        palette.hover
                    } else {
                        palette.sunken
                    })),
                    border: Border::default().rounded(RADIUS),
                    ..container::Style::default()
                }
            })
            .boxed()
    };
    let pick = opener(hover(content(false), content(true)))
        .on_press(move |bounds| Intent::PickEmoji(index, Spot::beside(bounds)));
    let mut layers = stack![container(pick).padding(Padding::ZERO.top(OVERHANG))];
    if emoji.is_some() {
        layers = layers.push(
            container(remove(index))
                .padding(Padding::ZERO.left(BADGE_LEFT))
                .boxed(),
        );
    }
    layers
        .width(WELL + OVERHANG)
        .height(WELL + OVERHANG)
        .boxed()
}

/// The badge: an 18 px disc in the card's colour with a 14 px one in it,
/// faint, red under the pointer.
fn remove<'a>(index: usize) -> Element<'a, Intent> {
    let disc = |lit: bool| -> Element<'a, Intent> {
        let cross = icon(Icon::X, CROSS).style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().text),
        });
        let inner = container(cross).center(Fill).style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(if lit {
                    palette.danger
                } else {
                    palette.faint
                })),
                border: Border::default().rounded((BADGE - 2.0 * RING) / 2.0),
                ..container::Style::default()
            }
        });
        container(inner)
            .padding(RING)
            .width(BADGE)
            .height(BADGE)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().raised)),
                border: Border::default().rounded(BADGE / 2.0),
                ..container::Style::default()
            })
            .boxed()
    };
    button(hover(disc(false), disc(true)))
        .width(BADGE)
        .height(BADGE)
        .padding(0)
        .on_press(Intent::RemoveEmoji(index))
        .style(|_: &Theme, _| button::Style::default())
        .boxed()
}
