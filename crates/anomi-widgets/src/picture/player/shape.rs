//! How a picture is drawn in a box: its corners, how it fills the box, and
//! the size of its copy at a scale.

use iced::border::Radius;
use iced::{ContentFit, Point, Rectangle, Size, Vector};

use super::super::look::{Fit, Look, SMALL};

/// How a picture is drawn: its corners, how it fills its box, how opaque.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape {
    /// The corners' radii in logical pixels.
    pub radius: Radius,
    /// Round: corners of half the shorter side, whatever `radius` says.
    pub round: bool,
    pub fit: Fit,
    /// A moving picture's frames, made at the size they show, up to
    /// [`EXACT`] pixels short of it on a side (`yapfast_core::picture::fit`
    /// makes even sides): within that, one is drawn one to one, centred on
    /// whole pixels, which the renderer copies in (0.10 ms for 720p) rather
    /// than stretching it by a fraction of a pixel (0.36 ms).
    pub exact: bool,
}

/// How many pixels a side of an [`Shape::exact`] frame may miss the size it
/// shows at and still be drawn one to one.
pub const EXACT: f32 = 2.0;

impl Shape {
    /// The corners for a box of `bounds`.
    pub(super) fn radius(&self, bounds: Rectangle) -> Radius {
        if self.round {
            Radius::from(bounds.width.min(bounds.height) / 2.0)
        } else {
            self.radius
        }
    }

    /// Whether a box of `bounds` has round corners, which the renderer cuts
    /// where the picture is drawn.
    pub(crate) fn rounded(&self, bounds: Rectangle) -> bool {
        <[f32; 4]>::from(self.radius(bounds))
            .iter()
            .any(|&corner| corner > 0.0)
    }

    /// Where a `width` by `height` picture draws in `bounds`, and its copy
    /// at `scale`:
    ///
    /// - with corners, at the pixels it's drawn at, even more than the
    ///   picture has: the renderer copies it in on whole pixels and cuts its
    ///   corners there, where scaling a smaller copy up would go through
    ///   tiny-skia's pattern pipeline at each frame;
    /// - cut to its box, at those pixels but never more than the picture
    ///   has, which tiny-skia scales up instead;
    /// - only shrunk and small (a decoration, a tag's badge, an emoji), at
    ///   the pixels it's drawn at too, which the renderer copies in rather
    ///   than scaling a larger copy through tiny-skia's pattern pipeline at
    ///   each frame drawn (0.1 ms a frame for a decoration of 38 pixels
    ///   scaled from 48); the copy is smaller than the picture;
    /// - only shrunk and larger than [`SMALL`], at a mip level: each side
    ///   halved until once more would be fewer pixels than drawn, so every
    ///   size within twice of it shares one copy, as a picture zoomed or a
    ///   pane resized draws one at each frame, and tiny-skia scales the
    ///   rest. Drawn larger than the picture, that's the picture itself.
    ///
    /// The others differ for every pixel the size does; [`Player`](super::Player)
    /// keeps one while the size changes.
    pub(crate) fn look(
        &self,
        bounds: Rectangle,
        (width, height): (u32, u32),
        scale: f32,
    ) -> (Rectangle, Look) {
        let source = Size::new(width.max(1) as f32, height.max(1) as f32);
        let mut drawn = if self.fit == Fit::Contain {
            centred(ContentFit::Contain.fit(source, bounds.size()), bounds)
        } else {
            bounds
        };
        if self.exact {
            drawn = one_to_one(drawn, source, scale).unwrap_or(drawn);
        }
        let wanted = Size::new(
            (drawn.width * scale).max(1.0),
            (drawn.height * scale).max(1.0),
        );
        let pixels = |side: f32| side.round().max(1.0) as u32;
        let (w, h) = if self.rounded(drawn) {
            (pixels(wanted.width), pixels(wanted.height))
        } else if self.fit.cuts() {
            let has = ContentFit::Contain.fit(wanted, source);
            let shrink = (has.width / wanted.width)
                .min(has.height / wanted.height)
                .min(1.0);
            (
                pixels(wanted.width * shrink),
                pixels(wanted.height * shrink),
            )
        } else if pixels(wanted.width).max(pixels(wanted.height)) <= SMALL {
            (
                pixels(wanted.width).min(width.max(1)),
                pixels(wanted.height).min(height.max(1)),
            )
        } else {
            (
                mip(width, pixels(wanted.width)),
                mip(height, pixels(wanted.height)),
            )
        };
        let look = Look {
            width: w,
            height: h,
            cover: self.fit.cuts(),
            top: self.fit == Fit::Top,
        };
        (drawn, look)
    }
}

/// `drawn` holding a `source` picture of as many pixels within [`EXACT`]
/// of its own at `scale`: the picture at its own pixels, centred where
/// `drawn` is, on whole pixels.
fn one_to_one(drawn: Rectangle, source: Size, scale: f32) -> Option<Rectangle> {
    let near = |side: f32, wanted: f32| (side - wanted * scale).abs() <= EXACT;
    if scale <= 0.0 || !near(source.width, drawn.width) || !near(source.height, drawn.height) {
        return None;
    }
    let whole = |at: f32, room: f32, side: f32| {
        ((at * scale + (room * scale - side) / 2.0).round()) / scale
    };
    Some(Rectangle {
        x: whole(drawn.x, drawn.width, source.width),
        y: whole(drawn.y, drawn.height, source.height),
        width: source.width / scale,
        height: source.height / scale,
    })
}

/// `size` centred on `bounds`.
pub(super) fn centred(size: Size, bounds: Rectangle) -> Rectangle {
    Rectangle::new(
        bounds.center() - Vector::new(size.width / 2.0, size.height / 2.0),
        size,
    )
}

/// `size` centred across `bounds`, its top at theirs.
pub(super) fn topped(size: Size, bounds: Rectangle) -> Rectangle {
    Rectangle::new(
        Point::new(bounds.center_x() - size.width / 2.0, bounds.y),
        size,
    )
}

/// A side of `side` pixels halved while half of it still covers `wanted`.
fn mip(side: u32, wanted: u32) -> u32 {
    let mut side = side.max(1);
    while side > 1 && side.div_ceil(2) >= wanted {
        side = side.div_ceil(2);
    }
    side
}
