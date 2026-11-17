//! What the two confirmations share: the dialog's card, its title and its
//! row of buttons, as Slint's `ChannelDialog` and `LeavingDialog` draw them
//! (`ui/server.slint:517`, `ui/dialogs.slint:518`).

use iced::advanced::widget::Id;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Color, Fill, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button, icon_button};
use yapfast_lang::tr;

/// The title (`font-size: 18px; font-weight: 700`).
const TITLE: f32 = 18.0;
/// Inside the card, and between its parts (`padding: 20px; spacing: 12px`).
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
/// The shadow under the card: `#00000066`, 12 px down, 32 px blur
/// (`ui/server.slint:528`).
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_OFFSET: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;
/// The X: a 28 px button with an 18 px glyph (`ui/server.slint:552`).
const CLOSE_SIZE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The ids of the two buttons, for focus and for the tests.
pub const CANCEL: &str = "dialog-cancel";
pub const CONFIRM: &str = "dialog-confirm";

/// A card at least `width` wide with `content` in it, a column of its parts,
/// as `max(460px, content.min-width)` (`ui/server.slint:526`): a hairline
/// spacer as wide as the card less its padding sits in the column, so the
/// column is never narrower, and grows with the buttons row, which is as
/// wide as both buttons whole. The wrapped texts fill what the column
/// measures from those two, so they never drive the width. The layer lays
/// it out within the window less its margins.
pub fn card<'a, M: 'a>(width: f32, mut parts: Vec<Element<'a, M>>) -> impl View<M> + 'a {
    parts.push(space().width(width - 2.0 * PADDING).height(0).boxed());
    container(column(parts).spacing(SPACING).align_x(Alignment::End))
        .width(Length::Shrink)
        .padding(iced::Padding::new(PADDING).bottom(PADDING - SPACING))
        .style(dialog)
}

/// A dialog: raised, edged, 12 px corners, and the shadow under it
/// (`drop-shadow-blur: 32px`, `-offset-y: 12px`, `#00000066`).
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

/// A heading, bold, in the text colour.
pub fn title<'a, M: 'a>(words: &'a str, wrapping: text::Wrapping) -> impl View<M> + 'a {
    text(words)
        .size(TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary)
        .width(Fill)
        .wrapping(wrapping)
        .ellipsis(if wrapping == text::Wrapping::None {
            text::Ellipsis::End
        } else {
            text::Ellipsis::None
        })
}

/// The title with the X at its end, on one line that elides.
pub fn title_row<'a, M: Clone + 'a>(words: &'a str, close: M, motion: Motion) -> Element<'a, M> {
    let x = icon_button(Icon::X, tr!("Close"), motion)
        .size(CLOSE_SIZE)
        .glyph(CLOSE_GLYPH)
        .on_press(close)
        .view();
    row![title(words, text::Wrapping::None), x]
        .align_y(iced::Alignment::Center)
        .boxed()
}

/// Cancel (or the alternative) and the main button, at the end.
pub fn buttons<'a, M: Clone + 'a>(
    cancel: (&'a str, M),
    confirm: (&'a str, Kind, M),
    enabled: bool,
    motion: Motion,
) -> Element<'a, M> {
    row![
        action_button(cancel.0, Kind::Ghost, motion)
            .id(Id::new(CANCEL))
            .on_press(cancel.1),
        action_button(confirm.0, confirm.1, motion)
            .id(Id::new(CONFIRM))
            .on_press(confirm.2)
            .enabled(enabled),
    ]
    .spacing(metrics::SPACE_S)
    .boxed()
}
