//! The way a popup opens and closes: over [`DURATION`], eased out, its
//! card grows from [`FROM`] of its size to all of it about its centre
//! ([`Grow`]) and the window's dim under it fades in ([`dim`]), and closing
//! does the same the other way. A layer that opens is wrapped in
//! [`Motion`], which says how far open it is to what's drawn inside it
//! while it draws, and asks for the window's redraws until it's open;
//! nothing sends a message or builds the view anew for it. One closing
//! ([`leave`]) is drawn and takes nothing, neither the pointer nor the
//! keys, and says once it's gone. Outside a `Motion` a card is drawn as it
//! is.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use iced::{
    Color, Element, Transformation,
    advanced::{Renderer as _, Shell, renderer, widget::Operation},
    mouse, window,
};

use super::{At, Hooks, Inner, Renderer, Stateless, Wrap};
use crate::theme::Motion as ThemeMotion;

/// State for a button or row whose installed theme lifts, grows, glows or
/// sinks it under the pointer. The animation repaints only; it does not
/// rebuild the view or move the widget's hit target.
#[derive(Default)]
struct InteractiveState {
    pointed: bool,
    pressed: bool,
    amount: f32,
    from: f32,
    target: f32,
    started: Option<Instant>,
}

struct Interactive {
    style: ThemeMotion,
    glow: Color,
    radius: f32,
    hover_cue: bool,
    press_cue: bool,
}

/// Applies the resolved hover and press motion to an ordinary interactive
/// surface. Rotation is deliberately not part of this widget.
pub fn interactive<'a, M: 'a>(
    surface: impl Into<Element<'a, M>>,
    style: ThemeMotion,
    glow: Color,
    radius: f32,
) -> Element<'a, M> {
    interactive_cued(surface, style, glow, radius, false, false)
}

/// Applies hover/press motion and optional interface sound cues.
pub fn interactive_cued<'a, M: 'a>(
    surface: impl Into<Element<'a, M>>,
    style: ThemeMotion,
    glow: Color,
    radius: f32,
    hover_cue: bool,
    press_cue: bool,
) -> Element<'a, M> {
    Wrap::new(
        surface,
        Interactive {
            style,
            glow,
            radius,
            hover_cue,
            press_cue,
        },
    )
    .into()
}

impl<M> Hooks<M> for Interactive {
    type State = InteractiveState;

    fn update(
        &mut self,
        state: &mut InteractiveState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        child.update(shell);
        if matches!(event, iced::Event::Mouse(_)) {
            let pointed = at.cursor.is_over(at.bounds);
            if pointed != state.pointed {
                if pointed && self.hover_cue {
                    crate::theme::emit_cue(crate::theme::CueEvent::Hover);
                }
                state.pointed = pointed;
                state.from = state.amount;
                state.target = f32::from(pointed);
                state.started = Some(Instant::now());
                shell.request_redraw();
            }
        }
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if state.pointed =>
            {
                if self.press_cue {
                    crate::theme::emit_cue(crate::theme::CueEvent::Press);
                }
                state.pressed = true;
                shell.request_redraw();
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.pressed =>
            {
                state.pressed = false;
                shell.request_redraw();
            }
            iced::Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(started) = state.started {
                    let duration = self.style.hover_duration(state.target > state.from);
                    let t = if duration.is_zero() {
                        1.0
                    } else {
                        (now.saturating_duration_since(started).as_secs_f32()
                            / duration.as_secs_f32())
                        .min(1.0)
                    };
                    state.amount = state.from + (state.target - state.from) * self.style.ease(t);
                    if t < 1.0 {
                        shell.request_redraw();
                    } else {
                        state.amount = state.target;
                        state.started = None;
                    }
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        state: &InteractiveState,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        let lift = self.style.lift(state.amount, state.pressed);
        let scale = self.style.grow(state.amount, state.pressed);
        let halo = self.style.halo(state.amount, self.glow);
        renderer.with_layer(at.bounds.expand(20.0), |renderer| {
            if halo != iced::Shadow::default() {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: at.child,
                        border: iced::Border::default().rounded(self.radius),
                        shadow: halo,
                        ..Default::default()
                    },
                    Color::TRANSPARENT,
                );
            }
            if lift == 0.0 && scale == 1.0 {
                child(renderer);
                return;
            }
            let centre = at.child.center();
            let transformation = Transformation::translate(0.0, lift)
                * Transformation::translate(centre.x, centre.y)
                * Transformation::scale(scale)
                * Transformation::translate(-centre.x, -centre.y);
            renderer.with_transformation(transformation, child);
        });
    }
}

