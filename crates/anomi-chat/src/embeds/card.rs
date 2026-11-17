//! One embed: a bar of its colour, its texts, fields and pictures, its
//! thumbnail beside them. Ported from `EmbedView`'s body.

use iced::widget::{column, container, mouse_area, row, space, stack};
use iced::{Border, Fill, Length, Padding};

use anomi_theme::Palette;
use anomi_theme::prelude::*;
use anomi_vm::chat::embeds::{Card, Field, Intent, Thumbnail};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_vm::text::Text;
use anomi_widgets::menu::opener;
use anomi_widgets::paragraph::paragraph;
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::skeleton::skeleton;

use super::gallery;

/// The card's widest: `max-width: 432px`.
const WIDEST: f32 = 432.0;
/// The bar: `width: 4px`; the card's corners: `border-radius: 4px`.
const BAR: f32 = 4.0;
const RADIUS: f32 = 4.0;
/// `padding: 12px; padding-left/right: 16px; spacing: 16px`.
const PAD_Y: f32 = 12.0;
const PAD_X: f32 = 16.0;
const GAP: f32 = 16.0;
/// The texts' column: `spacing: 8px`; a field's: `spacing: 2px`; fields'
/// row: `spacing: 8px`.
const STACK: f32 = 8.0;
const FIELD_STACK: f32 = 2.0;
const FIELDS: f32 = 8.0;
/// The thumbnail: 80 px square.
pub const THUMBNAIL: f32 = 80.0;
/// `padding-top: 4px`.
const ABOVE: f32 = 4.0;

use anomi_theme::typography::size::{CAPTION as SMALL, SMALL as BODY, TITLE};

type Ink = fn(&Palette) -> iced::Color;

fn text<'a>(words: &'a Text, size: f32, ink: Ink, motion: Motion) -> Element<'a, Intent> {
    paragraph(words)
        .size(size)
        .color(ink)
        .heading_color(|palette| palette.text)
        .width(Fill)
        .motion(motion)
        .on_link(Intent::Link)
        .boxed()
}

fn field<'a>(field: &'a Field, motion: Motion) -> Element<'a, Intent> {
    column![
        text(&field.name, BODY, |p| p.text, motion),
        text(&field.value, BODY, |p| p.secondary, motion),
    ]
    .spacing(FIELD_STACK)
    .width(Fill)
    .boxed()
}

fn thumbnail<'a>(item: u64, thumb: &'a Thumbnail, motion: Motion) -> Element<'a, Intent> {
    let face: Element<'a, Intent> = match &thumb.picture {
        Expected::Ready(handle) => picture(handle.clone())
            .side(THUMBNAIL)
            .radius(RADIUS)
            .fit(Fit::Contain)
            .motion(motion)
            .boxed(),
        Expected::Waiting => skeleton(motion)
            .width(THUMBNAIL)
            .height(THUMBNAIL)
            .radius(RADIUS)
            .boxed(),
        Expected::None => space().width(THUMBNAIL).height(THUMBNAIL).boxed(),
    };
    let index = thumb.index;
    let face = mouse_area(face)
        .on_press(Intent::View { item, index })
        .interaction(iced::mouse::Interaction::Pointer);
    opener(face)
        .on_right_press(move |at| Intent::Menu {
            item,
            index,
            video: false,
            at: Spot::pointer(at),
        })
        .boxed()
}

/// The card, left-aligned in the message's width, at most 432 px wide.
pub fn view<'a>(item: u64, card: &'a Card, motion: Motion) -> Element<'a, Intent> {
    let mut body = column![].spacing(STACK).width(Fill);
    if let Some(words) = &card.provider {
        body = body.push(
            paragraph(words)
                .size(SMALL)
                .color(|p| p.muted)
                .max_lines(1)
                .width(Fill)
                .motion(motion)
                .on_link(Intent::Link)
                .boxed(),
        );
    }
    if let Some(words) = &card.author {
        body = body.push(text(words, BODY, |p| p.text, motion));
    }
    if let Some(words) = &card.title {
        body = body.push(text(words, TITLE, |p| p.text, motion));
    }
    if let Some(words) = &card.description {
        body = body.push(text(words, BODY, |p| p.secondary, motion));
    }
    for fields in &card.rows {
        body = body.push(
            row(fields.iter().map(|one| field(one, motion)))
                .spacing(FIELDS)
                .width(Fill)
                .boxed(),
        );
    }
    if let Some(gallery) = &card.gallery {
        let beside = if card.thumbnail.is_some() {
            THUMBNAIL + GAP
        } else {
            0.0
        };
        body = body.push(gallery::view(item, &card.url, gallery, beside, motion));
    }
    if let Some(words) = &card.footer {
        body = body.push(text(words, SMALL, |p| p.muted, motion));
    }
    let mut inner = row![body].spacing(GAP);
    if let Some(thumb) = &card.thumbnail {
        inner = inner.push(thumbnail(item, thumb, motion));
    }
    let ground = container(inner)
        .padding([PAD_Y, PAD_X])
        .width(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sidebar.into()),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        });
    // The bar lies over the ground's left padding, its full height: a
    // stack takes its size from the ground.
    let color = card.color;
    let bar = container(space().width(BAR).height(Fill))
        .height(Fill)
        .style(move |theme: &Theme| container::Style {
            background: Some(color.unwrap_or(theme.palette().edge).into()),
            border: Border::default().rounded(iced::border::left(RADIUS)),
            ..container::Style::default()
        });
    container(stack![ground, bar].height(Length::Shrink))
        .width(Length::Fill.max(WIDEST))
        .padding(Padding::ZERO.top(ABOVE))
        .boxed()
}
