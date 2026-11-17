//! What the two dialogs share: the card, its title, the field with its
//! heading and the row of buttons (`OwnershipFrame`,
//! `ui/server_ownership.slint:38`).

use iced::advanced::widget::Id;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Color, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::ownership::{Intent, Ownership};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use super::{CANCEL, CONFIRM};
use crate::kit::heading;

/// `min(440px, root.width - 48px)` wide, `padding: 20px; spacing: 12px`.
const WIDTH: f32 = 440.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
/// The dialog's shadow: `#00000066`, 12 px down, 32 px blur.
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_OFFSET: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;
/// The title: 18 px, weight 700.
const TITLE: f32 = 18.0;
/// The words under the title: 15 px, secondary.
const WORDS: f32 = typography::size::BODY;
/// The heading over a field, the field and its error (`spacing: 6px`).
const FIELD_GAP: f32 = 6.0;

/// The card with `parts` in a column, 440 px wide (the window is never
/// narrow enough for Slint's `- 48px` to matter). Rows that shrink sit at
/// its end, as the buttons do.
pub fn card<'a>(parts: Vec<Element<'a, Intent>>) -> impl View<Intent> + 'a {
    container(column(parts).spacing(SPACING).align_x(Alignment::End))
        .width(WIDTH)
        .padding(PADDING)
        .style(dialog)
}

fn dialog(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_OFFSET),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// The title, bold, wrapping.
pub fn title(title: &str) -> Element<'_, Intent> {
    text(title)
        .size(TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary)
        .width(Fill)
        .boxed()
}

/// Discord's words: its text, with the names bold.
pub fn words(ownership: &Ownership) -> Element<'_, Intent> {
    paragraph(&ownership.text)
        .size(WORDS)
        .color(|palette| palette.secondary)
        .width(Fill)
        .boxed()
}

/// A line in the danger red: 14 px above the buttons, 12 px under a field.
pub fn problem<'a>(words: &'a str, size: f32) -> Element<'a, Intent> {
    text(words)
        .size(size)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.palette().danger),
            selection: None,
        })
        .width(Fill)
        .boxed()
}

/// A heading, the field under it and what is `wrong` with it. The field
/// has no placeholder (Slint's `label` is the screen reader's), and takes
/// the typing, and Enter confirms, only while nothing is sent.
pub fn typed<'a>(
    heading_text: &'a str,
    id: &'static str,
    ownership: &'a Ownership,
    wrong: Option<&'a str>,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut input = field("", &ownership.typed).id(id).motion(motion);
    if !ownership.sending {
        input = input.on_input(Intent::Write).on_submit(Intent::Confirm);
    }
    let mut parts = column![heading(heading_text), input.view()]
        .spacing(FIELD_GAP)
        .width(Fill);
    if let Some(wrong) = wrong {
        parts = parts.push(problem(wrong, typography::size::CAPTION));
    }
    parts.boxed()
}

/// Cancel, ghost, and the red button; Cancel waits while it is sent.
pub fn buttons<'a>(
    confirm: &'a str,
    confirm_enabled: bool,
    ownership: &Ownership,
    motion: Motion,
) -> Element<'a, Intent> {
    row![
        action_button(tr!("Cancel"), Kind::Ghost, motion)
            .id(Id::new(CANCEL))
            .on_press(Intent::Cancel)
            .enabled(!ownership.sending),
        action_button(confirm, Kind::Danger, motion)
            .id(Id::new(CONFIRM))
            .on_press(Intent::Confirm)
            .enabled(confirm_enabled),
    ]
    .spacing(metrics::SPACE_S)
    .boxed()
}
