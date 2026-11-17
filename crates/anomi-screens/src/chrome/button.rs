//! The bar's two kinds of button, as one small widget. The Slint side:
//! `CaptionButton` (`ui/yapfast/window-bar.slint:4`: 36 px wide, a 12 px
//! glyph on a square ground that fills with the hover colour, or with the
//! danger colour for close) and `BarButton` (`:31`: 28 by 24 px, a 16 px
//! glyph on a 4 px-rounded ground that takes 8 % of the overlay under
//! the pointer or the focus and 12 % while on, the "active" look of the
//! inbox); the old `WindowButton` (`ui/title_bar.slint:4`) is the classic
//! style's and isn't ported.
//!
//! Both ease their ground and ink over `QUICK` while the pointer or the
//! focus comes and goes (`animate background { duration: Theme.quick; }`),
//! linearly as Slint does; a still [`Motion`] lands at once and asks for no
//! frame. Tab reaches them (`forward-focus`), and Space or Enter presses
//! the one focused; a click doesn't focus. A focused button shows the hover
//! ground, as in Slint.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::keyboard::{self, key::Named};
use iced::{Border, Color, Event, Length, Rectangle, Size, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Motion, Palette, Renderer, Theme};
use anomi_widgets::icon::Icon;

/// A caption button's size and glyph (`window-bar.slint:9`, `:17`).
const CAPTION: (Size, f32) = (Size::new(36.0, 32.0), 12.0);
/// A bar button's size, glyph and corner (`:37`, `:46`).
const BAR: (Size, f32) = (Size::new(28.0, 24.0), 16.0);
const BAR_RADIUS: f32 = 4.0;
/// The overlay's alpha under the pointer and while on (`:38`).
pub(super) const HOVER_ALPHA: f32 = 0.08;
pub(super) const ACTIVE_ALPHA: f32 = 0.12;

/// Which look a button has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Minimize and maximize.
    Caption,
    /// Close: danger ground, `on_danger` ink.
    Close,
    /// Back, forward, inbox and help, with whether it's switched on.
    Bar { active: bool },
}

pub struct Button<M> {
    icon: Icon,
    kind: Kind,
    enabled: bool,
    on_press: Option<M>,
    motion: Motion,
}

pub fn button<M>(icon: Icon, kind: Kind, motion: Motion) -> Button<M> {
    Button {
        icon,
        kind,
        enabled: true,
        on_press: None,
        motion,
    }
}

impl<M> Button<M> {
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn active(&self) -> bool {
        matches!(self.kind, Kind::Bar { active: true })
    }

    fn live(&self) -> bool {
        self.enabled && self.on_press.is_some()
    }

    fn metrics(&self) -> (Size, f32, f32) {
        match self.kind {
            Kind::Caption | Kind::Close => (CAPTION.0, CAPTION.1, 0.0),
            Kind::Bar { .. } => (BAR.0, BAR.1, BAR_RADIUS),
        }
    }

    /// The ground and the ink for how lit the button is (hover or focus), how
    /// hovered (the pointer alone), how switched on and how enabled, each from
    /// 0 to 1. The looks are blended by those amounts, as Slint eases every
    /// change of the ground and the ink (`window-bar.slint:38`, `:47`).
    pub(super) fn colours(
        &self,
        palette: &Palette,
        lit: f32,
        over: f32,
        active: f32,
        enabled: f32,
    ) -> (Color, Color) {
        match self.kind {
            Kind::Caption => (palette.hover.scale_alpha(lit), palette.secondary),
            Kind::Close => (
                palette.danger.scale_alpha(lit),
                mix(palette.secondary, palette.on_danger, over),
            ),
            Kind::Bar { .. } => {
                let alpha = HOVER_ALPHA * lit + (ACTIVE_ALPHA - HOVER_ALPHA * lit) * active;
                let ink = mix(
                    palette.secondary,
                    palette.text,
                    over + (1.0 - over) * active,
                );
                (
                    Color {
                        a: alpha * enabled,
                        ..palette.overlay
                    },
                    mix(palette.faint, ink, enabled),
                )
            }
        }
    }
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    if amount <= 0.0 {
        return from;
    }
    if amount >= 1.0 {
        return to;
    }
    Color {
        r: from.r + (to.r - from.r) * amount,
        g: from.g + (to.g - from.g) * amount,
        b: from.b + (to.b - from.b) * amount,
        a: from.a + (to.a - from.a) * amount,
    }
}

