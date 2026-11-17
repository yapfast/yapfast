//! One of two layers over a tile, by whether the pointer is on it: the
//! tile's buttons at rest, and those it shows under the pointer. The Slint
//! side: the children of `StageTile`'s `TouchArea` that read
//! `has-hover` (`ui/voice.slint:775`-`:815`), where the viewers make room
//! for a stop button that shows under the pointer. iced's `hover` lays its
//! top over its base; here the rest and the hovered differ in place, so
//! only one draws and takes events.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

pub struct Swap<'a, M> {
    /// At rest, then under the pointer.
    children: [Element<'a, M>; 2],
}

/// `rest`, or `hovered` while the pointer is on it; both fill it.
pub fn swap<'a, M>(rest: impl View<M> + 'a, hovered: impl View<M> + 'a) -> Swap<'a, M> {
    Swap {
        children: [rest.boxed(), hovered.boxed()],
    }
}

#[derive(Default)]
struct State {
    hovered: bool,
}

impl<M> Swap<'_, M> {
    fn shown(tree: &Tree) -> usize {
        usize::from(tree.state.downcast_ref::<State>().hovered)
    }
}

impl<M> widget::Meta for Swap<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Swap<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let inside = layout::Limits::new(Size::ZERO, size);
        for (child, tree) in self.children.iter_mut().zip(&mut tree.children) {
            child.layout(tree, renderer, &inside);
        }
        tree.size = size;
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
        let hovered = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        if state.hovered != hovered {
            state.hovered = hovered;
            shell.request_redraw();
        }
        let shown = usize::from(hovered);
        if let Some((child, tree)) = layout.iter_mut(&mut tree.children).nth(shown) {
            self.children[shown].update(tree, event, child, cursor, renderer, shell, viewport);
        }
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
        let shown = Self::shown(tree);
        if let Some((child, tree)) = layout.iter(&tree.children).nth(shown) {
            self.children[shown].draw(tree, renderer, theme, style, child, cursor, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let shown = Self::shown(tree);
        layout
            .iter(&tree.children)
            .nth(shown)
            .map(|(child, tree)| {
                self.children[shown].mouse_interaction(tree, child, cursor, viewport, renderer)
            })
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let shown = Self::shown(tree);
        if let Some((child, tree)) = layout.iter_mut(&mut tree.children).nth(shown) {
            self.children[shown].operate(tree, child, viewport, renderer, operation);
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
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let shown = Self::shown(tree);
        match layout.iter_mut(&mut tree.children).nth(shown) {
            Some((child, tree)) => {
                self.children[shown].overlay(tree, child, renderer, viewport, translation, window)
            }
            None => Vec::new(),
        }
    }
}
