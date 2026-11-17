//! A picture's copy at the size and in the shape it's drawn: cut to its
//! box's shape for [`Fit::Cover`], scaled with an area average down and
//! bilinear up, and its corners cleared with a soft edge.
//!
//! tiny-skia ignores an image's border radius at the pinned iced (`TODO:
//! Border radius` in `tiny_skia/src/engine.rs`), so a round avatar is a
//! copy whose corners are transparent. Slint clipped instead (`Rectangle {
//! clip: true; border-radius }` around the `Picture` of
//! `ui/controls.slint:423`). The size a copy is made at (`Shape::look`):
//!
//! - With corners, the physical pixels it's drawn at, even past the
//!   picture's own, so tiny-skia never scales its corners: a 128-pixel
//!   avatar drawn at 32 is a sixteenth of the picture, but a 64-pixel one
//!   drawn at 120 on a 2x screen is 240 pixels square, some fourteen times
//!   the picture's bytes.
//! - Cut to its box, those pixels but never more than the picture has.
//! - Only shrunk, a mip level, which every size within twice of it shares.
//!
//! The first two differ for every pixel the size does, so while a picture's
//! size changes it keeps the copy of the size it held (`Player`). tiny-skia
//! converts the copy for drawing rather than the whole picture. Making one
//! holds the copy and the few source rows its row being made reads
//! (`resample`), however large the picture.

mod resample;

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

/// A copy's size and corners in physical pixels, and whether it's cut to
/// its shape: what tells two copies of one picture apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Look {
    pub width: u32,
    pub height: u32,
    /// The corners' radii in quarter pixels: top left, top right, bottom
    /// right, bottom left.
    pub corners: [u16; 4],
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
    /// one with corners or cut to its box, rather than a mip level.
    pub fn exact(&self) -> bool {
        self.cover || self.corners.iter().any(|&corner| corner > 0)
    }

    /// Whether a copy changes anything about a `width` by `height` picture
    /// drawn this way: corners to clear, a shape to cut, or fewer pixels,
    /// which tiny-skia would scale down unevenly and convert whole. A cut
    /// one near the picture's size draws it as it is; one only shrunk is a
    /// mip level, made only below the picture.
    pub fn needs_copy(&self, width: u32, height: u32) -> bool {
        if self.corners.iter().any(|&corner| corner > 0) {
            return true;
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
    let mut out = if crop == Crop::whole(width, height) && (w, h) == (width, height) {
        rgba[..w as usize * h as usize * 4].to_vec()
    } else {
        resample(rgba, (width, height), crop, w, h)
    };
    clear_corners(&mut out, w, h, look.corners);
    out
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

/// Clears what lies outside each rounded corner, with an edge a pixel soft.
/// Radii past half the shorter side are held to it, as CSS does.
fn clear_corners(rgba: &mut [u8], w: u32, h: u32, corners: [u16; 4]) {
    let most = w.min(h) as f32 / 2.0;
    let (wf, hf) = (w as f32, h as f32);
    for (corner, quarters) in corners.into_iter().enumerate() {
        let radius = (f32::from(quarters) / 4.0).min(most);
        if radius <= 0.0 {
            continue;
        }
        let (cx, cy) = match corner {
            0 => (radius, radius),
            1 => (wf - radius, radius),
            2 => (wf - radius, hf - radius),
            _ => (radius, hf - radius),
        };
        let reach = radius.ceil() as u32;
        let xs = if corner == 0 || corner == 3 {
            0..reach.min(w)
        } else {
            w.saturating_sub(reach)..w
        };
        for y in if corner < 2 {
            0..reach.min(h)
        } else {
            h.saturating_sub(reach)..h
        } {
            for x in xs.clone() {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // Only the side of the centre that faces the corner.
                let outward_x = if corner == 0 || corner == 3 {
                    px < cx
                } else {
                    px > cx
                };
                let outward_y = if corner < 2 { py < cy } else { py > cy };
                if !(outward_x && outward_y) {
                    continue;
                }
                let distance = (px - cx).hypot(py - cy);
                let cover = (radius - distance + 0.5).clamp(0.0, 1.0);
                let at = (y * w + x) as usize * 4 + 3;
                rgba[at] = (f32::from(rgba[at]) * cover).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests;
