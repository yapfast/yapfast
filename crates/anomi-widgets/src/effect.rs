//! A profile effect over a card: its still at rest, and while it plays its
//! layers, moving pictures laid over the whole card, each shown from its
//! start for its duration, and again after its delay when it loops. It
//! plays while the pointer is on it, once for an effect that doesn't loop,
//! and holds still otherwise; clicks go through it.
//!
//! Each time a layer shows it gets a new [`Player`]: a picture that plays a
//! number of times starts from its first frame, as Slint's `Picture` made
//! anew did; one that plays for ever shows the frame the shared clock is
//! at, as [`Player`] has no way yet to start it from its first. F16's
//! review asks F05, who owns it, for one (`Player::from_start`); once it's
//! there, `frame` makes the layers' players with it and this goes.
//!
//! The Slint side: `ProfileEffect` and `EffectLayerView`
//! (`ui/controls.slint:860`, `:833`); the layers as `effect_layers` makes
//! them, and the end and whether one loops as `profile_data` works them out
//! (`src/ui/profile.rs:570`, `:652`). The card lays it over its contents
//! (`ui/people.slint:2016`), the server profile editor's effect cell holds
//! it still (`ui/server.slint:963`).
//!
//! A layer whose picture hasn't come draws nothing; with
//! [`ProfileEffect::on_wanted`] it says so as it comes to show, so the app
//! asks for the layers' pictures only for a card someone points at, as
//! `EffectLayerView` asked for its own as it showed (`:851`).
//!
//! Nothing ticks: while it plays it asks for a frame when a layer shows or
//! hides and when it ends, and each layer's [`Player`] asks for its own
//! frames; held still, behind another window or with animations off, it
//! draws its still and asks for none. The scale its pictures are copied at
//! is the app's to set (`picture::set_scale`).

pub(crate) mod kind;
mod playback;
pub mod sample;
#[cfg(test)]
pub(crate) mod tests;

use std::time::Instant;

use iced::advanced::image::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::border::Radius;
use iced::widget::image;
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::{Motion, Renderer, Theme};

use crate::picture::{Player, Shape};
use playback::{Playback, shown};

/// The effect as a profile's view-model carries it.
pub use kind::{Effect, EffectLayer};

/// What makes an effect play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Play {
    /// While the pointer is on it, as the card's `card-pointer.has-hover`.
    #[default]
    Hover,
    /// Never: it shows its still, as the editor's cell has it.
    Held,
    /// While this holds, for a place that knows better than its own
    /// bounds; a change starts it over, as a hover does.
    While(bool),
}

/// An effect laid over a card; see [`effect`].
pub struct ProfileEffect<'a, Message> {
    effect: &'a Effect,
    motion: Motion,
    play: Play,
    width: Length,
    height: Length,
    radius: f32,
    on_wanted: Option<Message>,
}

/// `effect` over a card, filling it, playing on hover while `motion` plays.
/// Lay it over the card's contents in a `stack!`, the same size.
pub fn effect<Message>(effect: &Effect, motion: Motion) -> ProfileEffect<'_, Message> {
    ProfileEffect {
        effect,
        motion,
        play: Play::Hover,
        width: Length::Fill,
        height: Length::Fill,
        radius: 0.0,
        on_wanted: None,
    }
}

impl<Message> ProfileEffect<'_, Message> {
    pub fn play(mut self, play: Play) -> Self {
        self.play = play;
        self
    }

    /// Sends `message` when a layer comes to show before its picture has
    /// come, once a play: the app asks for the layers' pictures then.
    pub fn on_wanted(mut self, message: Message) -> Self {
        self.on_wanted = Some(message);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// The card's corner radius: what lies at its top left is cut to its
    /// top corners, as the card's rounded clip cut it in Slint. iced clips
    /// to rectangles only, so a picture reaching past the card's foot keeps
    /// its square corners there.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    fn asked(&self, hovered: bool) -> bool {
        match self.play {
            Play::Hover => hovered,
            Play::Held => false,
            Play::While(play) => play,
        }
    }

    /// Where a picture lying at `at` on the card draws, as wide as the card
    /// and as tall as that makes it, and its corners.
    fn place(&self, bounds: Rectangle, at: Vector, size: Size<u32>) -> Option<(Rectangle, Shape)> {
        if size.width == 0 {
            return None;
        }
        let height = bounds.width * size.height as f32 / size.width as f32;
        let drawn = Rectangle::new(bounds.position() + at, Size::new(bounds.width, height));
        let radius = if at == Vector::ZERO {
            Radius::default().top(self.radius)
        } else {
            Radius::default()
        };
        let shape = Shape {
            radius,
            ..Shape::default()
        };
        Some((drawn, shape))
    }
}

