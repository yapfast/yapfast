//! The box around the composer: `ComposerFrame` (`ui/composer.slint:68`),
//! Discord's faint outline, a notch lighter while the field has the keys
//! (the colour fades over `Theme.quick`), its top corners square under a
//! reply's bar. Everything inside is drawn over its ground.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::Operation;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::animation::{Animation, Easing};
use iced::border::Radius;
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme};

/// `border-radius: 10px`.
const RADIUS: f32 = 10.0;
/// `Theme.muted.with-alpha(0.55)`: the outline while the field has the keys.
const LIT: f32 = 0.55;

/// The box around `content`; the thread's and the post's composers draw with it
/// too.
pub struct Frame<'a, Message> {
    content: Element<'a, Message>,
    focused: bool,
    under_bar: bool,
    motion: Motion,
}

/// The frame around `content`; `focused` while the field has the keys,
/// `under_bar` while a bar sits on its top.
pub fn frame<'a, Message>(
    content: Element<'a, Message>,
    focused: bool,
    under_bar: bool,
    motion: Motion,
) -> Frame<'a, Message> {
    Frame {
        content,
        focused,
        under_bar,
        motion,
    }
}

/// The outline's fade: `true` is the lit look.
struct State {
    lit: bool,
    fade: Animation<bool>,
    now: Instant,
}

impl State {
    fn new() -> Self {
        Self {
            lit: false,
            fade: Animation::new(false),
            now: Instant::now(),
        }
    }

    /// Moves the fade to the frame at `now`, starting it where the target
    /// changed; whether another frame is wanted. Still, it lands at once.
    fn advance(&mut self, now: Instant, focused: bool, motion: Motion) -> bool {
        self.now = now;
        if motion.still() {
            self.fade = Animation::new(focused);
            self.lit = focused;
            return false;
        }
        if self.lit != focused {
            self.lit = focused;
            let from = self.fade.interpolate(0.0f32, 1.0, now) >= 0.5;
            self.fade = Animation::new(from)
                .duration(motion.duration(QUICK))
                .easing(Easing::Linear)
                .go(focused, now);
        }
        self.fade.is_animating(now)
    }

    fn amount(&self) -> f32 {
        self.fade.interpolate(0.0f32, 1.0, self.now)
    }
}

fn blend(from: Color, to: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

impl<Message> widget::Meta for Frame<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Frame<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new())
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
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
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
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
        let state = tree.state.downcast_mut::<State>();
        if state.lit != self.focused {
            shell.request_redraw();
        }
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child, event, layout, cursor, renderer, shell, viewport);
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let state = tree.state.downcast_mut::<State>();
            if state.advance(*now, self.focused, self.motion) {
                shell.request_redraw();
            }
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let outline = blend(
            palette.outline_field,
            palette.muted.scale_alpha(LIT),
            state.amount(),
        );
        let top = if self.under_bar { 0.0 } else { RADIUS };
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border {
                    color: outline,
                    width: 1.0,
                    radius: Radius {
                        top_left: top,
                        top_right: top,
                        bottom_right: RADIUS,
                        bottom_left: RADIUS,
                    },
                },
                ..Quad::default()
            },
            palette.composer(),
        );
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
