//! A part made of other widgets that the pointer and the keys act on as one
//! button: it fades the text colour its content inherits between rest and
//! hover (`animate color { duration: Theme.quick; }`), rings itself in the
//! accent under the keys, and presses on a click, Space or Enter. The build
//! stamp under the sections is one (`BuildStamp`,
//! `ui/yapfast/build-info.slint:108`). The content's text takes its colour
//! from the ground it is drawn on (`catalog::text::inherit`).

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Style};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;

use super::press::{Hot, RING, ring};

/// A pressable `content` called `label`; `ink(theme, lit)` is the text colour it inherits.
pub fn pressable<'a, M: Clone + 'a>(
    content: Element<'a, M>,
    label: String,
    on_press: M,
    radius: f32,
    ink: impl Fn(&Theme, f32) -> Color + 'a,
    motion: Motion,
) -> Element<'a, M> {
    Pressable {
        content,
        label,
        on_press,
        radius,
        ink: Box::new(ink),
        motion,
    }
    .boxed()
}

struct Pressable<'a, M> {
    content: Element<'a, M>,
    /// What the part is called, for a screen reader's sake and a test's.
    label: String,
    on_press: M,
    radius: f32,
    ink: Box<dyn Fn(&Theme, f32) -> Color + 'a>,
    motion: Motion,
}

impl<M> widget::Meta for Pressable<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Pressable<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hot>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Hot::new())
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &mut self.label.clone());
        operation.focusable(None, layout.bounds(), tree.state.downcast_mut::<Hot>());
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
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
        let (child_layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content.update(
            child,
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        let bounds = layout.bounds();
        let hot = tree.state.downcast_mut::<Hot>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = cursor.is_over(bounds);
                if hovered != hot.hovered {
                    hot.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                if !cursor.is_over(bounds) {
                    if hot.focused {
                        hot.focused = false;
                        shell.request_redraw();
                    }
                } else if *button == mouse::Button::Left && !shell.is_event_captured() {
                    hot.pressed = true;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if hot.pressed => {
                hot.pressed = false;
                if cursor.is_over(bounds) {
                    shell.publish(self.on_press.clone());
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if hot.focused && !shell.is_event_captured() =>
            {
                if matches!(
                    key.as_ref(),
                    keyboard::Key::Named(Named::Space | Named::Enter)
                ) {
                    shell.publish(self.on_press.clone());
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if hot.advance(*now, hot.hovered, self.motion) {
                    shell.request_redraw();
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
        _style: &Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let hot = tree.state.downcast_ref::<Hot>();
        let ink = renderer::Style {
            text_color: (self.ink)(theme, hot.amount()),
        };
        let (child_layout, child) = layout.iter(&tree.children).next().expect("child");
        self.content
            .draw(child, renderer, theme, &ink, child_layout, cursor, viewport);
        if hot.focused {
            ring(
                renderer,
                layout.bounds(),
                self.radius,
                RING,
                theme.palette().accent,
            );
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}
