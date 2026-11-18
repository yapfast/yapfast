//! The microphone's level as a row of segments (`InputLevel`,
//! `ui/voice.slint:1331`): 24 of them, 10 px high and 3 px apart, lit from the
//! left up to the level in the live colour, faded while muted (heard here,
//! sent nowhere), the rest in a divider's colour.
//!
//! It reads its [`Level`] as it draws, and nothing else: no state, no
//! frame asked. The app stores the level only while the menu shows in
//! front, and asks a redraw only when it lit or put out a segment, so a
//! level builds no view.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::{Border, Color, Length, Rectangle, Size};

use anomi_theme::{Renderer, Theme};
use anomi_vm::kind::Level;

/// How many segments (`for segment in 24`).
pub const SEGMENTS: usize = 24;
const HEIGHT: f32 = 10.0;
const GAP: f32 = 3.0;
const RADIUS: f32 = 2.0;
/// A muted level's fade (`Theme.live.with-alpha(0.4)`).
const MUTED_ALPHA: f32 = 0.4;

pub struct Meter {
    level: Level,
    muted: bool,
}

/// A meter showing `level`, from 0 to 1, as it is when drawn.
pub fn meter(level: &Level, muted: bool) -> Meter {
    Meter {
        level: level.clone(),
        muted,
    }
}

/// Whether segment `index` is lit at `level`: its middle is under it.
pub fn lit(index: usize, level: f32) -> bool {
    (index as f32 + 0.5) / SEGMENTS as f32 <= level
}

impl widget::Meta for Meter {}

impl<M> widget::Widget<M, Theme, Renderer> for Meter {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut widget::Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fixed(HEIGHT), Size::ZERO);
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let palette = theme.palette();
        let bounds = layout.bounds();
        let width = (bounds.width - GAP * (SEGMENTS as f32 - 1.0)) / SEGMENTS as f32;
        let live = if self.muted {
            palette.live.scale_alpha(MUTED_ALPHA)
        } else {
            palette.live
        };
        let level = self.level.get();
        for index in 0..SEGMENTS {
            let colour: Color = if lit(index, level) {
                live
            } else {
                palette.line
            };
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        iced::Point::new(bounds.x + index as f32 * (width + GAP), bounds.y),
                        Size::new(width, bounds.height),
                    ),
                    border: Border::default().rounded(RADIUS),
                    ..Quad::default()
                },
                colour,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_microphone_lights_nothing_and_a_loud_one_everything() {
        assert!((0..SEGMENTS).all(|index| !lit(index, 0.0)));
        assert!((0..SEGMENTS).all(|index| lit(index, 1.0)));
    }

    #[test]
    fn half_a_level_lights_half_the_segments() {
        assert_eq!((0..SEGMENTS).filter(|&index| lit(index, 0.5)).count(), 12);
    }
}
