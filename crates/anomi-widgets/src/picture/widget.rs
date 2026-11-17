//! The picture widget: a [`Player`] in its tree state, at a size.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::border::Radius;
use iced::widget::image;
use iced::{Event, Length, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};

use super::look::Fit;
use super::player::{Player, Shape};

/// A picture at a size, in a shape, moving while it may: see [`picture`].
pub struct Picture {
    handle: image::Handle,
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
    Picture {
        handle,
        width: Length::Shrink,
        height: Length::Shrink,
        shape: Shape::default(),
        opacity: 1.0,
        motion: Motion::default(),
    }
}

impl Picture {
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
        let size = match &self.handle {
            image::Handle::Rgba { width, height, .. } => Size::new(*width, *height),
            handle => renderer.measure_image(handle).unwrap_or_default(),
        };
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
            let due = player.update(&self.handle, layout.bounds(), viewport, self.motion, *now);
            if let Some(due) = due {
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
        player.draw(
            renderer,
            &self.handle,
            layout.bounds(),
            self.shape,
            self.opacity,
            viewport,
        );
    }
}
