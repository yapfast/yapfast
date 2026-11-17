//! Widgets that lay their content out under other limits than the room they
//! are given. Task N07's (docs/iced/tasks/shell.md). After iced's `Pin`.
//!
//! - [`unbounded`]: as wide as the content likes, and as wide as the room
//!   it is given: a line of text that runs on and is cut by whatever clips
//!   it, not wrapped at the last word that fits.
//! - [`capped`]: no taller than a height, whatever room there is.
//! - [`least`]: no narrower than a width, even where the room left is less
//!   (Slint's `min-width`): it overflows, the others' room is theirs.

use iced::advanced::layout;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Layout, Shell, Widget};
use iced::{Event, Length, Rectangle, Size, Vector, mouse};

/// `content` under other limits (see the module's doc).
pub(super) struct Limit<W> {
    content: W,
    unbounded: bool,
    most: f32,
    least: f32,
}

pub(super) fn unbounded<W>(content: W) -> Limit<W> {
    Limit {
        content,
        unbounded: true,
        most: f32::INFINITY,
        least: 0.0,
    }
}

pub(super) fn capped<W>(content: W, most: f32) -> Limit<W> {
    Limit {
        content,
        unbounded: false,
        most,
        least: 0.0,
    }
}

pub(super) fn least<W>(content: W, least: f32) -> Limit<W> {
    Limit {
        content,
        unbounded: false,
        most: f32::INFINITY,
        least,
    }
}

impl<W> widget::Meta for Limit<W> {}

impl<W, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Limit<W>
where
    W: Widget<Message, Theme, Renderer>,
    Renderer: iced::advanced::Renderer,
{
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        let size = self.content.size();
        if self.unbounded {
            Size::new(Length::Fill, size.height)
        } else {
            size
        }
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let size = self.size();
        let limits = limits.width(size.width).height(size.height);
        let room = limits.bounds();
        let width = if self.unbounded {
            f32::INFINITY
        } else {
            room.width.max(self.least)
        };
        self.content.layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(
                Size::new(self.least, 0.0),
                Size::new(width, room.height.min(self.most)),
            ),
        );
        tree.children[0].translation = Vector::ZERO;
        let mut resolved = limits.resolve(size.width, size.height, tree.children[0].size);
        resolved.width = resolved.width.max(self.least);
        tree.size = resolved;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        if !self.unbounded {
            self.content
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
            return;
        }
        // The line runs on past its area, and a clip over the viewport alone
        // leaves the quads (a mention's pill) at their full-width places:
        // a layer of the area's own keeps all of it inside.
        if let Some(clip) = bounds.intersection(viewport) {
            renderer.with_layer(clip, |renderer| {
                self.content
                    .draw(tree, renderer, theme, style, layout, cursor, &clip);
            });
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
