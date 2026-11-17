//! A pair of buttons whose last part, the caret, opens a menu from the
//! pair: it tells the window where the pair is, as `menu::opener` does for
//! a whole widget. The microphone's toggle and its caret are one
//! `device_toggle`, whose caret can't tell where it is, so this watches
//! the caret's strip of it. The content sees every event first and keeps
//! its looks; the strip's press is the only one this answers.
//!
//! Every widget between it and the window must forward `overlay` with its
//! translation, as iced's own do (see `menu::opener`).

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

type OnPress<'a, M> = Box<dyn Fn(Rectangle) -> M + 'a>;

pub struct Anchored<'a, M> {
    content: Element<'a, M>,
    /// The width of the caret's strip, at the content's right end.
    strip: f32,
    on_press: OnPress<'a, M>,
}

/// `content` with its last `strip` pixels opening a menu: a press there
/// sends `on_press` with the content's bounds in the window.
pub fn anchored<'a, M>(
    content: impl View<M> + 'a,
    strip: f32,
    on_press: impl Fn(Rectangle) -> M + 'a,
) -> Anchored<'a, M> {
    Anchored {
        content: content.boxed(),
        strip,
        on_press: Box::new(on_press),
    }
}

#[derive(Default)]
struct State {
    /// How far the window is from this widget's layout.
    translation: Vector,
    pressed: bool,
}

impl<M> Anchored<'_, M> {
    fn caret(&self, bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: bounds.x + bounds.width - self.strip,
            width: self.strip,
            ..bounds
        }
    }
}

impl<M> widget::Meta for Anchored<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Anchored<'_, M> {
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
        let over = cursor.is_over(self.caret(bounds));
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.pressed = over;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if std::mem::take(&mut state.pressed) && over =>
            {
                let shown = Rectangle::new(bounds.position() + state.translation, bounds.size());
                shell.publish((self.on_press)(shown));
                shell.capture_event();
            }
            _ => {}
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
        let over = cursor.is_over(self.caret(layout.bounds()));
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        let inner = self
            .content
            .mouse_interaction(tree, layout, cursor, viewport, renderer);
        if inner == mouse::Interaction::None && over {
            mouse::Interaction::Pointer
        } else {
            inner
        }
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
