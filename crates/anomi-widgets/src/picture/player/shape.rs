//! How a picture is drawn in a box: its corners, how it fills the box, and
//! the size and corners of its copy at a scale.

use iced::border::Radius;
use iced::{ContentFit, Rectangle, Size, Vector};

use super::super::look::{Fit, Look};

/// How a picture is drawn: its corners, how it fills its box, how opaque.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape {
    /// The corners' radii in logical pixels.
    pub radius: Radius,
    /// Round: corners of half the shorter side, whatever `radius` says.
    pub round: bool,
    pub fit: Fit,
}

impl Shape {
    /// The corners for a box of `bounds`.
    pub(super) fn radius(&self, bounds: Rectangle) -> Radius {
        if self.round {
            Radius::from(bounds.width.min(bounds.height) / 2.0)
        } else {
            self.radius
        }
    }

    /// Where a `width` by `height` picture draws in `bounds`, and its copy
    /// at `scale`:
    ///
    /// - with corners, at the pixels it's drawn at, even more than the
    ///   picture has: tiny-skia scaling a smaller copy up would scale its
    ///   corners with it, and a spoiler's ten-pixel blur would lose them;
    /// - cut to its box, at those pixels but never more than the picture
    ///   has, which tiny-skia scales up instead;
    /// - only shrunk, at a mip level: each side halved until once more
    ///   would be fewer pixels than drawn, so every size within twice of it
    ///   shares one copy, as a picture zoomed or a pane resized draws one at
    ///   each frame, and tiny-skia scales the rest. Drawn larger than the
    ///   picture, that's the picture itself.
    ///
    /// The first two differ for every pixel the size does; [`Player`](super::Player) keeps
    /// one while the size changes.
    pub(crate) fn look(
        &self,
        bounds: Rectangle,
        (width, height): (u32, u32),
        scale: f32,
    ) -> (Rectangle, Look) {
        let source = Size::new(width.max(1) as f32, height.max(1) as f32);
        let drawn = if self.fit == Fit::Contain {
            centred(ContentFit::Contain.fit(source, bounds.size()), bounds)
        } else {
            bounds
        };
        let wanted = Size::new(
            (drawn.width * scale).max(1.0),
            (drawn.height * scale).max(1.0),
        );
        let radius = <[f32; 4]>::from(self.radius(drawn));
        let pixels = |side: f32| side.round().max(1.0) as u32;
        let (w, h) = if radius.iter().any(|&corner| corner > 0.0) {
            (pixels(wanted.width), pixels(wanted.height))
        } else if self.fit == Fit::Cover {
            let has = ContentFit::Contain.fit(wanted, source);
            let shrink = (has.width / wanted.width)
                .min(has.height / wanted.height)
                .min(1.0);
            (
                pixels(wanted.width * shrink),
                pixels(wanted.height * shrink),
            )
        } else {
            (
                mip(width, pixels(wanted.width)),
                mip(height, pixels(wanted.height)),
            )
        };
        let per = w as f32 / drawn.width.max(1.0);
        let corners = radius
            .map(|radius| (radius * per * 4.0).round().clamp(0.0, f32::from(u16::MAX)) as u16);
        let look = Look {
            width: w,
            height: h,
            corners,
            cover: self.fit == Fit::Cover,
        };
        (drawn, look)
    }
}

/// `size` centred on `bounds`.
pub(super) fn centred(size: Size, bounds: Rectangle) -> Rectangle {
    Rectangle::new(
        bounds.center() - Vector::new(size.width / 2.0, size.height / 2.0),
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