struct State {
    hovered: bool,
    pressed: bool,
    focused: bool,
    now: Instant,
    lit: Animation<bool>,
    over: Animation<bool>,
    active: Animation<bool>,
    enabled: Animation<bool>,
}

impl State {
    fn new(active: bool, enabled: bool) -> Self {
        Self {
            hovered: false,
            pressed: false,
            focused: false,
            now: Instant::now(),
            lit: landed(false),
            over: landed(false),
            active: landed(active),
            enabled: landed(enabled),
        }
    }

    /// Moves each toward the pointer, the focus and the button's own props at
    /// `now`; true while one still plays.
    fn advance(&mut self, now: Instant, motion: Motion, active: bool, enabled: bool) -> bool {
        self.now = now;
        step(&mut self.lit, self.hovered || self.focused, motion, now);
        step(&mut self.over, self.hovered, motion, now);
        step(&mut self.active, active, motion, now);
        step(&mut self.enabled, enabled, motion, now);
        [&self.lit, &self.over, &self.active, &self.enabled]
            .into_iter()
            .any(|animation| animation.is_animating(now))
    }

    /// Lit, hovered, on and enabled, each from 0 to 1.
    fn amounts(&self) -> [f32; 4] {
        let at = |animation: &Animation<bool>| animation.interpolate(0.0f32, 1.0, self.now);
        [
            at(&self.lit),
            at(&self.over),
            at(&self.active),
            at(&self.enabled),
        ]
    }
}

/// An animation at `value`, playing nothing, that goes over [`QUICK`].
fn landed(value: bool) -> Animation<bool> {
    Animation::new(value).duration(QUICK).easing(Easing::Linear)
}

/// Moves `animation` toward `target` at `now`, landing at once while
/// still; touches it only when something changes, as this runs for each
/// button on every frame of any animation in the window.
fn step(animation: &mut Animation<bool>, target: bool, motion: Motion, now: Instant) {
    if motion.still() {
        if animation.value() != target || animation.is_animating(now) {
            *animation = landed(target);
        }
    } else if animation.value() != target {
        animation.go_mut(target, now);
    }
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

impl<M> widget::Meta for Button<M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Button<M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.active(), self.enabled))
    }

    fn size(&self) -> Size<Length> {
        let size = self.metrics().0;
        Size::new(Length::Fixed(size.width), Length::Fixed(size.height))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let size = self.metrics().0;
        tree.size = limits.resolve(Length::Fixed(size.width), Length::Fixed(size.height), size);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if self.live() {
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(None, layout.bounds(), state);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let live = self.live();
        let state = tree.state.downcast_mut::<State>();
        if !live {
            // A disabled button can't hold the focus, the hover or a press.
            state.focused = false;
            state.pressed = false;
            state.hovered = false;
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = live && cursor.is_over(bounds);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                if !cursor.is_over(bounds) {
                    if state.focused {
                        state.focused = false;
                        shell.request_redraw();
                    }
                } else if *button == mouse::Button::Left {
                    // Taken, so the bar under it doesn't start a move.
                    if live {
                        state.pressed = true;
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                if cursor.is_over(bounds)
                    && let Some(message) = &self.on_press
                {
                    shell.publish(message.clone());
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if live && state.focused && !shell.is_event_captured() =>
            {
                if matches!(
                    key.as_ref(),
                    keyboard::Key::Named(Named::Space | Named::Enter)
                ) {
                    if let Some(message) = &self.on_press {
                        shell.publish(message.clone());
                    }
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now))
                if state.advance(*now, self.motion, self.active(), self.enabled) =>
            {
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let [lit, over, active, enabled] = state.amounts();
        let (ground, ink) = self.colours(theme.palette(), lit, over, active, enabled);
        let bounds = layout.bounds();
        let (_, glyph, radius) = self.metrics();
        renderer::Renderer::fill_quad(
            renderer,
            Quad {
                bounds,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                ..Quad::default()
            },
            ground,
        );
        let mut svg = vector::Svg::new(self.icon.handle());
        // tiny-skia keeps only the rgb of an svg colour, so alpha goes by opacity.
        svg.color = Some(Color { a: 1.0, ..ink });
        svg.opacity = ink.a;
        let area = Rectangle::new(
            iced::Point::new(
                bounds.center_x() - glyph / 2.0,
                bounds.center_y() - glyph / 2.0,
            ),
            Size::new(glyph, glyph),
        );
        renderer.draw_svg(svg, area, *viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let pointer = matches!(self.kind, Kind::Bar { .. });
        if pointer && self.live() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
