//! The place's ink in the bar's middle: faint while the window is behind,
//! secondary while it has the focus, faded between them over [`QUICK`]
//! (`ui/yapfast/window-bar.slint:114`). A text style can't animate, so the
//! colour is a tree state drawn through `renderer::Style::text_color`, which
//! the label inherits. It lands at once while the motion is still, and asks
//! for frames only while the fade plays.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Color, Event, Length, Rectangle, Size, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme};

/// The room Slint's group keeps after a label that fits (`window-bar.slint:108`:
/// label + 26 = 16 + 8 + label + 2); a cut label has none.
const TAIL: f32 = 2.0;

struct State {
    lit: Animation<bool>,
    now: Instant,
}

pub struct Ink<'a, Message> {
    content: Element<'a, Message>,
    focused: bool,
    motion: Motion,
}

pub fn ink<'a, Message>(
    content: Element<'a, Message>,
    focused: bool,
    motion: Motion,
) -> Ink<'a, Message> {
    Ink {
        content,
        focused,
        motion,
    }
}

impl<Message> widget::Meta for Ink<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Ink<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            lit: Animation::new(self.focused),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        // Slint sizes the group from the label's unelided width, so decide
        // the cut from the natural width, not from the elided paragraph's.
        let max = limits.bounds();
        let natural = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, max.height));
        self.content
            .layout(&mut tree.children[0], renderer, &natural);
        let width = tree.children[0].size.width + TAIL;
        let width = if width <= max.width {
            width
        } else {
            self.content.layout(&mut tree.children[0], renderer, limits);
            max.width
        };
        tree.children[0].translation = iced::Vector::ZERO;
        let mut size = tree.children[0].size;
        size.width = width;
        tree.size = size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.traverse(&mut |operation| {
            let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
            self.content
                .operate(tree, layout, viewport, renderer, operation);
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child, event, layout, cursor, renderer, shell, viewport);
        let state = tree.state.downcast_mut::<State>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            if state.lit.value() != self.focused {
                state.lit = Animation::new(state.lit.value())
                    .duration(self.motion.duration(QUICK))
                    .easing(Easing::Linear)
                    .go(self.focused, *now);
            }
            if state.lit.is_animating(*now) {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let lit = if self.motion.still() {
            f32::from(self.focused)
        } else {
            state.lit.interpolate(0.0f32, 1.0, state.now)
        };
        let palette = theme.palette();
        let mix = |a: f32, b: f32| a + (b - a) * lit;
        let style = renderer::Style {
            text_color: Color {
                r: mix(palette.faint.r, palette.secondary.r),
                g: mix(palette.faint.g, palette.secondary.g),
                b: mix(palette.faint.b, palette.secondary.b),
                a: mix(palette.faint.a, palette.secondary.a),
            },
        };
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child, renderer, theme, &style, layout, cursor, viewport);
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
}