/// How long a popup takes to open.
pub const DURATION: Duration = Duration::from_millis(100);
/// How large a card opens from.
pub const FROM: f32 = 0.8;

thread_local! {
    /// How far open the layer being drawn is, eased, from 0 to 1: set by
    /// the [`Motion`] drawing it, 1 outside one.
    static SHOWN: Cell<f32> = const { Cell::new(1.0) };
    /// The resolved popup style of the layer currently being drawn.
    static STYLE: Cell<ThemeMotion> = Cell::new(ThemeMotion::default());
}

/// How far open a layer is that opened at `start`, at `now`, eased out
/// on a quadratic curve.
pub fn shown(style: ThemeMotion, closing: bool, start: Instant, now: Instant) -> f32 {
    let duration = style.duration(closing);
    if duration.is_zero() {
        return 1.0;
    }
    let t = (now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32()).min(1.0);
    if t == 0.0 || t == 1.0 {
        return t;
    }
    style.ease(t)
}

/// For a layer closing since `start`, when it would have started opening
/// to be as far open at `now` as it is: a popup opened again as it closes
/// grows on from there.
pub fn reopened(style: ThemeMotion, start: Instant, now: Instant) -> Instant {
    let open = 1.0 - shown(style, true, start, now);
    let duration = style.duration(false);
    if duration.is_zero() {
        return now;
    }
    // Invert any named curve closely enough that reopening continues at the
    // same visible amount instead of jumping.
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..20 {
        let middle = (low + high) / 2.0;
        if style.ease(middle) < open {
            low = middle;
        } else {
            high = middle;
        }
    }
    now - duration.mul_f32((low + high) / 2.0)
}

/// A layer opening or closing: `start` is when it was first drawn so,
/// kept by the one who keeps what's open, as the widget tree's state
/// would go with a layer opened or closed under it.
pub struct Motion<'a, M> {
    start: &'a Cell<Option<Instant>>,
    /// While it closes, what it says once it's closed.
    closed: Option<M>,
    style: ThemeMotion,
}

impl<M> Motion<'_, M> {
    /// How far open it is at `now`, its start set if it had none.
    fn shown(&self, now: Instant) -> f32 {
        let start = *self.start.get().get_or_insert(now);
        self.start.set(Some(start));
        let opened = shown(self.style, self.closed.is_some(), start, now);
        if self.closed.is_some() {
            1.0 - opened
        } else {
            opened
        }
    }
}

/// `layer`, opening since it was first drawn (`start`, empty until then).
pub fn motion<'a, M: Clone + 'a>(
    layer: impl Into<Element<'a, M>>,
    start: &'a Cell<Option<Instant>>,
    style: ThemeMotion,
) -> Element<'a, M> {
    Wrap::new(
        layer,
        Motion {
            start,
            closed: None,
            style,
        },
    )
    .into()
}

/// `layer`, closed and closing since it was first drawn so (`start`):
/// drawn, it takes nothing, and says `closed` as it's gone.
pub fn leave<'a, M: Clone + 'a>(
    layer: impl Into<Element<'a, M>>,
    start: &'a Cell<Option<Instant>>,
    closed: M,
    style: ThemeMotion,
) -> Element<'a, M> {
    Wrap::new(
        layer,
        Motion {
            start,
            closed: Some(closed),
            style,
        },
    )
    .into()
}

