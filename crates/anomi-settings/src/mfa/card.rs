//! The prompt's card and its title row: raised, edged, 12 px corners, the
//! dialog's shadow, `padding: 20px; spacing: 12px` (`ui/mfa.slint:62`).

use iced::widget::{column, container, row, text};
use iced::{Border, Color, Fill, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::mfa::Intent;
use anomi_widgets::button::icon_button;
use yapfast_lang::tr;

const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
/// The card's least width, `max(440px, ..)` (`ui/mfa.slint:58`); the content
/// is that less the padding on both sides.
pub const WIDTH: f32 = 440.0;
pub const CONTENT_MIN: f32 = WIDTH - 2.0 * PADDING;
/// The dialog's shadow: `drop-shadow-blur: 32px; drop-shadow-color:
/// #00000066; drop-shadow-offset-y: 12px` (`ui/mfa.slint:60`).
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_OFFSET: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;
/// The X: `size: 28px; glyph: 18px` (`ui/mfa.slint:74`).
const X_SIZE: f32 = 28.0;
const X_GLYPH: f32 = 18.0;
/// The title (`font-size: 18px; font-weight: 700`).
const TITLE: f32 = 18.0;

/// A card as wide as its widest fixed-size part (the button row's probe,
/// see `mfa::buttons`), and at least [`WIDTH`].
pub fn card<'a>(parts: Vec<Element<'a, Intent>>) -> impl View<Intent> + 'a {
    container(column(parts).spacing(SPACING).width(Length::Shrink))
        .padding(PADDING)
        .width(Length::Shrink)
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

/// "Multi-Factor Authentication" with the X, which cancels.
pub fn title_row<'a>(motion: Motion) -> Element<'a, Intent> {
    let x = icon_button(Icon::X, tr!("Cancel"), motion)
        .size(X_SIZE)
        .glyph(X_GLYPH)
        .on_press(Intent::Cancel)
        .view();
    row![
        text(tr!("Multi-Factor Authentication"))
            .size(TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        x
    ]
    .align_y(iced::Alignment::Center)
    .boxed()
}
