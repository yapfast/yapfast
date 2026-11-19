//! The value over a dial's handle while it's dragged, as Discord's sliders
//! show theirs: small bold text on the tooltip's card, its corners and
//! shadow, centred 6 px above the handle, in a layer of its own so it
//! covers the label it may cross.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::{Color, Point, Rectangle, Shadow, Size, Vector};

use anomi_theme::{Renderer, Theme, metrics, typography};

use crate::menu::text;

/// The card's height, its text's room at each side, and its gap to the
/// handle's top.
const HEIGHT: f32 = 24.0;
const PAD: f32 = 8.0;
const GAP: f32 = 6.0;
const TEXT: f32 = typography::size::CAPTION;
/// The tooltip's shadow, and the room the layer leaves it.
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
const SHADOW_BLUR: f32 = 8.0;
const SHADOW_DROP: f32 = 2.0;

/// Draws `words` over the handle centred at `handle`.
pub(super) fn draw(renderer: &mut Renderer, theme: &Theme, handle: Point, words: &str) {
    let width = text::width(words, TEXT, typography::BOLD) + 2.0 * PAD;
    let top = handle.y - metrics::SLIDER_HANDLE / 2.0 - GAP - HEIGHT;
    let card = Rectangle::new(
        Point::new((handle.x - width / 2.0).round(), top.round()),
        Size::new(width, HEIGHT),
    );
    let palette = theme.palette();
    renderer.with_layer(card.expand(SHADOW_BLUR + SHADOW_DROP), |renderer| {
        renderer.fill_quad(
            Quad {
                bounds: card,
                border: iced::Border::default().rounded(metrics::RADIUS_ROW),
                shadow: Shadow {
                    color: SHADOW,
                    offset: Vector::new(0.0, SHADOW_DROP),
                    blur_radius: SHADOW_BLUR,
                },
                ..Quad::default()
            },
            palette.tooltip,
        );
        let line = text::line_height(TEXT);
        let at = Point::new(card.x + PAD, card.center_y() - line / 2.0);
        let room = card.width - 2.0 * PAD + 1.0;
        text::line(
            renderer,
            words,
            (TEXT, typography::BOLD),
            at,
            room,
            palette.text,
            card,
        );
    });
}
