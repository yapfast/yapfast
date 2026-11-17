//! A widget that sends where it sits when clicked, or when Enter or Space
//! reach it focused: a tag opens its card beside itself, which a button's
//! plain message can't say. Kept to what [`super::ServerTag`] needs.

use anomi_theme::Element;
use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Renderer, Theme};

/// The focus ring's width (`border-width: 2px`).
const RING: f32 = 2.0;

#[derive(Default)]
struct State {
    /// How far the window is from this widget's layout (the overlay's
    /// translation), so a scrolled tag reports where it shows.
    translation: Vector,
    pressed: bool,
    focused: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

pub struct Opens<'a, Message> {
    content: Element<'a, Message>,
    on_press: Box<dyn Fn(Rectangle) -> Message + 'a>,
    keyboard: bool,
    radius: f32,
}

impl<'a, Message> Opens<'a, Message> {
    pub fn new(
        content: Element<'a, Message>,
        radius: f32,
        keyboard: bool,
        on_press: impl Fn(Rectangle) -> Message + 'a,
    ) -> Self {
        Self {
            content,
            on_press: Box::new(on_press),
            keyboard,
            radius,
        }
    }
}

impl<Message> widget::Meta for Opens<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Opens<'_, Message> {
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
        // The content sits at the origin, the widget as large as it.
        tree.children[0].translation = iced::Vector::ZERO;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if self.keyboard {
            operation.focusable(None, layout.bounds(), tree.state.downcast_mut::<State>());
        }
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            let content = &mut self.content;
            let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
            content.operate(tree, layout, viewport, renderer, operation);
        });
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
        // The badge's picture plays on redraws and rescales.
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let shown = Rectangle::new(bounds.position() + state.translation, bounds.size());
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.pressed = true;
                    shell.capture_event();
                } else if state.focused {
                    state.focused = false;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                if cursor.is_over(bounds) {
                    shell.publish((self.on_press)(shown));
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if state.focused && !shell.is_event_captured() =>
            {
                if matches!(
                    key.as_ref(),
                    keyboard::Key::Named(Named::Space | Named::Enter)
                ) {
                    shell.publish((self.on_press)(shown));
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
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child_tree, renderer, theme, style, child, cursor, viewport);
        if tree.state.downcast_ref::<State>().focused {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: theme.palette().accent,
                        width: RING,
                        radius: self.radius.into(),
                    },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
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
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        tree.state.downcast_mut::<State>().translation = translation;
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
