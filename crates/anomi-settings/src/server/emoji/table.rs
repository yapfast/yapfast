//! The tables of emoji: a heading with its slots and column titles, then a
//! row per emoji with its picture, a name that renames in place, who
//! uploaded it and a delete that shows on hover
//! (`ui/server_expressions.slint:111`).
//!
//! Slint lays the list 8 px past the column on each side and pads its rows
//! back in, so a row's hover reaches past the text above it; here the rows
//! sit on the column's edges, so their contents line up the same and the
//! hover stops at the column.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{column, container, mouse_area, row, space, text};
use iced::{Alignment, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::emoji::{Emoji, Intent, Row, Table};
use anomi_widgets::avatar::circle;
use anomi_widgets::button::icon_button;
use anomi_widgets::field::field;
use anomi_widgets::list::list;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

const ROW: f32 = 52.0;
const HEAD: f32 = 72.0;
const GAP: f32 = 16.0;
/// The name field's inset, which the column title above it shares.
const NAME_PAD: f32 = 8.0;
const IMAGE: f32 = 32.0;
const IMAGE_COLUMN: f32 = 48.0;
const AVATAR: f32 = 24.0;
const DELETE: f32 = 32.0;
const DELETE_GLYPH: f32 = 18.0;
const FIELD: f32 = 34.0;
const COLUMN: f32 = 12.0;

/// A line of the list: a table's heading, or an emoji of it.
enum Line<'a> {
    Head(&'a Table),
    Item(&'a Row),
}

pub fn tables(emoji: &Emoji, motion: Motion) -> Element<'_, Intent> {
    let lines: Vec<Line> = emoji
        .tables
        .iter()
        .filter(|table| !table.rows.is_empty())
        .flat_map(|table| {
            std::iter::once(Line::Head(table)).chain(table.rows.iter().map(Line::Item))
        })
        .collect();
    let count = lines.len();
    let lines = std::rc::Rc::new(lines);
    let keys = lines.clone();
    list(
        count,
        move |i| {
            let mut hasher = DefaultHasher::new();
            match &keys[i] {
                Line::Head(table) => (0, &table.heading).hash(&mut hasher),
                Line::Item(row) => (1, &row.id).hash(&mut hasher),
            }
            hasher.finish()
        },
        move |i| match &lines[i] {
            Line::Head(table) => head(table),
            Line::Item(row) => item(emoji, row, motion),
        },
        motion,
    )
    .height(Length::Fill)
    .boxed()
}

fn label<'a>(words: &'a str) -> iced::widget::Text<'a, Theme> {
    text(words)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::secondary)
}

/// A table's heading, its slots, and the titles of its columns.
fn head(table: &Table) -> Element<'_, Intent> {
    let title = row![
        text(&table.heading)
            .size(16)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        text(format!("\u{2013} {}", table.slots))
            .size(16)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::secondary),
    ]
    .spacing(6);
    let columns = row![
        label(tr!("IMAGE")).width(IMAGE_COLUMN),
        container(label(tr!("NAME")))
            .padding([0.0, NAME_PAD])
            .width(Length::FillPortion(1)),
        label(tr!("UPLOADED BY")).width(Length::FillPortion(1)),
        space().width(DELETE),
    ]
    .spacing(GAP);
    container(column![title, columns].spacing(COLUMN))
        .padding(iced::Padding {
            top: GAP,
            ..iced::Padding::ZERO
        })
        .height(HEAD)
        .boxed()
}

fn item<'a>(emoji: &'a Emoji, emoji_row: &'a Row, motion: Motion) -> Element<'a, Intent> {
    let hot = emoji.hovered.as_deref() == Some(emoji_row.id.as_str());
    let image: Element<'a, Intent> = match emoji_row.picture.ready() {
        Some(handle) => picture(handle.clone())
            .side(IMAGE)
            .fit(Fit::Contain)
            .boxed(),
        None => space().width(IMAGE).height(IMAGE).boxed(),
    };
    let name: Element<'a, Intent> = if emoji_row.may_change {
        let typed = match &emoji.editing {
            Some((id, typed)) if *id == emoji_row.id => Some(typed.as_str()),
            _ => None,
        };
        let id = emoji_row.id.clone();
        let blurred = emoji_row.id.clone();
        let mut name = field(tr!("Emoji name"), typed.unwrap_or(&emoji_row.name))
            .on_input(move |typed| Intent::Edit(id.clone(), typed))
            .on_submit(Intent::Rename(emoji_row.id.clone()))
            // The keys light the row, as the pointer does; losing them
            // renames by what was typed.
            .on_focus(move |focused| {
                if focused {
                    Intent::Hover(blurred.clone(), true)
                } else {
                    Intent::Rename(blurred.clone())
                }
            })
            .text_size(typography::size::SMALL)
            .pad(NAME_PAD)
            .height(FIELD)
            .motion(motion);
        // The well shows under the pointer or while a name is typed in it.
        if !hot && typed.is_none() {
            name = name.fill(Color::TRANSPARENT);
        }
        name.view()
    } else {
        container(
            text(&emoji_row.name)
                .size(typography::size::SMALL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::primary),
        )
        .padding([0.0, NAME_PAD])
        .boxed()
    };
    let mut by = row![].spacing(NAME_PAD).align_y(Alignment::Center);
    if !emoji_row.uploader.is_empty() {
        by = by.push(circle(&emoji_row.uploader_avatar, AVATAR).boxed());
    }
    by = by.push(
        text(&emoji_row.uploader)
            .size(typography::size::SMALL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed(),
    );
    let delete: Element<'a, Intent> = if emoji_row.may_change && hot {
        icon_button(Icon::X, tr!("Delete"), motion)
            .size(DELETE)
            .glyph(DELETE_GLYPH)
            .danger(true)
            .on_press(Intent::Delete(emoji_row.id.clone()))
            .view()
            .boxed()
    } else {
        space().width(DELETE).height(DELETE).boxed()
    };
    let line = row![
        container(image).width(IMAGE_COLUMN),
        container(name).width(Length::FillPortion(1)),
        container(by).width(Length::FillPortion(1)),
        delete,
    ]
    .spacing(GAP)
    .align_y(Alignment::Center);
    mouse_area(
        container(line)
            .height(ROW)
            .align_y(Alignment::Center)
            .style(move |theme: &Theme| container::Style {
                background: hot.then(|| theme.palette().lift(0.06).into()),
                border: Border {
                    radius: metrics::RADIUS_ROW.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
    )
    .on_enter(Intent::Hover(emoji_row.id.clone(), true))
    .on_exit(Intent::Hover(emoji_row.id.clone(), false))
    .boxed()
}