impl<M: Clone> Hooks<M> for Motion<'_, M> {
    type State = Stateless;

    fn update(
        &mut self,
        _state: &mut Stateless,
        event: &iced::Event,
        _at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            let start = *self.start.get().get_or_insert(*now);
            self.start.set(Some(start));
            if now.saturating_duration_since(start) < self.style.duration(self.closed.is_some()) {
                shell.request_redraw();
            } else if let Some(closed) = &self.closed {
                shell.publish(closed.clone());
            }
        }
        if self.closed.is_none() {
            child.update(shell);
        }
    }

    fn interaction(
        &self,
        _state: &Stateless,
        _at: At,
        child: mouse::Interaction,
    ) -> mouse::Interaction {
        // Closing, the pointer goes to what's under it.
        if self.closed.is_some() {
            mouse::Interaction::None
        } else {
            child
        }
    }

    fn draw(
        &self,
        _state: &Stateless,
        renderer: &mut Renderer,
        _at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        let shown = self.shown(Instant::now());
        if shown <= 0.0 {
            return;
        }
        let outer = SHOWN.replace(shown);
        let outer_style = STYLE.replace(self.style);
        child(renderer);
        STYLE.set(outer_style);
        SHOWN.set(outer);
    }

    fn overlays(&self) -> bool {
        self.closed.is_none()
    }

    fn operate(
        &mut self,
        _state: &mut Stateless,
        operation: &mut dyn Operation,
        child: &mut dyn FnMut(&mut dyn Operation),
    ) {
        // Closing, no focus or scroll finds what's in it.
        if self.closed.is_none() {
            child(operation);
        }
    }
}

/// A popup's card, grown from [`FROM`] of its size about its centre as
/// the [`Motion`] it's drawn in opens.
pub struct Grow;

/// `card`, grown as its layer opens.
pub fn grow<'a, M: 'a>(card: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Wrap::new(card, Grow).into()
}

impl<M> Hooks<M> for Grow {
    type State = Stateless;

    fn draw(
        &self,
        _state: &Stateless,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        let shown = SHOWN.get();
        if shown >= 1.0 {
            child(renderer);
            return;
        }
        let scale = STYLE.get().scale(shown);
        let centre = at.child.center();
        let transformation = Transformation::translate(centre.x, centre.y)
            * Transformation::scale(scale)
            * Transformation::translate(-centre.x, -centre.y);
        renderer.with_transformation(transformation, |renderer| child(renderer));
    }
}

/// The window dimmed by `color` under a popup, fading in as its layer
/// opens: `layer` (taking the clicks, holding the id) is drawn over it.
pub fn dim<'a, M: 'a>(layer: impl Into<Element<'a, M>>, color: Color) -> Element<'a, M> {
    Wrap::new(layer, Dim { color }).into()
}

/// What [`dim`] paints.
pub struct Dim {
    color: Color,
}

impl<M> Hooks<M> for Dim {
    type State = Stateless;

    fn draw(
        &self,
        _state: &Stateless,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        let color = Color {
            a: self.color.a * STYLE.get().opacity(SHOWN.get()),
            ..self.color
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: at.bounds,
                ..Default::default()
            },
            color,
        );
        child(renderer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_popup_opens_eased_out_over_its_duration() {
        let start = Instant::now();
        let style = ThemeMotion::default();
        assert_eq!(shown(style, false, start, start), 0.0);
        let half = shown(style, false, start, start + DURATION / 2);
        assert!(half > 0.5, "{half}");
        assert_eq!(shown(style, false, start, start + DURATION), 1.0);
        assert_eq!(shown(style, false, start, start + DURATION * 3), 1.0);
    }

    #[test]
    fn a_popup_opened_again_as_it_closes_grows_on_from_where_it_was() {
        let closing = Instant::now();
        let now = closing + DURATION / 4;
        let style = ThemeMotion::default();
        let open = 1.0 - shown(style, true, closing, now);
        let again = reopened(style, closing, now);
        assert!((shown(style, false, again, now) - open).abs() < 1e-3);
        // Closed all the way, it opens from the start.
        let gone = closing + DURATION;
        assert!(shown(style, false, reopened(style, closing, gone), gone) < 1e-3);
    }
}
