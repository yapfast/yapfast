//! A sticker on its tile: its picture, name and uploader, with a pencil and
//! an X on hover; and the dashed-looking Upload Sticker tile that follows
//! the last one (`ui/server_expressions.slint:246`, `:322`).

use iced::widget::{column, container, mouse_area, row, stack, svg, text};
use iced::{Alignment, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::stickers::{Intent, Stickers, Tile, UPLOAD};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use crate::kit::pressable;

const HEIGHT: f32 = 160.0;
const PAD: f32 = 10.0;
const PICTURE: f32 = 80.0;
const BUTTON: f32 = 28.0;
const GLYPH: f32 = 16.0;

fn ground(hot: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(
                palette
                    .overlay
                    .scale_alpha(if hot { 0.08 } else { 0.03 })
                    .into(),
            ),
            border: Border::default().rounded(metrics::RADIUS_CARD),
            ..container::Style::default()
        }
    }
}

/// An icon in one colour of the palette, whatever the pointer does.
fn tinted<'a>(name: Icon, side: f32, pick: fn(&Palette) -> Color) -> Element<'a, Intent> {
    icon(name, side)
        .style(move |theme: &Theme, _| svg::Style {
            color: Some(pick(theme.palette())),
        })
        .boxed()
}

fn line(words: &str, size: f32, bold: bool, secondary: bool) -> Element<'_, Intent> {
    let mut line = text(words)
        .size(size)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        // The lines share what the picture leaves, each at its top, as
        // Slint's layout stretched them.
        .height(Length::Fill);
    if bold {
        line = line.font(typography::SEMIBOLD);
    }
    line.style(if secondary {
        catalog::text::secondary
    } else {
        catalog::text::primary
    })
    .boxed()
}

/// A sticker `width` wide; its pencil and X show while the pointer is on it.
pub fn tile<'a>(
    stickers: &'a Stickers,
    sticker: &'a Tile,
    width: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    let hot = stickers.hovered.as_deref() == Some(sticker.id.as_str());
    let art: Element<'a, Intent> = match sticker.picture.ready() {
        Some(handle) => picture(handle.clone())
            .side(PICTURE)
            .fit(Fit::Contain)
            .motion(motion)
            .boxed(),
        // A sticker that can't be drawn keeps a picture's place.
        None => container(tinted(Icon::Sticker, 32.0, |palette| palette.faint))
            .center(PICTURE)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().overlay.scale_alpha(0.04).into()),
                border: Border::default().rounded(metrics::RADIUS_CARD),
                ..container::Style::default()
            })
            .boxed(),
    };
    let body = container(
        column![
            container(art).center_x(Length::Fill),
            line(&sticker.name, typography::size::SMALL, true, false),
            line(&sticker.uploader, typography::size::CAPTION, false, true),
        ]
        .spacing(6),
    )
    .padding(PAD)
    .width(width)
    .height(HEIGHT)
    .style(ground(hot));
    let mut layers = stack![body.boxed()];
    if sticker.may_change && hot {
        let button = |name, tip, danger, intent| {
            icon_button(name, tip, motion)
                .size(BUTTON)
                .glyph(GLYPH)
                .fill(|palette| palette.raised)
                .danger(danger)
                .on_press(intent)
                .view()
        };
        let buttons = row![
            button(
                Icon::Pencil,
                tr!("Edit"),
                false,
                Intent::Edit(sticker.id.clone())
            ),
            button(
                Icon::X,
                tr!("Delete"),
                true,
                Intent::Delete(sticker.id.clone())
            ),
        ]
        .spacing(2);
        layers = layers.push(
            container(buttons)
                .padding(4)
                .width(width)
                .align_x(Alignment::End)
                .boxed(),
        );
    }
    mouse_area(layers)
        .on_enter(Intent::Hover(sticker.id.clone(), true))
        .on_exit(Intent::Hover(sticker.id.clone(), false))
        .boxed()
}

/// The slot where the next sticker goes, `width` wide: a plus and Upload
/// Sticker, its border lit under the pointer.
pub fn upload<'a>(stickers: &Stickers, width: f32, motion: Motion) -> Element<'a, Intent> {
    let hot = stickers.hovered.as_deref() == Some(UPLOAD);
    let face = container(
        column![
            tinted(Icon::CirclePlus, 32.0, |palette| palette.secondary),
            text(tr!("Upload Sticker"))
                .size(typography::size::SMALL)
                .font(typography::SEMIBOLD)
                .align_x(Alignment::Center)
                .style(catalog::text::secondary),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center(Length::Fill)
    .width(width)
    .height(HEIGHT)
    .style(move |theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            border: Border::default()
                .rounded(metrics::RADIUS_CARD)
                .width(2)
                .color(if hot { palette.secondary } else { palette.line }),
            ..container::Style::default()
        }
    });
    let label = tr!("Upload Sticker");
    let button = pressable(
        face.boxed(),
        label,
        Intent::Upload,
        metrics::RADIUS_CARD,
        |theme, _| theme.palette().secondary,
        motion,
    );
    mouse_area(button)
        .on_enter(Intent::Hover(UPLOAD.into(), true))
        .on_exit(Intent::Hover(UPLOAD.into(), false))
        .boxed()
}
