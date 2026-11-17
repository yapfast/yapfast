//! What a name drawn from outlines keeps in its tree state until what it's
//! drawn for changes: the effect's geometry, or for a faded name whose
//! effect stacks one thing on another, a picture of it.
//!
//! Slint fades a `StyledName` as one layer (`ui/people.slint:308`, an
//! offline member), so neon's, toon's and pop's edges, drawn under the
//! letters, never show through them. iced fades each fill on its own, which
//! would let the edge through every letter: such a name is drawn once at
//! full opacity into a picture at the window's scale, by a renderer of its
//! own, and the picture drawn at the name's opacity.

use std::cell::RefCell;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::image::{self, Renderer as _};
use iced::advanced::renderer::{Headless as _, Renderer as _};
use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Size, Vector};

use anomi_theme::Renderer;

use crate::picture;

use super::StyledName;
use super::colours::Colours;
use super::outline;
use super::paint::{self, Paint};
use super::style::{Effect, Style};
use super::widget::State;

/// What the outlines were drawn for: when any of it changes they're drawn
/// again.
#[derive(Clone, Copy, PartialEq)]
struct Drawn {
    shaped: u64,
    style: Style,
    dark: bool,
    opacity: u32,
    size: (u32, u32),
    scale: u32,
}

/// A faded name's effect drawn whole: its pixels, and its size in logical
/// pixels.
struct Faded {
    handle: image::Handle,
    size: Size,
}

/// The effect as last drawn, and what for.
pub(super) struct Kept {
    drawn: RefCell<Option<Drawn>>,
    outlines: canvas::Cache<Renderer>,
    faded: RefCell<Option<Faded>>,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            drawn: RefCell::default(),
            outlines: canvas::Cache::new(),
            faded: RefCell::default(),
        }
    }
}

impl Kept {
    /// Forgets what was drawn when it was drawn for something else.
    fn keep(&self, drawn: Drawn) {
        let mut kept = self.drawn.borrow_mut();
        if *kept != Some(drawn) {
            self.outlines.clear();
            *self.faded.borrow_mut() = None;
            *kept = Some(drawn);
        }
    }
}

impl StyledName<'_> {
    /// Draws an effect from the name's outlines, made again only when what
    /// they're drawn for changed.
    pub(super) fn outlined(
        &self,
        state: &State,
        renderer: &mut Renderer,
        (dark, viewport): (bool, &Rectangle),
        (bounds, anchor): (Rectangle, Point),
        (effect, colours): (Effect, &Colours),
    ) {
        let Some(style) = self.style else {
            return;
        };
        let scale = picture::scale();
        let kept = &state.kept;
        kept.keep(Drawn {
            shaped: state.shaped,
            style,
            dark,
            opacity: self.opacity.to_bits(),
            size: (bounds.width.to_bits(), bounds.height.to_bits()),
            scale: scale.to_bits(),
        });
        let reach = paint::reach(self.size);
        let clip = Rectangle::new(
            Point::new(-reach, -reach),
            Size::new(bounds.width + 2.0 * reach, bounds.height + 2.0 * reach),
        );
        let faded = self.opacity < 1.0 && paint::stacked(effect);
        let draw = |frame: &mut canvas::Frame<Renderer>| {
            let origin = Point::new(0.0, anchor.y - bounds.y);
            let glyphs = outline::glyphs(state.paragraph.raw(), origin);
            let paint = Paint {
                glyphs: &glyphs,
                colours,
                size: bounds.size(),
                em: self.size,
                line_height: self.line(state),
                // A faded picture is drawn whole, and faded as one.
                opacity: if faded { 1.0 } else { self.opacity },
            };
            paint::effect(frame, effect, &paint);
        };
        if !faded {
            let geometry = kept.outlines.draw_with_bounds(renderer, clip, draw);
            renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
                renderer.draw_geometry(geometry);
            });
            return;
        }
        let mut made = kept.faded.borrow_mut();
        if made.is_none() {
            let mut frame = canvas::Frame::with_bounds(renderer, clip);
            draw(&mut frame);
            *made = whole(renderer, frame.into_geometry(), clip, scale);
        }
        let Some(faded) = made.as_ref() else {
            return;
        };
        // On the window's pixel grid, so the picture's pixels are the
        // window's and the letters stay sharp.
        let origin = bounds.position() + Vector::new(clip.x, clip.y);
        let snap = |value: f32| (value * scale).round() / scale;
        let at = Rectangle::new(Point::new(snap(origin.x), snap(origin.y)), faded.size);
        if let Some(shown) = at.intersection(viewport) {
            let picture = image::Image {
                opacity: self.opacity,
                ..image::Image::new(faded.handle.clone())
            };
            renderer.draw_image(picture, at, shown);
        }
    }
}

/// `geometry`, drawn within `clip`, as a picture at `scale` physical pixels
/// a logical one, by a renderer of its own: none when it has no pixels.
fn whole(
    renderer: &Renderer,
    geometry: canvas::Geometry<Renderer>,
    clip: Rectangle,
    scale: f32,
) -> Option<Faded> {
    let width = (clip.width * scale).ceil();
    let height = (clip.height * scale).ceil();
    if !(width >= 1.0 && height >= 1.0) {
        return None;
    }
    let (width, height) = (width as u32, height as u32);
    let size = Size::new(width as f32 / scale, height as f32 / scale);
    let mut own = Renderer::new(renderer.settings());
    own.reset(Rectangle::with_size(size));
    own.with_translation(Vector::new(-clip.x, -clip.y), |own| {
        own.draw_geometry(geometry);
    });
    let mut pixels = own.screenshot(Size::new(width, height), scale, Color::TRANSPARENT);
    unpremultiply(&mut pixels);
    Some(Faded {
        handle: image::Handle::from_rgba(width, height, pixels),
        size,
    })
}

/// tiny-skia's pixels carry their colour multiplied by their alpha; a
/// picture's don't.
fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            let straight = (u16::from(*channel) * 255 + alpha / 2) / alpha;
            *channel = straight.min(255) as u8;
        }
    }
}
