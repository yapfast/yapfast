//! The round call button: a 64 by 48 pill in the caller's colour with a
//! 22 px picture, and its meaning said under it, so it doesn't rest on the
//! icon. The Slint side: `CallButton` (`ui/controls.slint:1070`). Hovering
//! darkens the fill by 15% and lights the caption; a disabled one fades to
//! half.

use iced::Color;

use anomi_theme::{Motion, Palette, Theme};

use super::face::Face;
use super::look::Colours;
use crate::icon::Icon;

/// The pill and its picture (`ui/controls.slint:1083`, `:1090`).
const WIDTH: f32 = 64.0;
const HEIGHT: f32 = 48.0;
const GLYPH: f32 = 22.0;
/// The hover darkening (`ui/controls.slint:1084`).
const HOVER_DARKER: f32 = 0.15;
/// The disabled fade (`:1078`).
const DISABLED: f32 = 0.5;

/// A call button: `fill` is the colour of the pill (the danger colour for
/// hanging up, `live` for answering), `label` what it says underneath.
pub fn call_button<'a, Message>(
    icon: Icon,
    label: &'a str,
    fill: fn(&Palette) -> Color,
    motion: Motion,
) -> Face<'a, Message> {
    let mut face = Face::new(motion, move |theme: &Theme, hovered, enabled, _open| {
        let palette = theme.palette();
        let base = fill(palette);
        let ground = if hovered {
            darker(base, HOVER_DARKER)
        } else {
            base
        };
        let fade = if enabled { 1.0 } else { DISABLED };
        Colours {
            fill: ground.scale_alpha(fade),
            ink: palette.on_danger.scale_alpha(fade),
            caption: (if hovered {
                palette.text
            } else {
                palette.secondary
            })
            .scale_alpha(fade),
        }
    });
    face.leading = Some(icon);
    face.glyph = GLYPH;
    face.min = iced::Size::new(WIDTH, HEIGHT);
    face.fixed = true;
    face.radius = HEIGHT / 2.0;
    face.caption = Some(label);
    face
}

/// Slint's `darker`: the brightness divided by one more than `factor`.
fn darker(color: Color, factor: f32) -> Color {
    let k = 1.0 / (1.0 + factor);
    Color {
        r: color.r * k,
        g: color.g * k,
        b: color.b * k,
        ..color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darker_keeps_the_alpha_and_dims() {
        let color = Color::from_rgba(0.8, 0.4, 0.2, 0.5);
        let dim = darker(color, 0.15);
        assert_eq!(dim.a, 0.5);
        assert!((dim.r - 0.8 / 1.15).abs() < 1e-6);
    }
}
