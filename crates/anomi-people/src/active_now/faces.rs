//! The stack of faces at the end of a row: each 22 px in a 26 px ring of the
//! card's ground, overlapping by 6, and "+N" for those beyond them. The
//! Slint side: `Faces` (`ui/active_now.slint:71`).

use iced::Border;
use iced::widget::{Stack, container, pin, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::kind::Avatar;
use anomi_widgets::avatar::circle;

/// A face and its ring, and how far each overlaps the one before.
const RING: f32 = 26.0;
const FACE: f32 = 22.0;
const OVERLAP: f32 = 6.0;
/// The "+N" disc's text (`:96`).
const MORE: f32 = 10.0;

/// A disc of the card's ground around `face`.
fn ring<'a, M: 'a>(face: Element<'a, M>) -> Element<'a, M> {
    container(face)
        .center(RING)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().raised.into()),
            border: Border::default().rounded(RING / 2.0),
            ..container::Style::default()
        })
        .boxed()
}

pub fn view<'a, M: 'a>(faces: &'a [Avatar], more: u32) -> impl View<M> + 'a {
    let mut rings: Vec<Element<'a, M>> = faces
        .iter()
        .map(|face| ring(circle(face, FACE).boxed()))
        .collect();
    if more > 0 {
        let disc = container(
            text(format!("+{more}"))
                .size(MORE)
                .font(typography::BOLD)
                .style(catalog::text::secondary),
        )
        .center(FACE)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sunken.into()),
            border: Border::default().rounded(FACE / 2.0),
            ..container::Style::default()
        });
        rings.push(ring(disc.boxed()));
    }
    let width = rings.len().saturating_sub(1) as f32 * (RING - OVERLAP) + RING;
    // Each child after the first has its own layer, so its ring covers the
    // face before it, as Slint's later Rectangle does.
    let mut layers = Stack::new().width(width).height(RING);
    for (at, one) in rings.into_iter().enumerate() {
        layers = layers.push(pin(one).x(at as f32 * (RING - OVERLAP)).boxed());
    }
    layers
}
