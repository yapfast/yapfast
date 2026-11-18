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

/// How long a popup takes to open.
pub const DURATION: Duration = Duration::from_millis(100);
/// How large a card opens from.
pub const FROM: f32 = 0.8;

thread_local! {
    /// How far open the layer being drawn is, eased, from 0 to 1: set by
    /// the [`Motion`] drawing it, 1 outside one.
    static SHOWN: Cell<f32> = const { Cell::new(1.0) };
}

/// How far open a layer is that opened at `start`, at `now`, eased out
/// on a quadratic curve.
pub fn shown(start: Instant, now: Instant) -> f32 {
    let t = (now.saturating_duration_since(start).as_secs_f32() / DURATION.as_secs_f32()).min(1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// For a layer closing since `start`, when it would have started opening
/// to be as far open at `now` as it is: a popup opened again as it closes
/// grows on from there.
pub fn reopened(start: Instant, now: Instant) -> Instant {
    let open = 1.0 - shown(start, now);
    // `shown` backwards: the time at which it's `open`.
    let t = 1.0 - (1.0 - open).sqrt();
    now - DURATION.mul_f32(t)
}

/// A layer opening or closing: `start` is when it was first drawn so,
/// kept by the one who keeps what's open, as the widget tree's state
/// would go with a layer opened or closed under it.
pub struct Motion<'a, M> {
    start: &'a Cell<Option<Instant>>,
    /// While it closes, what it says once it's closed.
    closed: Option<M>,
}

impl<M> Motion<'_, M> {
    /// How far open it is at `now`, its start set if it had none.
    fn shown(&self, now: Instant) -> f32 {
        let start = *self.start.get().get_or_insert(now);
        self.start.set(Some(start));
        let opened = shown(start, now);
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
) -> Element<'a, M> {
    Wrap::new(
        layer,
        Motion {
            start,
            closed: None,
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
) -> Element<'a, M> {
    Wrap::new(
        layer,
        Motion {
            start,
            closed: Some(closed),
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
            if now.saturating_duration_since(start) < DURATION {
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
        child(renderer);
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
        let scale = FROM + (1.0 - FROM) * shown;
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
            a: self.color.a * SHOWN.get(),
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
        assert_eq!(shown(start, start), 0.0);
        let half = shown(start, start + DURATION / 2);
        // Eased out: past half way at half the time.
        assert!((half - 0.75).abs() < 1e-4, "{half}");
        assert_eq!(shown(start, start + DURATION), 1.0);
        assert_eq!(shown(start, start + DURATION * 3), 1.0);
    }

    #[test]
    fn a_popup_opened_again_as_it_closes_grows_on_from_where_it_was() {
        let closing = Instant::now();
        let now = closing + DURATION / 4;
        let open = 1.0 - shown(closing, now);
        let again = reopened(closing, now);
        assert!((shown(again, now) - open).abs() < 1e-3);
        // Closed all the way, it opens from the start.
        let gone = closing + DURATION;
        assert!(shown(reopened(closing, gone), gone) < 1e-3);
    }
}
