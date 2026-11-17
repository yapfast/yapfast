//! The fields of a form, each as `BotModalDialog`'s repeater draws its kind
//! (`ui/interactions.slint:226`-`:318`): the label (bold, with a star when
//! required) and its description, then the part itself.

use std::borrow::Cow;

use iced::widget::{button, column, container, hover, row, svg, text};
use iced::{Alignment, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::bot_forms::{Field, Intent, Kind};
use anomi_vm::menu::Spot;
use anomi_widgets::button::icon_button;
use anomi_widgets::composer::composer;
use anomi_widgets::field::field as text_field;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use super::marks::{group, mark_row};

/// Between a field's parts and between fields (`spacing: 8px`, `16px`).
const PART_GAP: f32 = 8.0;
pub const FIELD_GAP: f32 = 16.0;
/// A label (`font-size: 14px`, bold), its description (13 px), a text to
/// read and what a part can't do (15, 14).
const LABEL: f32 = typography::size::SMALL;
const DETAIL: f32 = typography::size::LABEL;
/// A select's box: its height, its corners, how far it fades while it can't
/// be used (`opacity: 0.5`) and the gap to its dropdown (`gap: 4px`).
const BOX_HEIGHT: f32 = 40.0;
const BOX_FADE: f32 = 0.5;
pub const DROP_GAP: f32 = 4.0;
/// A paragraph input (`height: 100px`, `x: 10px`, `y: 8px`).
const AREA_HEIGHT: f32 = 100.0;
const AREA_PAD_X: f32 = 10.0;
const AREA_PAD_Y: f32 = 8.0;
/// A chosen file's row and the upload's (`height: 40px`, `56px`).
const FILE_HEIGHT: f32 = 40.0;
const UPLOAD_HEIGHT: f32 = 56.0;
const FILE_ICON: f32 = 18.0;
const UPLOAD_ICON: f32 = 20.0;

pub fn field<'a>(
    index: usize,
    field: &'a Field,
    ready: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut parts = column(Vec::<Element<'_, Intent>>::new()).spacing(PART_GAP);
    // A checkbox's label is its row's.
    if !field.label.is_empty() && field.kind != Kind::Check {
        parts = parts.push(heading(field));
    }
    parts
        .push(match field.kind {
            Kind::Text => paragraph(&field.text)
                .size(typography::size::BODY)
                .on_link(Intent::Link)
                .boxed(),
            Kind::Unsupported => text(tr!("Yapfast can't fill this part yet."))
                .size(LABEL)
                .style(catalog::text::secondary)
                .width(Fill)
                .boxed(),
            Kind::Select => select_box(index, field, ready),
            Kind::Input if !field.paragraph => {
                let input = text_field(&field.placeholder, &field.value);
                if ready {
                    input
                        .on_input(move |value| Intent::Edit(index, value))
                        .view()
                } else {
                    input.view()
                }
            }
            Kind::Input => area(index, field, ready, motion),
            Kind::Radio => group(&field.choices, true, ready, |value| {
                Intent::Edit(index, value.to_owned())
            }),
            Kind::Checks => group(&field.choices, false, ready, |value| {
                Intent::Edit(index, value.to_owned())
            }),
            Kind::Check => mark_row(
                label(field),
                &field.description,
                field.checked,
                false,
                ready.then(|| Intent::Edit(index, String::new())),
            ),
            Kind::Files => files(index, field, ready, motion),
        })
        .boxed()
}

/// The label with its star, and its description under it (`spacing: 2px`).
fn heading(field: &Field) -> Element<'_, Intent> {
    let mut lines = column![
        text(label(field))
            .size(LABEL)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .boxed()
    ]
    .spacing(2.0);
    if !field.description.is_empty() {
        lines = lines.push(
            text(&field.description)
                .size(DETAIL)
                .style(catalog::text::secondary)
                .width(Fill)
                .boxed(),
        );
    }
    lines.boxed()
}

fn label(field: &Field) -> Cow<'_, str> {
    if field.required {
        Cow::Owned(format!("{} *", field.label))
    } else {
        Cow::Borrowed(&field.label)
    }
}

