//! The pill a voice message's player sits in (`pill` in `FileCard`,
//! `ui/messages.slint`): the sidebar's colour, 52 px tall with round ends,
//! 8 px in at the start and 16 at the end. Its speed and mute slide out
//! after the time over `SLIDE` while the pointer is on it or the keyboard
//! is on one of its buttons (`pill.expanded`); with animations off they
//! always show, and while the window is behind they land at once.
//!
//! Folded, they take no pointer but stay in the focus chain, as Slint's
//! clipped `FocusScope`s do, so Tab reaches them and brings them out. The
//! layout's 12 px spacing stays after the time though they fold to nothing,
//! as Slint's does. Iced can't fade a group, so they are clipped to the
//! width shown instead of fading too.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Id, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Border, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::{GLIDE, SLIDE};
use anomi_theme::{Element, Motion, Renderer, Theme};

pub const HEIGHT: f32 = 52.0;
const PAD_LEFT: f32 = 8.0;
const PAD_RIGHT: f32 = 16.0;
const GAP: f32 = 12.0;

/// The player: `base` (the button, the waveform and the time) and the
/// `extras` that fold away.
pub fn pill<'a, M>(base: Element<'a, M>, extras: Element<'a, M>, motion: Motion) -> Pill<'a, M> {
    Pill {
        children: [base, extras],
        motion,
    }
}

pub struct Pill<'a, M> {
    children: [Element<'a, M>; 2],
    motion: Motion,
}

struct State {
    hovered: bool,
    focused: bool,
    reveal: Animation<bool>,
    /// It moved at the last frame: the next one lays it out where it landed.
    moving: bool,
    now: Instant,
}

impl State {
    /// How far the extras are out, 0 to 1.
    fn shown(&self) -> f32 {
        self.reveal.interpolate(0.0, 1.0, self.now)
    }
}

/// Whether anything inside has the keyboard.
struct Focused(bool);

impl Operation for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        self.0 |= state.is_focused();
    }
}

/// The part of the pill the extras show in, `shown` of the way out.
fn reach(pill: Rectangle, extras: Rectangle, shown: f32) -> Rectangle {
    Rectangle {
        x: extras.x,
        y: pill.y,
        width: extras.width * shown,
        height: pill.height,
    }
}

impl<M> widget::Meta for Pill<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Pill<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            focused: false,
            reveal: Animation::new(self.motion.never),
            moving: false,
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let loose = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, HEIGHT));
        for (child, child_tree) in self.children.iter_mut().zip(&mut tree.children) {
            child.layout(child_tree, renderer, &loose);
        }
        let shown = tree.state.downcast_ref::<State>().shown();
        let (base, extras) = (tree.children[0].size, tree.children[1].size);
        let after = PAD_LEFT + base.width + GAP;
        tree.children[0].translation = Vector::new(PAD_LEFT, (HEIGHT - base.height) / 2.0);
        tree.children[1].translation = Vector::new(after, (HEIGHT - extras.height) / 2.0);
        let width = after + extras.width * shown + PAD_RIGHT;
        tree.size = limits.resolve(
            Length::Shrink,
            Length::Fixed(HEIGHT),
            Size::new(width, HEIGHT),
        );
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
        let shown = tree.state.downcast_ref::<State>().shown();
        let check = matches!(
            event,
            Event::Keyboard(_) | Event::Mouse(mouse::Event::ButtonPressed(_))
        );
        let mut focused = Focused(false);
        for (index, (child, (child_layout, child_tree))) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
            .enumerate()
        {
            // The folded part takes no pointer; keys reach it, as it may
            // hold the focus.
            let shows = shown > 0.0 && cursor.is_over(reach(bounds, child_layout.bounds(), shown));
            let cursor = if index == 0 || shows {
                cursor
            } else {
                mouse::Cursor::Unavailable
            };
            child.update(
                child_tree,
                event,
                child_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
            if check {
                child.operate(child_tree, child_layout, viewport, renderer, &mut focused);
            }
        }

        let state = tree.state.downcast_mut::<State>();
        if let Event::Mouse(_) = event {
            state.hovered = cursor.is_over(bounds);
        }
        if check {
            state.focused = focused.0;
        }
        let wanted = self.motion.never || state.hovered || state.focused;
        if wanted != state.reveal.value() {
            let now = Instant::now();
            state.now = now;
            let reveal = std::mem::replace(&mut state.reveal, Animation::new(wanted));
            if self.motion.playing() {
                state.reveal = reveal.duration(SLIDE).easing(GLIDE).go(wanted, now);
            }
            shell.invalidate_layout();
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            if self.motion.still() {
                state.reveal = Animation::new(wanted);
            }
            let moving = state.reveal.is_animating(*now);
            if moving || state.moving {
                shell.invalidate_layout();
            }
            if moving {
                shell.request_redraw();
            }
            state.moving = moving;
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
        let bounds = layout.bounds();
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default().rounded(HEIGHT / 2.0),
                ..Quad::default()
            },
            theme.palette().sidebar,
        );
        let shown = tree.state.downcast_ref::<State>().shown();
        let mut children = layout.iter(&tree.children);
        let (base, base_tree) = children.next().unwrap();
        self.children[0].draw(base_tree, renderer, theme, style, base, cursor, viewport);
        let (extras, extras_tree) = children.next().unwrap();
        if shown <= 0.0 {
            return;
        }
        let reach = reach(bounds, extras.bounds(), shown);
        let cursor = if cursor.is_over(reach) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        let draw = |renderer: &mut Renderer| {
            self.children[1].draw(
                extras_tree,
                renderer,
                theme,
                style,
                extras,
                cursor,
                viewport,
            );
        };
        if shown < 1.0 {
            renderer.with_layer(reach, draw);
        } else {
            draw(renderer);
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
        let bounds = layout.bounds();
        let shown = tree.state.downcast_ref::<State>().shown();
        let mut children = layout.iter(&tree.children);
        let (base, base_tree) = children.next().unwrap();
        let (extras, extras_tree) = children.next().unwrap();
        if shown > 0.0 && cursor.is_over(reach(bounds, extras.bounds(), shown)) {
            self.children[1].mouse_interaction(extras_tree, extras, cursor, viewport, renderer)
        } else {
            self.children[0].mouse_interaction(base_tree, base, cursor, viewport, renderer)
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
        for (child, (child_layout, child_tree)) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            child.operate(child_tree, child_layout, viewport, renderer, operation);
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
        let mut layers = Vec::new();
        for (child, (child_layout, child_tree)) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            layers.extend(child.overlay(
                child_tree,
                child_layout,
                renderer,
                viewport,
                translation,
                window,
            ));
        }
        layers
    }
}
