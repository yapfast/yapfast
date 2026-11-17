//! The edges and corners that resize a window drawn without the desktop's
//! frame. The Slint side: `ResizeEdges` (`ui/title_bar.slint:87`): 5 px
//! strips along the sides and 8 px squares at the corners, each with its
//! resize cursor; a left press on one asks for the resize from there.
//!
//! [`Edges`] fills what it is laid over (the window) and takes only those
//! strips: the pointer anywhere else passes to what is underneath.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree};
use iced::advanced::{Shell, mouse, renderer};
use iced::{Event, Length, Point, Rectangle, Size};

use anomi_theme::{Renderer, Theme};
use anomi_vm::chrome::Edge;

/// A strip's width and a corner's side (`ui/title_bar.slint:89`, `:90`).
pub const EDGE: f32 = 5.0;
pub const CORNER: f32 = 8.0;

/// The edge or corner of a window of `size` under `at`, in the window's own
/// coordinates; the corners win over the strips they cut short.
pub fn edge_at(size: Size, at: Point) -> Option<Edge> {
    if at.x < 0.0 || at.y < 0.0 || at.x > size.width || at.y > size.height {
        return None;
    }
    let west = at.x
        < if at.y < CORNER || at.y >= size.height - CORNER {
            CORNER
        } else {
            EDGE
        };
    let east = at.x
        >= size.width
            - if at.y < CORNER || at.y >= size.height - CORNER {
                CORNER
            } else {
                EDGE
            };
    let north = at.y
        < if at.x < CORNER || at.x >= size.width - CORNER {
            CORNER
        } else {
            EDGE
        };
    let south = at.y
        >= size.height
            - if at.x < CORNER || at.x >= size.width - CORNER {
                CORNER
            } else {
                EDGE
            };
    match (north, south, west, east) {
        (true, _, true, _) => Some(Edge::NorthWest),
        (true, _, _, true) => Some(Edge::NorthEast),
        (_, true, true, _) => Some(Edge::SouthWest),
        (_, true, _, true) => Some(Edge::SouthEast),
        (true, ..) => Some(Edge::North),
        (_, true, ..) => Some(Edge::South),
        (_, _, true, _) => Some(Edge::West),
        (_, _, _, true) => Some(Edge::East),
        _ => None,
    }
}

/// The cursor an edge shows.
pub fn cursor_for(edge: Edge) -> mouse::Interaction {
    match edge {
        Edge::East | Edge::West => mouse::Interaction::ResizingHorizontally,
        Edge::North | Edge::South => mouse::Interaction::ResizingVertically,
        Edge::NorthEast | Edge::SouthWest => mouse::Interaction::ResizingDiagonallyUp,
        Edge::NorthWest | Edge::SouthEast => mouse::Interaction::ResizingDiagonallyDown,
    }
}

pub struct Edges<M> {
    on_resize: fn(Edge) -> M,
}

pub fn edges<M>(on_resize: fn(Edge) -> M) -> Edges<M> {
    Edges { on_resize }
}

impl<M> Edges<M> {
    fn under(layout: Layout, cursor: mouse::Cursor) -> Option<Edge> {
        let bounds = layout.bounds();
        let at = cursor.position_over(bounds)?;
        edge_at(bounds.size(), Point::new(at.x - bounds.x, at.y - bounds.y))
    }
}

impl<M> widget::Meta for Edges<M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Edges<M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(edge) = Self::under(layout, cursor)
        {
            shell.publish((self.on_resize)(edge));
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        Self::under(layout, cursor).map_or(mouse::Interaction::None, cursor_for)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1000.0, 600.0);

    fn at(x: f32, y: f32) -> Option<Edge> {
        edge_at(WINDOW, Point::new(x, y))
    }

    #[test]
    fn strips_and_corners_as_resize_edges_has_them() {
        assert_eq!(at(995.5, 300.0), Some(Edge::East));
        assert_eq!(at(500.0, 2.0), Some(Edge::North));
        assert_eq!(at(2.0, 300.0), Some(Edge::West));
        assert_eq!(at(500.0, 597.0), Some(Edge::South));
        assert_eq!(at(995.0, 3.0), Some(Edge::NorthEast));
        assert_eq!(at(3.0, 3.0), Some(Edge::NorthWest));
        assert_eq!(at(995.0, 597.0), Some(Edge::SouthEast));
        assert_eq!(at(7.0, 593.0), Some(Edge::SouthWest));
    }

    #[test]
    fn the_far_strips_include_their_first_pixel_as_slints_do() {
        assert_eq!(at(995.0, 300.0), Some(Edge::East));
        assert_eq!(at(500.0, 595.0), Some(Edge::South));
        assert_eq!(at(992.0, 3.0), Some(Edge::NorthEast));
        assert_eq!(at(6.0, 592.0), Some(Edge::SouthWest));
    }

    #[test]
    fn the_inside_belongs_to_the_window() {
        assert_eq!(at(500.0, 300.0), None);
        assert_eq!(at(6.0, 300.0), None);
        assert_eq!(at(500.0, 6.0), None);
        // 6 px down the side is past the 5 px strip but inside the corner's 8.
        assert_eq!(at(6.0, 6.0), Some(Edge::NorthWest));
        assert_eq!(at(-1.0, 300.0), None);
    }
}