#[derive(Default)]
struct State {
    hovered: bool,
    playback: Playback,
    /// Each layer's player while it shows, made afresh each time it does.
    layers: Vec<Option<Player>>,
    still: Player,
    /// It said in this play that a layer's picture is wanted.
    wanted: bool,
}

impl<Message> widget::Meta for ProfileEffect<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for ProfileEffect<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(self.width, self.height, Size::ZERO);
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
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(_) | Event::Touch(_) if self.play == Play::Hover => {
                let hovered = cursor.is_over(bounds);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if self.play == Play::Hover {
                    state.hovered = cursor.is_over(bounds);
                }
                let due = self.frame(state, bounds, renderer, viewport, *now);
                if let Some(due) = due {
                    shell.request_redraw_at(due);
                }
                if self.wants(state)
                    && let Some(wanted) = &self.on_wanted
                {
                    shell.publish(wanted.clone());
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            if !state.playback.playing() {
                if let Some(still) = &self.effect.still
                    && let Some((drawn, shape)) =
                        self.place(bounds, Vector::ZERO, size_of(renderer, still))
                {
                    state.still.draw(renderer, still, drawn, shape, 1.0, &clip);
                }
                return;
            }
            for (layer, player) in self.effect.layers.iter().zip(&state.layers) {
                if let (Some(player), Some(picture)) = (player, &layer.picture)
                    && let Some((drawn, shape)) =
                        self.place(bounds, offset(layer), size_of(renderer, picture))
                {
                    player.draw(renderer, picture, drawn, shape, 1.0, &clip);
                }
            }
        });
    }
}

impl<Message> ProfileEffect<'_, Message> {
    /// Whether to say now that a layer's picture is wanted: one shows
    /// without it, and that wasn't said yet in this play.
    fn wants(&self, state: &mut State) -> bool {
        if !state.playback.playing() {
            state.wanted = false;
            return false;
        }
        let at = state.playback.at();
        let missing = |layer: &EffectLayer| layer.picture.is_none() && shown(layer, at);
        let missing = self.effect.layers.iter().any(missing);
        let wants = missing && !state.wanted;
        state.wanted |= missing;
        wants
    }

    /// Takes the frame at `now`: moves the play on, starts the players of
    /// the layers that came to show and drops those of the layers that
    /// hid, and says when the next frame is due.
    fn frame(
        &self,
        state: &mut State,
        bounds: Rectangle,
        renderer: &Renderer,
        viewport: &Rectangle,
        now: Instant,
    ) -> Option<Instant> {
        let layers = &self.effect.layers;
        let (end, loops) = (self.effect.end(), self.effect.loops());
        let asked = self.asked(state.hovered);
        let may = self.motion.playing() && !layers.is_empty();
        let playing = state.playback.frame(asked, may, end, loops, now);
        let at = state.playback.at();
        let mut due = state.playback.next(layers, end, loops, now);
        let mut sooner = |next: Option<Instant>| due = due.into_iter().chain(next).min();
        state.layers.resize_with(layers.len(), || None);
        for (layer, slot) in layers.iter().zip(&mut state.layers) {
            let picture = layer
                .picture
                .as_ref()
                .filter(|_| playing && shown(layer, at));
            let Some(picture) = picture else {
                *slot = None;
                continue;
            };
            let player = slot.get_or_insert_with(Player::default);
            if let Some((drawn, _)) = self.place(bounds, offset(layer), size_of(renderer, picture))
            {
                sooner(player.update(picture, drawn, viewport, self.motion, now));
            }
        }
        if !playing
            && let Some(still) = &self.effect.still
            && let Some((drawn, _)) = self.place(bounds, Vector::ZERO, size_of(renderer, still))
        {
            // A still, as Slint's `Image`: it never moves.
            sooner(
                state
                    .still
                    .update(still, drawn, viewport, Motion::default(), now),
            );
        }
        due
    }
}

fn offset(layer: &EffectLayer) -> Vector {
    Vector::new(layer.x, layer.y)
}

/// A picture's size in pixels: its own for pixels, else as the renderer
/// reads it; nothing for one it can't. The frame's layers' too.
pub(crate) fn size_of(renderer: &Renderer, handle: &image::Handle) -> Size<u32> {
    match handle {
        image::Handle::Rgba { width, height, .. } => Size::new(*width, *height),
        _ => renderer.measure_image(handle).unwrap_or_default(),
    }
}
