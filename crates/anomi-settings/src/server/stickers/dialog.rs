//! The sticker form: the picture, a file to browse for (a new sticker only),
//! related emoji, name and description, Cancel and Upload or Update. Slint's
//! `StickerDialog` (`ui/server_expressions.slint:341`). The card is the
//! dialog's own; the backdrop, the way in, Escape (`Intent::Cancel`) and the
//! focus are the layer's.

use iced::widget::{column, container, row, space, text};
use iced::{Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::stickers::{Form, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use crate::kit;

const WIDTH: f32 = 520.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
const TITLE: f32 = 18.0;
const PREVIEW: f32 = 180.0;
const SIDE: f32 = 160.0;

fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: catalog::float::dialog(theme).shadow,
        ..container::Style::default()
    }
}

fn sunken(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().sunken.into()),
        border: Border::default().rounded(8),
        ..container::Style::default()
    }
}

/// A heading over a field.
fn labelled<'a>(
    title: &'a str,
    placeholder: &'a str,
    value: &'a str,
    form: &Form,
    on_input: fn(String) -> Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut input = field(placeholder, value).motion(motion);
    if !form.saving {
        input = input.on_input(on_input).on_submit(Intent::Save);
    }
    column![kit::heading(title), input.view()]
        .spacing(8)
        .boxed()
}

/// The form over the settings; `form` is what `Stickers::form` holds.
pub fn dialog(form: &Form, motion: Motion) -> Element<'_, Intent> {
    let shown: Element<'_, Intent> = match form.preview.ready() {
        Some(handle) => picture(handle.clone())
            .side(SIDE)
            .fit(Fit::Contain)
            .motion(motion)
            .boxed(),
        None => text(tr!("Preview"))
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
            .boxed(),
    };
    let mut parts = column![
        text(if form.editing {
            tr!("Edit Sticker")
        } else {
            tr!("Upload Sticker")
        })
        .size(TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary),
        container(shown)
            .center(Length::Fill)
            .height(PREVIEW)
            .style(sunken),
    ]
    .spacing(SPACING);
    // An edit can't change the file.
    if !form.editing {
        let (name, style): (&str, fn(&Theme) -> text::Style) = if form.has_file {
            (&form.file_name, catalog::text::primary)
        } else {
            (
                tr!("File should be APNG, JPG, PNG or GIF (512 KB max)"),
                catalog::text::secondary,
            )
        };
        let name = text(name)
            .size(13)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(style);
        let mut browse =
            action_button(tr!("Browse"), Kind::Secondary, motion).enabled(!form.saving);
        if !form.saving {
            browse = browse.on_press(Intent::Browse);
        }
        parts = parts.push(
            column![
                kit::heading(tr!("FILE")),
                row![container(name).width(Length::Fill), browse]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
            ]
            .spacing(8)
            .boxed(),
        );
    }
    parts = parts
        .push(labelled(
            tr!("RELATED EMOJI"),
            tr!("Type or paste an emoji"),
            &form.tags,
            form,
            Intent::Tags,
            motion,
        ))
        .push(labelled(
            tr!("STICKER NAME"),
            tr!("e.g.: cat hug"),
            &form.name,
            form,
            Intent::Name,
            motion,
        ))
        .push(labelled(
            tr!("DESCRIPTION"),
            "",
            &form.description,
            form,
            Intent::Description,
            motion,
        ))
        .push(
            text(tr!(
                "Descriptions are optional but must be more than 1 character."
            ))
            .size(typography::size::CAPTION)
            .style(catalog::text::secondary)
            .boxed(),
        );
    if !form.error.is_empty() {
        parts = parts.push(
            text(&form.error)
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Length::Fill)
                .boxed(),
        );
    }
    let mut cancel = action_button(tr!("Cancel"), Kind::Ghost, motion).enabled(!form.saving);
    let go = form.may_save && !form.saving;
    let mut send = action_button(
        if form.saving {
            tr!("Saving...")
        } else if form.editing {
            tr!("Update")
        } else {
            tr!("Upload")
        },
        Kind::Primary,
        motion,
    )
    .enabled(go);
    if !form.saving {
        cancel = cancel.on_press(Intent::Cancel);
    }
    if go {
        send = send.on_press(Intent::Save);
    }
    parts = parts.push(
        row![space().width(Length::Fill), cancel, send]
            .spacing(8)
            .boxed(),
    );
    container(parts)
        .padding(PADDING)
        .width(WIDTH)
        .style(card)
        .boxed()
}
