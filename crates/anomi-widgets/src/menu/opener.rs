//! What a menu or a popover opens from: a wrapper that tells, when clicked
//! or right-clicked, where it is in the window. The Slint window reads
//! `self.absolute-position` in the handler that opens each popup (the
//! server header at `ui/yapfast.slint:2840`, the bell at `:3357`, a right
//! click at `ui/rail.slint:230`); iced's layouts are relative to the
//! scrollables they're in, so this keeps the offset iced hands overlays
//! (`overlay`'s `translation`) and adds it.
//!
//! Every widget between it and the window must forward `overlay` with its
//! translation, as iced's containers, rows, scrollables and buttons do.
//!
//! Its content still sees the pointer, so it keeps its hover and pressed
//! looks; it shouldn't send a press of its own as well.
//!
//! ```ignore
//! opener(header_look).on_press(|button| Intent::Menu(Spot::below(button)))
//! opener(row).on_right_press(|at| Intent::RowMenu(id, Spot::pointer(at)))
//! ```

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

type OnPress<'a, M> = Box<dyn Fn(Rectangle) -> M + 'a>;
type OnRightPress<'a, M> = Box<dyn Fn(Point) -> M + 'a>;

pub struct Opener<'a, M> {
    content: Element<'a, M>,
    on_press: Option<OnPress<'a, M>>,
    on_right_press: Option<OnRightPress<'a, M>>,
}

/// Wraps what opens a menu or a popover.
pub fn opener<'a, M>(content: impl View<M> + 'a) -> Opener<'a, M> {
    Opener {
        content: content.boxed(),
        on_press: None,
        on_right_press: None,
    }
}

impl<'a, M> Opener<'a, M> {
    /// A click on it sends this, with its bounds in the window.
    pub fn on_press(mut self, message: impl Fn(Rectangle) -> M + 'a) -> Self {
        self.on_press = Some(Box::new(message));
        self
    }

    /// A right click on it sends this, with the pointer in the window.
    pub fn on_right_press(mut self, message: impl Fn(Point) -> M + 'a) -> Self {
        self.on_right_press = Some(Box::new(message));
        self
    }
}

#[derive(Default)]
struct State {
    /// How far the window is from this widget's layout: the overlay's
    /// translation, seen on each pass before events.
    translation: Vector,
    pressed: bool,
}

impl<M> widget::Meta for Opener<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Opener<'_, M> {
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
        let over = cursor.is_over(bounds);
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        let state = tree.state.downcast_mut::<State>();
        let window = |point: Point| point + state.translation;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = self.on_press.is_some();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = std::mem::take(&mut state.pressed);
                if let Some(on_press) = &self.on_press
                    && pressed
                    && over
                {
                    let shown = Rectangle::new(window(bounds.position()), bounds.size());
                    shell.publish(on_press(shown));
                    shell.capture_event();
                }
            }
            // A right press something inside took (a picture's own menu
            // in a message's) isn't this one's.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if over && !shell.is_event_captured() =>
            {
                if let (Some(on_right_press), Some(at)) = (&self.on_right_press, cursor.position())
                {
                    shell.publish(on_right_press(window(at)));
                    shell.capture_event();
                }
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
        let over = cursor.is_over(layout.bounds());
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        let inner = self
            .content
            .mouse_interaction(tree, layout, cursor, viewport, renderer);
        if inner == mouse::Interaction::None && over && self.on_press.is_some() {
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
