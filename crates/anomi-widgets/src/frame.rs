//! A profile frame around a card, drawn as Discord's stylesheet does: its
//! layers behind the card's contents or over them, each a staple across the
//! card and past its sides once beyond the end it's anchored to, a rail the
//! card's height with its picture at its anchor, or a border repeated down
//! the card. All are stills, as the client's, and take no clicks.
//!
//! The Slint side: `ProfileFrameLayer` (`ui/people.slint:1153`), and the
//! card's two passes over its layers, the back ones before its contents and
//! the front ones after (`ui/people.slint:1482`, `:2032`); the rows as
//! `frame_rows` makes them (`src/ui/profile.rs:445`).
//!
//! The frame draws past the card's bounds, by 12 px past each side (16 on
//! the full profile) and its staples beyond the card's ends: what holds the
//! card leaves that room unclipped. How the card stacks its two sides with
//! its ground, contents and effect: [`frame`]. The card decides whether it
//! has one at all (none docked beside a DM, in the editor's preview, or
//! with frames off: `has-frame`, `:1479`). Its pictures are the app's to
//! ask for as the card shows, as `ProfileFrameLayer`'s `init` asked, and
//! the scale they're copied at the app's to set (`picture::set_scale`).

mod place;
pub mod sample;
#[cfg(test)]
mod tests;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::widget::image;
use iced::{Event, Length, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};

use crate::effect::size_of;
use crate::picture::{self, Player, Shape};
use place::{Placed, place};

/// A frame's layers as a profile's view-model carries them.
pub use crate::effect::kind::{FrameAnchor as Anchor, FrameKind as Kind, FrameLayer};

/// Which of a frame's layers a [`ProfileFrame`] draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// Those over the card's ground and behind its contents: push it under
    /// the contents with `Stack::push_under`, then the ground under it with
    /// a second (see [`frame`]).
    #[default]
    Back,
    /// Those over the contents: push it onto the stack after them, and
    /// after the profile effect.
    Front,
}

/// A frame's layers of one [`Side`] around a card; see [`frame`].
pub struct ProfileFrame<'a> {
    layers: &'a [FrameLayer],
    side: Side,
    large: bool,
}

/// The layers of `side` among `layers` (the back ones first, as a profile
/// carries them), around the card they're stacked with. A frame fills the
/// stack and draws past it, so the card's contents must be the stack's
/// base, its first child, which sizes it: a frame first, being Fill, would
/// make the stack, and the frame's idea of the card with any effect in the
/// same stack, all the room the card's parent gives rather than the card's
/// height. The back side goes in with `Stack::push_under`, then the card's
/// ground with a second, so the back layers draw over the ground and under
/// the contents, as Slint's back pass inside the card's root
/// (`ui/people.slint:1482`); the effect and the front side go on top:
///
/// ```ignore
/// stack![contents, effect(&effect, motion), frame(&layers, Side::Front)]
///     .push_under(frame(&layers, Side::Back).boxed())
///     .push_under(ground.boxed())
/// ```
///
/// where `ground` is the card's background, Fill by Fill.
pub fn frame(layers: &[FrameLayer], side: Side) -> ProfileFrame<'_> {
    ProfileFrame {
        layers,
        side,
        large: false,
    }
}

impl ProfileFrame<'_> {
    /// Around the full profile: 16 px past each side, its top staple 8 px
    /// lower, and none at its foot, as Discord's.
    pub fn large(mut self, large: bool) -> Self {
        self.large = large;
        self
    }

    /// `layer`'s picture when it's of this side, and where it draws around
    /// `card`.
    fn place<'b>(
        &self,
        layer: &'b FrameLayer,
        card: Rectangle,
        renderer: &Renderer,
    ) -> Option<(&'b image::Handle, Placed)> {
        let front = self.side == Side::Front;
        let picture = layer.picture.as_ref().filter(|_| layer.front == front)?;
        let size = size_of(renderer, picture);
        Some((
            picture,
            place(layer.kind, layer.anchor, card, size, self.large)?,
        ))
    }
}

/// One player a layer: a border's copies share it, as they share a size.
#[derive(Default)]
struct State {
    players: Vec<Player>,
}

impl widget::Meta for ProfileFrame<'_> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for ProfileFrame<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        _cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        state
            .players
            .resize_with(self.layers.len(), Player::default);
        let mut due = None;
        for (layer, player) in self.layers.iter().zip(&mut state.players) {
            let Some((picture, placed)) = self.place(layer, layout.bounds(), renderer) else {
                continue;
            };
            // Stills: a moving one holds its first frame.
            let next = player.update(picture, placed.first, viewport, Motion::default(), *now);
            due = due.into_iter().chain(next).min();
        }
        if let Some(due) = due {
            shell.request_redraw_at(due);
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
        let card = layout.bounds();
        let scale = picture::scale();
        // Before its first frame, a layer draws with a player of no past.
        let fresh = Player::default();
        for (index, layer) in self.layers.iter().enumerate() {
            let Some((picture, placed)) = self.place(layer, card, renderer) else {
                continue;
            };
            let player = state.players.get(index).unwrap_or(&fresh);
            let draw = |renderer: &mut Renderer, clip: &Rectangle| {
                for drawn in placed.pictures(scale) {
                    player.draw(renderer, picture, drawn, Shape::default(), 1.0, clip);
                }
            };
            match placed.clip {
                Some(inside) => {
                    if let Some(clip) = inside.intersection(viewport) {
                        renderer.with_layer(clip, |renderer| draw(renderer, &clip));
                    }
                }
                None => draw(renderer, viewport),
            }
        }
    }
}
