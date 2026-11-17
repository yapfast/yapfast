//! A view drawn headless at any scale, its frames at times the test gives,
//! and its pixels read back: what the effect's and the frame's looks tests
//! and their gallery share. The simulator does neither: its snapshots are
//! at 2x, and its frames at the clock's now.

use std::time::Instant;

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::{Headless as _, Renderer as _, Style};
use iced::advanced::shell::{Bus, Waker};
use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Shell, mouse};
use iced::theme::Base as _;
use iced::{Event, Point, Rectangle, Size, window};

use anomi_theme::{Element, Renderer, Theme, View};

use crate::name::tests::support::renderer;

/// A view laid out in a window of `size` logical pixels, the pointer where
/// it was last put.
pub(crate) struct Shot<'a> {
    root: Element<'a, ()>,
    tree: Tree,
    renderer: Renderer,
    size: Size,
    cursor: mouse::Cursor,
}

impl<'a> Shot<'a> {
    pub fn new(view: impl View<()> + 'a, size: Size) -> Self {
        let mut root = view.boxed();
        let renderer = renderer();
        let mut tree = Tree::empty();
        tree.diff(&mut root);
        root.layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        Self {
            root,
            tree,
            renderer,
            size,
            cursor: mouse::Cursor::Unavailable,
        }
    }

    fn send(&mut self, event: Event) {
        let mut bus = Bus::new();
        let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
        let layout = Layout::new(self.tree.size);
        self.root.update(
            &mut self.tree,
            &event,
            layout,
            self.cursor,
            &self.renderer,
            &mut shell,
            &Rectangle::with_size(self.size),
        );
    }

    pub fn point_at(&mut self, position: Point) {
        self.cursor = mouse::Cursor::Available(position);
        self.send(Event::Mouse(mouse::Event::CursorMoved { position }));
    }

    /// A frame at `now`, as the window sends before it draws.
    pub fn frame(&mut self, now: Instant) {
        self.send(Event::Window(window::Event::RedrawRequested(now)));
    }

    /// Its pixels in `theme`, `scale` physical pixels a logical one.
    pub fn pixels(&mut self, theme: &Theme, scale: f32) -> image::RgbaImage {
        let viewport = Rectangle::with_size(self.size);
        let base = theme.base();
        self.renderer.reset(viewport);
        self.root.draw(
            &self.tree,
            &mut self.renderer,
            theme,
            &Style {
                text_color: base.text_color,
            },
            Layout::new(self.tree.size),
            self.cursor,
            &viewport,
        );
        let physical = Size::new(
            (self.size.width * scale).round() as u32,
            (self.size.height * scale).round() as u32,
        );
        let rgba = self
            .renderer
            .screenshot(physical, scale, base.background_color);
        image::RgbaImage::from_raw(physical.width, physical.height, rgba)
            .expect("a pixel for each of the window's")
    }
}

/// The window's ground in `theme`, as a pixel.
pub(crate) fn ground(theme: &Theme) -> [u8; 4] {
    theme.base().background_color.into_rgba8()
}

/// Whether `a` and `b` are the same colour but for rounding and smoothing.
pub(crate) fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 4)
}
