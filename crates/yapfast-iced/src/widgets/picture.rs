//! Pictures drawn one to one: still ([`Pic`]) and moving ([`Animated`]).

use std::{cell::RefCell, collections::HashMap, sync::Arc, time::Duration};

use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, image::Handle, layout, mouse, renderer, widget::Tree,
    },
    widget::{Container, container},
    window,
};
use yapfast_core::chat::Picture as CorePicture;

use crate::{app::Message, paint, pictures::Player};

use super::{Renderer, draw_picture};

/// A picture drawn one to one (the patched renderer copies it in), with
/// round corners and an opacity. It takes `size` in the layout but may be
/// drawn elsewhere around it (`at`, from the layout's corner), as a
/// decoration overhangs its avatar.
pub struct Pic {
    handle: Handle,
    size: Size,
    at: Rectangle,
    radius: [f32; 4],
    opacity: f32,
}

impl Pic {
    pub fn new(handle: Handle, size: Size) -> Self {
        Self {
            handle,
            size,
            at: Rectangle::with_size(size),
            radius: [0.0; 4],
            opacity: 1.0,
        }
    }

    /// A square picture, `side` across.
    pub fn square(handle: Handle, side: f32) -> Self {
        Self::new(handle, Size::new(side, side))
    }

    /// Centred in a box `side` across, as an icon on a button's face or a
    /// badge; the box can be styled further.
    pub fn boxed<'a>(self, side: impl Into<Length>) -> Container<'a, Message, Theme, Renderer> {
        container(self).center(side)
    }

    pub fn at(mut self, at: Rectangle) -> Self {
        self.at = at;
        self
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
}

impl Widget<Message, Theme, Renderer> for Pic {
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
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
        draw_picture(renderer, &self.handle, bounds, self.radius, self.opacity);
    }
}

impl<'a> From<Pic> for Element<'a, Message> {
    fn from(picture: Pic) -> Self {
        Element::new(picture)
    }
}

/// A moving picture on screen: its decoder and the frame showing.
pub struct Playing {
    player: Player,
    frame: Option<Handle>,
}

/// A moving picture by its core picture and the size it's drawn at.
pub type PlayKey = (usize, u32, u32);

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
    playing: &'a RefCell<HashMap<PlayKey, Playing>>,
}

impl<'a> Animated<'a> {
    pub fn new(
        picture: Arc<CorePicture>,
        size: Size,
        at: Rectangle,
        cover: bool,
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

impl Widget<Message, Theme, Renderer> for Animated<'_> {
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let iced::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        // Rows built in the margin around the viewport don't play.
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
        if !bounds.intersects(viewport) {
            return;
        }
        let key = self.key();
        let mut playing = self.playing.borrow_mut();
        let Some(entry) = (match playing.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                Player::new(&self.picture).map(|player| {
                    entry.insert(Playing {
                        player,
                        frame: None,
                    })
                })
            }
        }) else {
            return;
        };
        let now = *now;
        if let Some(due) = entry.player.due()
            && due <= now + Duration::from_millis(4)
            && let Some(mut rgba) = entry.player.next(due.max(now))
        {
            paint::to_native(&mut rgba);
            let (width, height) = (self.picture.width, self.picture.height);
            let (to_width, to_height) = (self.at.width as u32, self.at.height as u32);
            entry.frame = Some(if (width, height) == (to_width, to_height) {
                Handle::from_rgba(width, height, rgba)
            } else {
                // Fitted to the size drawn, so it's copied in one to one.
                let fitted =
                    paint::fitted(&rgba, (width, height), (to_width, to_height), self.cover);
                Handle::from_rgba(to_width, to_height, fitted)
            });
        }
        if let Some(due) = entry.player.due() {
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
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
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

impl<'a> From<Animated<'a>> for Element<'a, Message> {
    fn from(animated: Animated<'a>) -> Self {
        Element::new(animated)
    }
}
