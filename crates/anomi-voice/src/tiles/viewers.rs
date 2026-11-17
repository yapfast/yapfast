//! Who watches a stream, over its top right as Discord shows them
//! (`ViewerStack`, `ui/voice.slint:60`): on a round dark scrim 30 px tall,
//! an eye and how many, then the first three's pictures, 22 px in black
//! rings of 26, each 15 px after the one before, and how many more.
//! Pointing at it names them all, below it. Nobody watching shows nothing.

use iced::widget::{Row, Space, Stack, container, text};
use iced::{Alignment, Background, Border, Color, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::voice::tiles::Viewers;
use anomi_widgets::avatar::portrait;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::tip::{Side, tip};

use super::SCRIM;

const HEIGHT: f32 = 30.0;
/// The eye, 16 px, 8 px from the left and 4 before the count; the
/// pictures 6 px after it (`:70`, `:72`, `:76`).
const EYE: f32 = 16.0;
const LEFT: f32 = 8.0;
const EYE_GAP: f32 = 4.0;
const COUNT_GAP: f32 = 6.0;
/// A picture and its ring, and how far each is from the one before.
const FACE: f32 = 22.0;
const RING: f32 = 26.0;
const STEP: f32 = 15.0;
/// After the last picture: 6 px of ground, of which its ring takes 4; and
/// 4 px before "+N", which its ring fills (`:67`, `:82`).
const RIGHT: f32 = 6.0;
const MORE_GAP: f32 = 4.0;
const COUNT: f32 = typography::size::HEADING;

/// The stack of `viewers`, or nothing when nobody watches.
pub fn viewers<'a, M: 'a>(viewers: &'a Viewers, motion: Motion) -> Element<'a, M> {
    if viewers.count == 0 {
        return Space::new().boxed();
    }
    let number = |value: String| {
        text(value)
            .size(COUNT)
            .font(typography::BOLD)
            .color(Color::WHITE)
    };
    let shown = viewers.faces.len().min(Viewers::FACES);
    let mut parts: Row<Element<'a, M>> = Row::new()
        .align_y(Alignment::Center)
        .height(HEIGHT)
        .push(
            icon(Icon::Eye, EYE)
                .style(anomi_theme::catalog::svg::colored(Color::WHITE))
                .boxed(),
        )
        .push(Space::new().width(EYE_GAP).boxed())
        .push(number(viewers.count.to_string()).boxed());
    // A ring overflows its picture's place by 4 px, eaten from the ground
    // after it, or from the gap before "+N".
    let ring = RING - FACE;
    parts = parts.push(Space::new().width(COUNT_GAP).boxed());
    let mut right = RIGHT;
    if shown > 0 {
        parts = parts.push(faces(&viewers.faces[..shown]));
        right -= ring;
    }
    if viewers.more() > 0 {
        let gap = if shown > 0 { MORE_GAP - ring } else { MORE_GAP };
        parts = parts
            .push(Space::new().width(gap).boxed())
            .push(number(format!("+{}", viewers.more())).boxed());
        right = RIGHT;
    }
    let stack = container(parts)
        .padding(Padding::ZERO.left(LEFT).right(right))
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(SCRIM)),
            border: Border::default().rounded(HEIGHT / 2.0),
            ..container::Style::default()
        });
    tip(stack, viewers.names.as_str(), Side::Below, motion).boxed()
}

/// The pictures, each in its black ring over the one before.
fn faces<'a, M: 'a>(faces: &'a [anomi_vm::kind::Avatar]) -> Element<'a, M> {
    let width = RING + STEP * (faces.len().saturating_sub(1)) as f32;
    let mut layers = Stack::new().width(width).height(RING);
    for (index, face) in faces.iter().enumerate() {
        let ringed = container(portrait(face, FACE).view())
            .center(RING)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::BLACK)),
                border: Border::default().rounded(RING / 2.0),
                ..container::Style::default()
            });
        layers = layers.push(
            container(ringed)
                .padding(Padding::ZERO.left(STEP * index as f32))
                .boxed(),
        );
    }
    layers.boxed()
}
