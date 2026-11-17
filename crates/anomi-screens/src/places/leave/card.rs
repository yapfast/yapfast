//! What the two confirmations share: the dialog's card, its title and its
//! row of buttons, as Slint's `ChannelDialog` and `LeavingDialog` draw them
//! (`ui/server.slint:517`, `ui/dialogs.slint:518`).

use iced::widget::{column, container, row, space, text};
use iced::{Border, Color, Fill, Length, Shadow, Vector};

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

/// A card `width` wide with `content` in it, a column of its parts. The
/// layer lays it out within the window less its margins, so a narrow window
/// keeps it inside (`min(440px, root.width - 48px)`).
pub fn card<'a, M: 'a>(width: f32, parts: Vec<Element<'a, M>>) -> impl View<M> + 'a {
    container(column(parts).spacing(SPACING))
        .padding(PADDING)
        .width(Length::Fixed(width))
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
            color: Color::from_rgba8(0, 0, 0, 0.4),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
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
}

/// The title with the X at its end, on one line that elides.
pub fn title_row<'a, M: Clone + 'a>(words: &'a str, close: M, motion: Motion) -> Element<'a, M> {
    let x = icon_button(Icon::X, tr!("Close"), motion)
        .size(28.0)
        .glyph(18.0)
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
        space().width(Fill),
        action_button(cancel.0, Kind::Ghost, motion).on_press(cancel.1),
        action_button(confirm.0, confirm.1, motion)
            .on_press(confirm.2)
            .enabled(enabled),
    ]
    .spacing(metrics::SPACE_S)
    .boxed()
}
