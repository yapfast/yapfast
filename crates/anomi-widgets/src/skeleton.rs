//! Placeholders while something loads, with their shimmer: `Skeleton` and
//! `MessagesSkeleton` (`ui/controls.slint:1108`, `:1123`).
//!
//! A skeleton is a step off its ground (lighter on a dark theme, darker on a
//! light one, whose raised surfaces are near white and would hide the white
//! sweep) with a light band sweeping across it as in Discord. The band moves
//! only while someone may be looking ([`Motion::watched`]): media can stay
//! unloaded for ever, and the sweep asks for a frame each tick, which a
//! window behind another, or one nobody has touched for a while, mustn't.
//! Without the band the skeleton is its fill alone and draws once.
//!
//! The band's place comes from one clock shared by every skeleton, so those
//! side by side sweep together, as Slint's `animation-tick()` made them.

mod messages;
mod sweep;

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::border::Radius;
use iced::{Border, Color, Event, Length, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};

pub use messages::messages_skeleton;

/// The ground of a skeleton on a theme.
pub fn ground(theme: &Theme) -> Color {
    let palette = theme.palette();
    if palette.dark {
        palette.raised
    } else {
        palette.hover
    }
}

/// Whether a skeleton draws and asks for frames of its band.
fn sweeping(motion: Motion) -> bool {
    motion.watched()
}

/// Whether the band moves now: watched, and in sight of `viewport`.
fn animating(motion: Motion, bounds: Rectangle, viewport: &Rectangle) -> bool {
    sweeping(motion) && bounds.intersects(viewport)
}

/// A placeholder of a size and a corner radius, no taller than it says.
pub struct Skeleton {
    width: Length,
    height: Length,
    radius: f32,
    motion: Motion,
}

/// Builds a skeleton; size it with [`Skeleton::width`] and [`Skeleton::height`].
pub fn skeleton(motion: Motion) -> Skeleton {
    Skeleton {
        width: Length::Fill,
        height: Length::Fill,
        radius: 0.0,
        motion,
    }
}

impl Skeleton {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Corner radius, as `border-radius` of the Slint skeleton.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

struct State {
    /// The frame the band is drawn at.
    now: Instant,
}

impl widget::Meta for Skeleton {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Skeleton {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.width(self.width).height(self.height).resolve(
            self.width,
            self.height,
            Size::ZERO,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && animating(self.motion, layout.bounds(), viewport)
        {
            tree.state.downcast_mut::<State>().now = *now;
            shell.request_redraw();
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
        let bounds = layout.bounds();
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return;
        }
        let radius = self.radius.min(bounds.height / 2.0).min(bounds.width / 2.0);
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border {
                    radius: Radius::new(radius),
                    ..Border::default()
                },
                ..Quad::default()
            },
            ground(theme),
        );
        if animating(self.motion, bounds, viewport) {
            let now = tree.state.downcast_ref::<State>().now;
            sweep::draw(renderer, bounds, radius, theme.palette().shimmer, now);
        }
    }
}

#[cfg(test)]
mod tests;
