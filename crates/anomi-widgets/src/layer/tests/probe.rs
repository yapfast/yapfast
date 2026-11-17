//! A widget that tells whether what it wraps has the focus, seen past the
//! stack, which hides the focus of what lies under its top layer from the
//! operations a test runs; and the pointer it's given when the window is
//! drawn, which iced's widgets take their hover from.

use std::cell::Cell;
use std::rc::Rc;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Focusable, Scrollable, Text, TextInput};
use iced::advanced::widget::{self, Id, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::{Element, Renderer, Theme, View};

pub(super) struct Probe<'a, M> {
    content: Element<'a, M>,
    focused: Rc<Cell<bool>>,
    pointer: Rc<Cell<Option<mouse::Cursor>>>,
}

/// `content`, noting in `focused` whether it has the focus each time an
/// operation runs over it.
pub(super) fn probe<'a, M>(content: impl View<M> + 'a, focused: Rc<Cell<bool>>) -> Probe<'a, M> {
    Probe {
        content: content.boxed(),
        focused,
        pointer: Rc::default(),
    }
}

impl<M> Probe<'_, M> {
    /// Notes in `pointer` the pointer it's given on `RedrawRequested`.
    pub(super) fn pointer(mut self, pointer: Rc<Cell<Option<mouse::Cursor>>>) -> Self {
        self.pointer = pointer;
        self
    }
}

impl<M> widget::Meta for Probe<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Probe<'_, M> {
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
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            self.pointer.set(Some(cursor));
        }
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let mut peek = Peek {
            operation,
            focused: None,
        };
        self.content
            .operate(tree, layout, viewport, renderer, &mut peek);
        if let Some(focused) = peek.focused {
            self.focused.set(focused);
        }
    }
}

/// Passes an operation on, noting whether a focusable widget it meets has
/// the focus.
struct Peek<'a> {
    operation: &'a mut dyn Operation,
    focused: Option<bool>,
}

impl Operation for Peek<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let focused = &mut self.focused;
        self.operation.traverse(&mut |operation| {
            let mut peek = Peek {
                operation,
                focused: *focused,
            };
            operate(&mut peek);
            *focused = peek.focused;
        });
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, viewport: &Rectangle) {
        self.operation.container(id, bounds, viewport);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Size,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        self.operation
            .scrollable(id, bounds, content, translation, state);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        self.operation.focusable(id, bounds, state);
        self.focused = Some(state.is_focused());
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
        self.operation.text_input(id, bounds, state);
    }

    fn text(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Text) {
        self.operation.text(id, bounds, state);
    }

    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn std::any::Any) {
        self.operation.custom(id, bounds, state);
    }
}
