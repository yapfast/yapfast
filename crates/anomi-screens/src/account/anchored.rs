//! A pair of buttons whose caret opens a menu from the pair: it tells the
//! window where the pair is, as `menu::opener` does for a whole widget. The
//! pair's caret sends `marker` (its `on_menu`) for a click and for Enter or
//! Space while it has the keyboard's focus; this runs the content's update
//! through a shell of its own and swaps the marker for `on_press` of the
//! pair's bounds in the window.
//!
//! Every widget between it and the window must forward `overlay` with its
//! translation, as iced's own do (see `menu::opener`).

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::shell::Bus;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

type OnPress<'a, M> = Box<dyn Fn(Rectangle) -> M + 'a>;

pub struct Anchored<'a, M> {
    content: Element<'a, M>,
    /// What the content's caret sends.
    marker: M,
    on_press: OnPress<'a, M>,
}

/// `content`, whose caret sends `marker`: that message is replaced by
/// `on_press` with the content's bounds in the window.
pub fn anchored<'a, M>(
    content: impl View<M> + 'a,
    marker: M,
    on_press: impl Fn(Rectangle) -> M + 'a,
) -> Anchored<'a, M> {
    Anchored {
        content: content.boxed(),
        marker,
        on_press: Box::new(on_press),
    }
}

#[derive(Default)]
struct State {
    /// How far the window is from this widget's layout.
    translation: Vector,
}

impl<M> widget::Meta for Anchored<'_, M> {}

impl<M: Clone + PartialEq> widget::Widget<M, Theme, Renderer> for Anchored<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
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
        let bounds = layout.bounds();
        let translation = tree.state.downcast_ref::<State>().translation;
        let shown = Rectangle::new(bounds.position() + translation, bounds.size());
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let mut sent = Bus::new();
        let mut inner = shell.local(&mut sent);
        self.content.update(
            child_tree, event, child, cursor, renderer, &mut inner, viewport,
        );
        let (marker, on_press) = (&self.marker, &self.on_press);
        shell.merge(inner, |message| {
            if message == *marker {
                on_press(shown)
            } else {
                message
            }
        });
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
        tree.state.downcast_mut::<State>().translation = translation;
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
