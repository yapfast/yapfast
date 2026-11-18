//! A picture's copy at the size and in the shape it's drawn: cut to its
//! box's shape for [`Fit::Cover`], scaled with an area average down and
//! bilinear up.
//!
//! Its corners aren't in it: the renderer cuts them where it's drawn
//! (`vendor/iced_tiny_skia`, patches 2, 7 and 10), as Slint clipped
//! (`Rectangle { clip: true; border-radius }` around the `Picture` of
//! `ui/controls.slint:423`), so a round and a square use of one size share
//! a copy. The size a copy is made at (`Shape::look`):
//!
//! - With corners, the physical pixels it's drawn at, even past the
//!   picture's own, so the renderer copies it in on whole pixels and cuts
//!   its corners there rather than scaling it through tiny-skia at each
//!   frame: a 128-pixel avatar drawn at 32 is a sixteenth of the picture,
//!   but a 64-pixel one drawn at 120 on a 2x screen is 240 pixels square,
//!   some fourteen times the picture's bytes.
//! - Cut to its box, those pixels but never more than the picture has.
//! - Only shrunk, a mip level, which every size within twice of it shares.
//!
//! The first two differ for every pixel the size does, so while a picture's
//! size changes it keeps the copy of the size it held (`Player`). Making one
//! holds the copy and the few source rows its row being made reads
//! (`resample`), however large the picture.
//!
//! [`handle`] hands a copy to the renderer in its own format, premultiplied
//! BGRA, which it draws from the copy's bytes (`vendor/iced_tiny_skia`,
//! patch 1), so a copy on screen is held once; stock, tiny-skia kept a
//! converted copy of it while it showed and converted it again after a
//! frame that didn't draw it. A copy is made from straight RGBA only: the
//! core's pictures, the fixtures, a spoiler's blur.

mod resample;

use iced::widget::image;
use resample::resample;

/// How a picture fills the box it's drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Stretched to the box, as Slint's `image-fit: fill`.
    #[default]
    Fill,
    /// Cut to the box's shape and filling it, as a banner.
    Cover,
    /// Whole and as large as fits, centred in the box.
    Contain,
    /// As `Cover`, but cut from the picture's top rather than its middle: a
    /// profile effect reaching past a card's foot is cut there, so its
    /// corners round with the card's.
    Top,
}

impl Fit {
    /// It's cut to the box's shape.
    pub fn cuts(self) -> bool {
        matches!(self, Self::Cover | Self::Top)
    }
}

/// A copy's size in physical pixels and whether it's cut to its shape:
/// what tells two copies of one picture apart. Its corners don't: the
/// renderer cuts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Look {
    pub width: u32,
    pub height: u32,
    /// Cut to the copy's shape around the middle rather than stretched.
    pub cover: bool,
    /// With `cover`, cut from the top rather than around the middle.
    pub top: bool,
}

impl Look {
    /// The bytes a copy of this look takes.
    pub fn bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// Made at the very pixels drawn, so each size is a copy of its own:
    /// one cut to its box, or one `rounded` (its corners cut where it's
    /// drawn), rather than a mip level.
    pub fn exact(&self, rounded: bool) -> bool {
        self.cover || rounded
    }

    /// Whether a copy changes anything about a `width` by `height` picture
    /// drawn this way: a shape to cut, or fewer pixels, which tiny-skia
    /// would scale down unevenly and convert whole; `rounded`, any other
    /// number of pixels, so the renderer copies it in on whole pixels with
    /// its corners cut rather than scaling it at each frame. A cut one near
    /// the picture's size draws it as it is; one only shrunk is a mip
    /// level, made only below the picture.
    pub fn needs_copy(&self, width: u32, height: u32, rounded: bool) -> bool {
        if rounded {
            return (self.width, self.height) != (width, height);
        }
        if !self.cover {
            return self.width < width || self.height < height;
        }
        let source = u64::from(width) * u64::from(height);
        let drawn = u64::from(self.width) * u64::from(self.height);
        u64::from(width) * u64::from(self.height) != u64::from(height) * u64::from(self.width)
            || source >= 2 * drawn
    }
}

/// `rgba`, a `width` by `height` picture, as `look` draws it. A picture
/// with fewer bytes than its size needs comes out clear.
pub(crate) fn make(rgba: &[u8], width: u32, height: u32, look: &Look) -> Vec<u8> {
    let (w, h) = (look.width.max(1), look.height.max(1));
    if width == 0 || height == 0 || rgba.len() < width as usize * height as usize * 4 {
        return vec![0; w as usize * h as usize * 4];
    }
    let crop = if look.cover {
        cover(width, height, w, h, look.top)
    } else {
        Crop::whole(width, height)
    };
    if crop == Crop::whole(width, height) && (w, h) == (width, height) {
        rgba[..w as usize * h as usize * 4].to_vec()
    } else {
        resample(rgba, (width, height), crop, w, h)
    }
}

/// A copy [`make`] made for `look`, as a handle drawn from these very
/// bytes: they're turned in place into the renderer's premultiplied BGRA,
/// as it would have converted them, so it draws the same pixels. The handle
/// owns them, and they can't change while it, or the renderer drawing it,
/// lives (`iced_tiny_skia::premultiplied`). What draws it is the only
/// reader: no copy is made of it (`player::copy`).
pub(crate) fn handle(look: &Look, pixels: Vec<u8>) -> image::Handle {
    iced_tiny_skia::premultiplied::from_rgba(look.width, look.height, pixels)
}

/// The part of a picture a copy shows, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Crop {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Crop {
    fn whole(width: u32, height: u32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        }
    }
}

/// The middle of a `width` by `height` picture in the shape of `w` by `h`,
/// or its top when `top`.
fn cover(width: u32, height: u32, w: u32, h: u32, top: bool) -> Crop {
    let (width, height) = (width as f32, height as f32);
    let shape = w as f32 / h as f32;
    if width / height > shape {
        let cut = height * shape;
        Crop {
            x: (width - cut) / 2.0,
            y: 0.0,
            width: cut,
            height,
        }
    } else {
        let cut = width / shape;
        Crop {
            x: 0.0,
            y: if top { 0.0 } else { (height - cut) / 2.0 },
            width,
            height: cut,
        }
    }
}

#[cfg(test)]
mod tests;
