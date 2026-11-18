//! The picture widget: a [`Player`] in its tree state, at a size, showing
//! a still or a moving picture's newest frame ([`live`]).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::border::Radius;
use iced::widget::image;
use iced::{Event, Length, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::kind::Live;

use super::look::Fit;
use super::player::{Player, Shape};

/// A picture at a size, in a shape, moving while it may: see [`picture`]
/// and [`live`].
pub struct Picture {
    source: Source,
    width: Length,
    height: Length,
    shape: Shape,
    opacity: f32,
    motion: Motion,
}

/// `handle` at its own size, square-cornered and still; the setters say
/// otherwise. A handle whose picture moves plays once [`Picture::motion`]
/// lets it.
pub fn picture(handle: image::Handle) -> Picture {
    Picture::of(Source::Still(handle))
}

/// The newest frame of `frames`, read as it's drawn: a frame that comes
/// redraws it, and builds and lays out nothing. Its frames are drawn as
/// they are while they come, with no copy (the `Player`'s feed), cut to
/// its corners where they're drawn; it takes its size from its first
/// frame until it's laid out again, so give it one ([`Picture::width`]).
pub fn live(frames: &Live) -> Picture {
    Picture::of(Source::Live(frames.clone()))
}

/// What a picture shows: a still, or a moving picture's newest frame.
enum Source {
    Still(image::Handle),
    Live(Live),
}

impl Picture {
    fn of(source: Source) -> Self {
        Self {
            source,
            width: Length::Shrink,
            height: Length::Shrink,
            shape: Shape::default(),
            opacity: 1.0,
            motion: Motion::default(),
        }
    }

    /// Calls `with` with what it shows now; none before a frame came.
    fn showing<T>(&self, with: impl FnOnce(&image::Handle) -> T) -> Option<T> {
        match &self.source {
            Source::Still(handle) => Some(with(handle)),
            Source::Live(frames) => frames.newest().as_ref().map(with),
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// A square `side` logical pixels across.
    pub fn side(self, side: f32) -> Self {
        self.width(side).height(side)
    }

    /// Round, as an avatar.
    pub fn round(mut self) -> Self {
        self.shape.round = true;
        self
    }

    /// Rounded corners, in logical pixels.
    pub fn radius(mut self, radius: impl Into<Radius>) -> Self {
        self.shape.radius = radius.into();
        self
    }

    pub fn fit(mut self, fit: Fit) -> Self {
        self.shape.fit = fit;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Plays a moving picture while `motion` plays; without it, it holds
    /// its first frame.
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    fn intrinsic(&self, renderer: &Renderer) -> Size {
        use iced::advanced::image::Renderer as _;
        let size = self
            .showing(|handle| match handle {
                image::Handle::Rgba { width, height, .. } => Size::new(*width, *height),
                handle => renderer.measure_image(handle).unwrap_or_default(),
            })
            .unwrap_or_default();
        Size::new(size.width as f32, size.height as f32)
    }
}

impl widget::Meta for Picture {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Picture {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Player>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Player::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let intrinsic = self.intrinsic(renderer);
        tree.size = limits.resolve(self.width, self.height, intrinsic);
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
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let player = tree.state.downcast_mut::<Player>();
            let due = self.showing(|handle| {
                player.update(handle, layout.bounds(), viewport, self.motion, *now)
            });
            if let Some(due) = due.flatten() {
                shell.request_redraw_at(due);
            }
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
        let player = tree.state.downcast_ref::<Player>();
        let _ = self.showing(|handle| {
            player.draw(
                renderer,
                handle,
                layout.bounds(),
                self.shape,
                self.opacity,
                viewport,
            );
        });
    }
}