/// A select field: the box with what is picked, or its placeholder, and a
/// chevron (`BotSelectBox`, `ui/bot_select.slint:15`). Pressing it opens the
/// dropdown under it; the pointer over it turns its rim from the edge to the
/// muted text colour.
fn select_box(index: usize, field: &Field, ready: bool) -> Element<'_, Intent> {
    let picked = !field.summary.is_empty();
    let words = if picked {
        field.summary.as_str()
    } else if !field.placeholder.is_empty() {
        field.placeholder.as_str()
    } else {
        tr!("Make a selection")
    };
    let face = |over: bool| {
        let fade = if ready { 1.0 } else { BOX_FADE };
        let ink = move |theme: &Theme| {
            let palette = theme.palette();
            let ink = if picked {
                palette.text
            } else {
                palette.secondary
            };
            ink.scale_alpha(fade)
        };
        container(
            row![
                text(words)
                    .size(typography::size::BODY)
                    .font(typography::MEDIUM)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .style(move |theme: &Theme| text::Style {
                        color: Some(ink(theme)),
                        ..text::Style::default()
                    })
                    .width(Fill),
                icon(Icon::ChevronDown, FILE_ICON).style(move |theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().secondary.scale_alpha(fade)),
                }),
            ]
            .spacing(PART_GAP)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(12.0).right(12.0))
        .height(BOX_HEIGHT)
        .width(Fill)
        .align_y(Alignment::Center)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let rim = if over && ready {
                palette.secondary
            } else {
                palette.edge
            };
            container::Style {
                background: Some(palette.field.scale_alpha(fade).into()),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(rim.scale_alpha(fade)),
                ..container::Style::default()
            }
        })
    };
    let boxed = hover(face(false), face(true));
    if ready {
        opener(boxed)
            .on_press(move |bounds| Intent::OpenSelect(index, Spot::below(bounds).gap(DROP_GAP)))
            .boxed()
    } else {
        face(false).boxed()
    }
}

/// A paragraph input: a sunken box 100 px tall, the text wrapping inside.
/// Enter breaks the line here (the composer's send is not wired).
fn area(index: usize, field: &Field, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let mut input = composer::<Intent>(&field.value, motion)
        .key(index as u64)
        .placeholder(field.placeholder.as_str())
        .size(typography::size::BODY)
        .pad(AREA_PAD_Y)
        .min_height(AREA_HEIGHT);
    if ready {
        input = input.on_edit(move |value, _| Intent::Edit(index, value));
    }
    container(input)
        .padding([0.0, AREA_PAD_X - 6.0])
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sunken.into()),
            border: Border::default().rounded(metrics::RADIUS_ROW),
            ..container::Style::default()
        })
        .boxed()
}

/// The files chosen, each with its X, and the box that chooses more while
/// there is room for them.
fn files(index: usize, field: &Field, ready: bool, motion: Motion) -> Element<'_, Intent> {
    let mut list = column(Vec::<Element<'_, Intent>>::new()).spacing(6.0);
    for (at, name) in field.files.iter().enumerate() {
        let remove = icon_button(Icon::X, tr!("Remove"), motion)
            .size(28.0)
            .glyph(16.0)
            .enabled(ready)
            .on_press(Intent::RemoveFile(index, at))
            .view();
        list = list.push(
            container(
                row![
                    icon(Icon::File, FILE_ICON),
                    text(name)
                        .size(LABEL)
                        .wrapping(text::Wrapping::None)
                        .ellipsis(text::Ellipsis::End)
                        .style(catalog::text::primary)
                        .width(Fill),
                    remove,
                ]
                .spacing(PART_GAP)
                .align_y(Alignment::Center),
            )
            .padding(Padding::ZERO.left(10.0).right(4.0))
            .height(FILE_HEIGHT)
            .align_y(Alignment::Center)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().sunken.into()),
                border: Border::default().rounded(metrics::RADIUS_ROW),
                ..container::Style::default()
            })
            .boxed(),
        );
    }
    if field.files.len() < field.max_files {
        list = list.push(upload(index, field, ready));
    }
    list.boxed()
}

/// "Choose files", "Up to 3", in a box whose rim turns to the accent.
fn upload(index: usize, field: &Field, ready: bool) -> Element<'_, Intent> {
    let content = row![
        icon(Icon::CirclePlus, UPLOAD_ICON),
        text(tr!("Choose files"))
            .size(LABEL)
            .font(typography::MEDIUM)
            .style(catalog::text::primary),
        text(tr!("Up to {}", field.max_files))
            .size(DETAIL)
            .style(catalog::text::secondary),
    ]
    .spacing(PART_GAP)
    .align_y(Alignment::Center);
    button(container(content).center(Fill))
        .padding(0)
        .width(Fill)
        .height(UPLOAD_HEIGHT)
        .on_press_maybe(ready.then_some(Intent::ChooseFiles(index)))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                background: Some(palette.sunken.into()),
                text_color: palette.text,
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(if status == button::Status::Hovered {
                        palette.accent
                    } else {
                        palette.edge
                    }),
                ..button::Style::default()
            }
        })
        .boxed()
}
