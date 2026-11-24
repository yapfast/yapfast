//! A moving picture drawn one to one ([`Animated`]): on each of the
//! window's redraws while it's in view, the frame of the core's picture
//! that has fallen due, decoded by its [`Player`].

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{
    Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, image::Handle, layout, mouse, renderer, widget::Tree,
    },
    window,
};
use yapfast_core::chat::Picture as CorePicture;

use crate::{
    paint,
    pictures::{MediaLook, Player},
    widgets::{Renderer, draw_picture},
};

/// A moving picture on screen: its decoder and the frame showing.
pub struct Playing {
    player: Player,
    frame: Option<Handle>,
}

/// A moving picture by its core picture and the size it's drawn at.
pub type PlayKey = (usize, u32, u32);

/// Moving pictures playing, one player for each picture and size however
/// many places show it, and those the view being built shows: the others'
/// players go as each view ends.
#[derive(Default)]
pub struct Players {
    playing: RefCell<HashMap<PlayKey, Playing>>,
    shown: RefCell<HashSet<PlayKey>>,
}

impl Players {
    /// A view begins: what it shows is noted afresh.
    pub fn begin(&self) {
        self.shown.borrow_mut().clear();
    }

    /// `picture` playing in the view being built, drawn at `at` within a
    /// layout of `size`. Its player starts on the first redraw that finds
    /// it in view.
    pub fn animated(
        &self,
        picture: &Arc<CorePicture>,
        size: Size,
        at: Rectangle,
        cover: bool,
        media: MediaLook,
    ) -> Animated<'_> {
        let animated = Animated::new(picture.clone(), size, at, cover, media, &self.playing);
        self.shown.borrow_mut().insert(animated.key());
        animated
    }

    /// A view was built: the players of what it didn't show go.
    pub fn end(&self) {
        let shown = self.shown.borrow();
        self.playing
            .borrow_mut()
            .retain(|key, _| shown.contains(key));
    }

    /// Every player goes.
    pub fn clear(&self) {
        self.playing.borrow_mut().clear();
        self.shown.borrow_mut().clear();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.playing.borrow().is_empty()
    }

    /// The pictures and sizes playing.
    #[cfg(test)]
    pub fn keys(&self) -> Vec<PlayKey> {
        self.playing.borrow().keys().copied().collect()
    }
}

/// A moving picture: on each of the window's redraws while it's in view, it
/// shows the frame that has fallen due (fitted to the size it's drawn at)
/// and asks for a redraw when the next will. Every moving picture asks the
/// same window, so frames due together cost one repaint.
pub struct Animated<'a> {
    picture: Arc<CorePicture>,
    size: Size,
    at: Rectangle,
    cover: bool,
    radius: [f32; 4],
    opacity: f32,
    /// What shows until the first frame.
    still: Option<Handle>,
    media: MediaLook,
    playing: &'a RefCell<HashMap<PlayKey, Playing>>,
}

impl<'a> Animated<'a> {
    pub fn new(
        picture: Arc<CorePicture>,
        size: Size,
        at: Rectangle,
        cover: bool,
        media: MediaLook,
        playing: &'a RefCell<HashMap<PlayKey, Playing>>,
    ) -> Self {
        Self {
            picture,
            size,
            at,
            cover,
            radius: [0.0; 4],
            opacity: 1.0,
            still: None,
            media,
            playing,
        }
    }

    pub fn key(&self) -> PlayKey {
        (
            Arc::as_ptr(&self.picture) as usize,
            self.at.width as u32,
            self.at.height as u32,
        )
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = [radius; 4];
        self
    }

    pub fn radius_each(mut self, radius: [f32; 4]) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn still(mut self, still: Option<Handle>) -> Self {
        self.still = still;
        self
    }
}

impl<M> Widget<M, Theme, Renderer> for Animated<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.size.width),
            Length::Fixed(self.size.height),
        )
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(self.size)
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let iced::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        if let Some(due) = self.step(layout.position(), viewport, *now) {
            shell.request_redraw_at(due);
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        self.paint(renderer, layout.position());
    }
}

/// In a piece of text, over a custom emoji's span.
impl crate::piece::Moving for Animated<'_> {
    fn redraw(&self, at: Point, viewport: &Rectangle, now: Instant) -> Option<Instant> {
        self.step(at, viewport, now)
    }

    fn draw(&self, renderer: &mut Renderer, at: Point) {
        self.paint(renderer, at);
    }
}

impl Animated<'_> {
    /// A redraw at `now`, laid out at `origin`: the frame that fell due is
    /// made, and when the next falls due is said. One out of `viewport`
    /// makes nothing and asks for nothing.
    fn step(&self, origin: Point, viewport: &Rectangle, now: Instant) -> Option<Instant> {
        // Rows built in the margin around the viewport don't play.
        let bounds = self.at + Vector::new(origin.x, origin.y);
        if !bounds.intersects(viewport) {
            return None;
        }
        let key = self.key();
        let mut playing = self.playing.borrow_mut();
        let entry = match playing.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(Playing {
                player: Player::new(&self.picture)?,
                frame: None,
            }),
        };
        if let Some(due) = entry.player.due()
            && due <= now + Duration::from_millis(4)
            && let Some(mut rgba) = entry.player.next(due.max(now))
        {
            crate::pictures::apply_frame_media(
                self.media,
                &mut rgba,
                self.picture.width,
                self.picture.height,
            );
            paint::to_native(&mut rgba);
            let (width, height) = (self.picture.width, self.picture.height);
            // At the window's pixels, so the renderer copies it in unstretched.
            let scale = crate::widgets::pixel_scale();
            let (to_width, to_height) = (
                crate::pictures::window_pixels(self.at.width as u32, scale),
                crate::pictures::window_pixels(self.at.height as u32, scale),
            );
            entry.frame = Some(if (width, height) == (to_width, to_height) {
                Handle::from_rgba(width, height, rgba)
            } else {
                // Fitted to the size drawn, so it's copied in one to one.
                let fitted =
                    paint::fitted(&rgba, (width, height), (to_width, to_height), self.cover);
                Handle::from_rgba(to_width, to_height, fitted)
            });
        }
        entry.player.due()
    }

    /// The frame showing, or the still until there is one, laid out at
    /// `origin`.
    fn paint(&self, renderer: &mut Renderer, origin: Point) {
        let bounds = self.at + Vector::new(origin.x, origin.y);
        let playing = self.playing.borrow();
        let frame = playing
            .get(&self.key())
            .and_then(|entry| entry.frame.as_ref())
            .or(self.still.as_ref());
        if let Some(frame) = frame {
            draw_picture(renderer, frame, bounds, self.radius, self.opacity);
        }
    }
}

impl<'a, M: 'a> From<Animated<'a>> for Element<'a, M> {
    fn from(animated: Animated<'a>) -> Self {
        Element::new(animated)
    }
}
