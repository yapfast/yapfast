//! The search field's width. `HeaderSearch`'s `natural` (`ui/header.slint:90`)
//! glides between 32, 240 and 300 px over `Theme.slide`; a layout can't be
//! animated from a view, so this wraps the field, lays it out at the width
//! the glide has reached and asks for frames only while it goes. It also
//! takes Escape for the field while the field has the keys, and says whether
//! it had text (the key handler of `HeaderSearch`, `escape(cleared)`).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::animation::Animation;
use iced::keyboard::{self, key};
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::{GLIDE, SLIDE};
use anomi_theme::{Element, Motion, Renderer, Theme};

/// The least the field gives way to (`min-width: min(natural, 120px)`,
/// `ui/header.slint:88`).
const LEAST: f32 = 120.0;

pub struct Grow<'a, Message> {
    content: Element<'a, Message>,
    /// The width it is heading for.
    width: f32,
    /// What Escape says, for a field with text and for an empty one.
    on_escape: Option<(Message, Message)>,
    has_text: bool,
    motion: Motion,
}

impl<'a, Message> Grow<'a, Message> {
    pub fn new(content: Element<'a, Message>, width: f32, motion: Motion) -> Self {
        Self {
            content,
            width,
            on_escape: None,
            has_text: false,
            motion,
        }
    }

    /// Escape in a field that has the keys says `cleared` when it has text
    /// and `empty` when it has none.
    pub fn on_escape(mut self, cleared: Message, empty: Message, has_text: bool) -> Self {
        self.on_escape = Some((cleared, empty));
        self.has_text = has_text;
        self
    }
}

struct State {
    glide: Animation<f32>,
    /// The width the glide is heading for.
    target: f32,
    /// The width it laid out at.
    current: f32,
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

impl<Message> widget::Meta for Grow<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Grow<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            glide: Animation::new(self.width),
            target: self.width,
            current: self.width,
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Shrink)
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let current = tree.state.downcast_ref::<State>().current;
        let child = &mut tree.children[0];
        let bounds = limits.bounds();
        // `max-width: natural; min-width: min(natural, 120px)`: it gives way
        // in a crowded row, down to the least.
        let width = current.min(bounds.width).max(current.min(LEAST));
        let fixed = layout::Limits::new(Size::new(width, 0.0), Size::new(width, bounds.height));
        self.content.layout(child, renderer, &fixed);
        tree.size = child.size;
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let (Some((cleared, empty)), Event::Keyboard(keyboard::Event::KeyPressed { key, .. })) =
            (&self.on_escape, event)
            && matches!(key.as_ref(), keyboard::Key::Named(key::Named::Escape))
        {
            let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
            let mut probe = Probe(None);
            self.content
                .operate(child, layout, viewport, renderer, &mut probe);
            if probe.0 == Some(true) {
                shell.publish(if self.has_text { cleared } else { empty }.clone());
                shell.capture_event();
                return;
            }
        }

        {
            let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
            self.content
                .update(child, event, layout, cursor, renderer, shell, viewport);
        }

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            self.glide(tree, *now, shell);
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
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}

impl<Message> Grow<'_, Message> {
    /// Moves the glide to the frame at `now`, starting it where the target
    /// changed; lays out again when the width moved, and asks for the next
    /// frame only while it goes.
    fn glide(&self, tree: &mut Tree, now: Instant, shell: &mut Shell<'_, Message>) {
        let state = tree.state.downcast_mut::<State>();
        if state.target != self.width {
            state.glide = Animation::new(state.current)
                .duration(self.motion.duration(SLIDE))
                .easing(GLIDE)
                .go(self.width, now);
            state.target = self.width;
        }
        let width = if state.glide.is_animating(now) {
            shell.request_redraw();
            state.glide.interpolate(now)
        } else {
            state.target
        };
        if width != state.current {
            state.current = width;
            shell.invalidate_layout();
        }
    }
}
