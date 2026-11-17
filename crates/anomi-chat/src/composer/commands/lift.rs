//! A view that takes no room in its column and draws over what is above
//! it, ending a little over its own place. Slint's lists over the composer
//! sit at `composer-box.y - self.height - 8px` in the window, over the
//! timeline (`ui/yapfast.slint:4649`); in the conversation's column the
//! list's place is a slot of no height right over the composer, and the list
//! floats up from it as an overlay of its own layer, above the timeline's
//! text, with the pointer going to it first. Nothing is drawn where the
//! slot is, so a list that is not shown costs nothing.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::widget::float;
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme};

/// Floats `content` up from the slot, its bottom edge `gap` above it.
pub fn lift<'a, M: 'a>(content: impl View<M> + 'a, gap: f32) -> Element<'a, M> {
    let floating =
        float(content.boxed()).translate(move |bounds, _| Vector::new(0.0, -bounds.height - gap));
    Lift {
        content: floating.boxed(),
    }
    .boxed()
}

struct Lift<'a, M> {
    content: Element<'a, M>,
}

impl<M> widget::Meta for Lift<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Lift<'_, M> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(0.0))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let room = Size::new(limits.bounds().width, f32::INFINITY);
        let loose = layout::Limits::new(Size::ZERO, room);
        self.content.layout(&mut tree.children[0], renderer, &loose);
        tree.size = Size::new(tree.children[0].size.width, 0.0);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
