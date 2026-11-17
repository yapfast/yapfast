//! The popover's size from the room its layer gives it (`ui/yapfast.slint:
//! 4941`): `min(max(480, 35 % of W), 600, W - 16)` wide and `min(max(400,
//! fit), 80 % of H)` tall, W and H being the window. The layer lays the card
//! out within the window less 8 px on each side, so the window is the room
//! plus 16. It also takes Escape for the search field while the field has the
//! keys and text: the text clears and the popover stays; an empty field lets
//! Escape through to close it (`HeaderSearch`, `ui/header.slint:117`).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, key};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

/// What the layer keeps around the card.
const MARGIN: f32 = 8.0;
const WIDE_LEAST: f32 = 480.0;
const WIDE_SHARE: f32 = 0.35;
const WIDE_MOST: f32 = 600.0;
const TALL_LEAST: f32 = 400.0;
const TALL_SHARE: f32 = 0.8;

pub struct Fit<'a, M> {
    content: Element<'a, M>,
    /// The height its rows ask for.
    fit: f32,
    /// What Escape says while the field has text.
    clear: Option<M>,
}

/// `content` at the size the room allows, `fit` tall if it may.
pub fn fit<'a, M>(content: Element<'a, M>, fit: f32, clear: Option<M>) -> Fit<'a, M> {
    Fit {
        content,
        fit,
        clear,
    }
}

/// The size for the room `bounds`.
fn size_in(bounds: Size, fit: f32) -> Size {
    let window = |room: f32| {
        if room.is_finite() {
            room + 2.0 * MARGIN
        } else {
            f32::INFINITY
        }
    };
    let (w, h) = (window(bounds.width), window(bounds.height));
    let width = (w * WIDE_SHARE)
        .clamp(WIDE_LEAST, WIDE_MOST)
        .min(bounds.width);
    let height = fit.max(TALL_LEAST).min(h * TALL_SHARE).min(bounds.height);
    Size::new(width, height)
}

/// Whether the first focusable under it has the keys.
struct Probe(Option<bool>);

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.0.is_none() {
            self.0 = Some(state.is_focused());
        }
    }
}

impl<M> widget::Meta for Fit<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Fit<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let size = size_in(limits.bounds(), self.fit);
        let exact = layout::Limits::new(size, size);
        self.content.layout(&mut tree.children[0], renderer, &exact);
        tree.size = size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(child, layout, viewport, renderer, operation);
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        if let (Some(clear), Event::Keyboard(keyboard::Event::KeyPressed { key, .. })) =
            (&self.clear, event)
            && matches!(key.as_ref(), keyboard::Key::Named(key::Named::Escape))
        {
            let mut probe = Probe(None);
            self.content
                .operate(child, layout, viewport, renderer, &mut probe);
            if probe.0 == Some(true) {
                shell.publish(clear.clone());
                shell.capture_event();
                return;
            }
        }
        self.content
            .update(child, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(child, layout, cursor, viewport, renderer)
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
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child, renderer, theme, style, layout, cursor, viewport);
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}
