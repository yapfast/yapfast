use crate::core::image as raster;
use crate::core::{Rectangle, Size};
use crate::graphics;

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::hash_map;

#[derive(Debug)]
pub struct Pipeline {
    cache: RefCell<Cache>,
    opaque: RefCell<Opaque>,
    resized: RefCell<Resized>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            cache: RefCell::new(Cache::default()),
            opaque: RefCell::new(Opaque::default()),
            resized: RefCell::new(Resized::default()),
        }
    }

    pub fn load(
        &self,
        handle: &raster::Handle,
    ) -> Result<raster::Allocation, raster::Error> {
        // Yapfast port: see `native`.
        if let Some(image) = native(handle) {
            #[allow(unsafe_code)]
            return Ok(unsafe {
                raster::allocate(handle, Size::new(image.width(), image.height()))
            });
        }

        let mut cache = self.cache.borrow_mut();
        let image = cache.allocate(handle)?;

        #[allow(unsafe_code)]
        Ok(unsafe {
            raster::allocate(handle, Size::new(image.width(), image.height()))
        })
    }

    pub fn dimensions(&self, handle: &raster::Handle) -> Option<Size<u32>> {
        if let Some(image) = native(handle) {
            return Some(Size::new(image.width(), image.height()));
        }

        let mut cache = self.cache.borrow_mut();
        let image = cache.allocate(handle).ok()?;

        Some(Size::new(image.width(), image.height()))
    }

    pub fn draw(
        &mut self,
        handle: &raster::Handle,
        filter_method: raster::FilterMethod,
        bounds: Rectangle,
        opacity: f32,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        transform: tiny_skia::Transform,
        clip_mask: Option<&tiny_skia::Mask>,
        clip_bounds: Rectangle,
        corners: Rectangle,
        border_radius: [f32; 4],
    ) {
        let mut cache = self.cache.borrow_mut();

        let image = match native(handle) {
            Some(image) => image,
            None => match cache.allocate(handle) {
                Ok(image) => image,
                Err(_) => return,
            },
        };

        // Yapfast port (patch 13): a video frame the app draws itself,
        // copied or scaled into the frame inside the clip, at any scale
        // factor; see video.rs.
        if transform.kx == 0.0
            && transform.ky == 0.0
            && transform.sx > 0.0
            && transform.sy > 0.0
            && crate::video::draw(
                image.data(),
                image.width(),
                image.height(),
                Rectangle {
                    x: bounds.x * transform.sx + transform.tx,
                    y: bounds.y * transform.sy + transform.ty,
                    width: bounds.width * transform.sx,
                    height: bounds.height * transform.sy,
                },
                clip_bounds,
                pixels,
            )
        {
            return;
        }

        // Yapfast port: a picture drawn unrotated is put on whole pixels and
        // copied in with source-over, its corners cut to `border_radius`
        // (which tiny-skia's path ignores): at its own size as it is, else
        // scaled to the pixels it covers (patch 24), as at a zoom other
        // than 100%. tiny-skia's pattern pipeline costs several times more
        // per pixel. Round 3: partly transparent ones too (faded offline
        // avatars, nameplates).
        if transform.kx == 0.0
            && transform.ky == 0.0
            && transform.sx > 0.0
            && transform.sy > 0.0
        {
            // Patch 19: whether it's opaque is found once, not row by row
            // for every frame it's drawn in.
            let opaque = self.opaque.borrow_mut().of(handle.id(), image.data());
            let (left, top, right, bottom) = whole_span(bounds, transform);
            let opacity = (opacity.clamp(0.0, 1.0) * 256.0).round() as u32;
            // The engine hands the radius over in physical pixels already
            // (scaled by the window's scale factor), as the turned path
            // below takes it; scaling it again made an 8 px corner 18 px at
            // 150% (patch 39).
            let radius = border_radius;
            // Patch 38: round at the picture's own clip, as iced_wgpu's
            // image shader does, so a picture larger than its box (art
            // laid as a cover) takes the box's round corners.
            // Held to a frame's reach: a clip may be as wide as can be.
            let corners = border_radius.iter().any(|radius| *radius > 0.0).then(|| {
                let pixel = |at: f32| at.round().clamp(-16_777_216.0, 16_777_216.0) as i32;
                let left = pixel(corners.x * transform.sx + transform.tx);
                let top = pixel(corners.y * transform.sy + transform.ty);
                // Its corner on the nearest pixel and its size rounded, as
                // `whole_span` places the picture, so a box the picture's
                // own size cuts exactly the pixels it covers (patch 39).
                [
                    left,
                    top,
                    left.saturating_add(pixel(corners.width * transform.sx)),
                    top.saturating_add(pixel(corners.height * transform.sy)),
                ]
            });
            if (right - left, bottom - top) == (image.width() as i32, image.height() as i32) {
                crate::blit::blit(
                    image.data(),
                    image.width(),
                    image.height(),
                    left,
                    top,
                    clip_bounds,
                    corners,
                    radius,
                    opacity,
                    opaque,
                    pixels,
                );
            } else if right > left && bottom > top {
                let (width, height) = ((right - left) as u32, (bottom - top) as u32);
                let mut resized = self.resized.borrow_mut();
                let copy = resized.of(
                    (
                        handle.id(),
                        width,
                        height,
                        filter_method == raster::FilterMethod::Nearest,
                    ),
                    image,
                );
                crate::blit::blit(
                    copy,
                    width,
                    height,
                    left,
                    top,
                    clip_bounds,
                    corners,
                    radius,
                    opacity,
                    opaque,
                    pixels,
                );
            }
            return;
        }

        let width_scale = bounds.width / image.width() as f32;
        let height_scale = bounds.height / image.height() as f32;

        let quality = match filter_method {
            raster::FilterMethod::Linear => tiny_skia::FilterQuality::Bilinear,
            raster::FilterMethod::Nearest => tiny_skia::FilterQuality::Nearest,
        };

        // Yapfast port (patch 10): a scaled picture with round corners
        // (any scale factor but 1, as 125% and 150% on Windows) is filled
        // as its rounded rectangle with the picture as the pattern. Stock,
        // `draw_pixmap` fills the whole rectangle: square avatars.
        if border_radius.iter().any(|radius| *radius > 0.0) {
            let scale = transform.sx.abs().max(f32::EPSILON);
            let path = crate::engine::rounded_rectangle(
                bounds,
                border_radius.map(|radius| radius / scale),
            );
            let pattern = tiny_skia::Pattern::new(
                image,
                tiny_skia::SpreadMode::Pad,
                quality,
                opacity,
                tiny_skia::Transform::from_row(
                    width_scale,
                    0.0,
                    0.0,
                    height_scale,
                    bounds.x,
                    bounds.y,
                ),
            );
            pixels.fill_path(
                &path,
                &tiny_skia::Paint {
                    shader: pattern,
                    anti_alias: true,
                    ..Default::default()
                },
                tiny_skia::FillRule::Winding,
                transform,
                clip_mask,
            );
            return;
        }

        // Yapfast port (patch 36): placed at the bounds' own position, then
        // scaled.
        // Stock, the position was divided by the scale and cut to whole
        // pixels of the picture, so a small picture stretched far (a
        // turned card's ground) landed up to a scaled pixel away.
        let transform = transform
            .pre_translate(bounds.x, bounds.y)
            .pre_scale(width_scale, height_scale);

        pixels.draw_pixmap(
            0,
            0,
            image,
            &tiny_skia::PixmapPaint {
                quality,
                opacity,
                ..Default::default()
            },
            transform,
            clip_mask,
        );
    }

    /// After a frame: `shown` are the pictures in its layers, each by its
    /// handle and the size it's drawn at ([`shown_at`]).
    pub fn trim_cache(&mut self, shown: &FxHashSet<Shown>) {
        self.cache.borrow_mut().trim();
        self.opaque.borrow_mut().trim();
        self.resized.borrow_mut().trim(shown);
    }
}

/// Yapfast port: an `Rgba` handle's pixels are taken to be in tiny-skia's
/// own format already, premultiplied BGRA as this renderer stores them, and
/// drawn from the handle's bytes: no converted copy in the cache, nothing to
/// convert again after a frame whose damage missed the image (the cache
/// keeps only images drawn in the last frame). Only the Yapfast port makes
/// `Rgba` handles, and it makes them in this format (`pictures::handle`,
/// `pictures::shared`, its fitter's copies, video frames). A handle of
/// straight RGBA bytes made anywhere else would draw with red and blue
/// swapped and nothing would say so.
fn native(handle: &raster::Handle) -> Option<tiny_skia::PixmapRef<'_>> {
    match handle {
        raster::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => tiny_skia::PixmapRef::from_bytes(pixels, *width, *height),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct Cache {
    entries: FxHashMap<raster::Id, Option<Entry>>,
    hits: FxHashSet<raster::Id>,
}

/// Yapfast port (patch 19): whether each picture drawn in the last frame or
/// this one is opaque all over, and those drawn in this one.
#[derive(Debug, Default)]
struct Opaque {
    known: FxHashMap<raster::Id, bool>,
    hits: FxHashSet<raster::Id>,
}

impl Opaque {
    /// Whether the picture `id`, of premultiplied `pixels`, is opaque all
    /// over: looked at once while it's drawn frame after frame.
    fn of(&mut self, id: raster::Id, pixels: &[u8]) -> bool {
        let _ = self.hits.insert(id);
        *self
            .known
            .entry(id)
            .or_insert_with(|| pixels.chunks_exact(4).all(|pixel| pixel[3] == 255))
    }

    fn trim(&mut self) {
        self.known.retain(|key, _| self.hits.contains(key));
        self.hits.clear();
    }
}

/// Yapfast port (patch 24): pictures drawn at a size other than their own
/// (all of them at a zoom other than 100%), each scaled once to the whole
/// pixels it covers and copied in from then on, kept while it's in the
/// frame's layers at that size. Scaling them through tiny-skia's pattern pipeline on every
/// frame made a scrolled frame of a picture-heavy channel at 110% cost
/// 17 ms rather than 1. A picture drawn at its own size has no copy.
#[derive(Debug, Default)]
struct Resized {
    copies: FxHashMap<Shown, Vec<u8>>,
    hits: FxHashSet<Shown>,
}

/// Yapfast port (patch 24): a picture as drawn: its handle, the whole
/// pixels it covers across and down, and whether it's sampled nearest.
pub type Shown = (raster::Id, u32, u32, bool);

/// The whole pixels (left, top, right, bottom) a picture at `bounds`
/// covers under `transform` (unrotated): its corner on the nearest pixel,
/// its size rounded. Its size doesn't change with where it is, so a list
/// scrolled by fractions of a pixel (any at 110%) neither scales it anew
/// nor has its edges flicker by a pixel.
fn whole_span(bounds: Rectangle, transform: tiny_skia::Transform) -> (i32, i32, i32, i32) {
    let left = (bounds.x * transform.sx + transform.tx).round() as i32;
    let top = (bounds.y * transform.sy + transform.ty).round() as i32;
    (
        left,
        top,
        left + (bounds.width * transform.sx).round() as i32,
        top + (bounds.height * transform.sy).round() as i32,
    )
}

/// Yapfast port (patch 24): a picture at `bounds` in a frame at `scale`, as
/// [`Resized`] keeps its copy.
pub fn shown_at(image: &raster::Image, bounds: Rectangle, scale: f32) -> Shown {
    let (left, top, right, bottom) =
        whole_span(bounds, tiny_skia::Transform::from_scale(scale, scale));
    (
        image.handle.id(),
        (right - left).max(0) as u32,
        (bottom - top).max(0) as u32,
        image.filter_method == raster::FilterMethod::Nearest,
    )
}

impl Resized {
    /// `image`'s pixels as `shown`, scaled the first time.
    fn of(&mut self, shown: Shown, image: tiny_skia::PixmapRef<'_>) -> &[u8] {
        let _ = self.hits.insert(shown);
        let (_, width, height, nearest) = shown;
        self.copies.entry(shown).or_insert_with(|| {
            crate::blit::resized(
                image.data(),
                image.width(),
                image.height(),
                width,
                height,
                nearest,
            )
        })
    }

    /// Lets go of the copies of pictures the frame doesn't show as they
    /// were made: drawn in it, or in its layers outside what it drew (a
    /// frame that redraws a moving GIF alone keeps the copies round it).
    fn trim(&mut self, shown: &FxHashSet<Shown>) {
        self.copies
            .retain(|key, _| self.hits.contains(key) || shown.contains(key));
        self.hits.clear();
    }
}

impl Cache {
    pub fn allocate(
        &mut self,
        handle: &raster::Handle,
    ) -> Result<tiny_skia::PixmapRef<'_>, raster::Error> {
        let id = handle.id();

        if let hash_map::Entry::Vacant(entry) = self.entries.entry(id) {
            let image = match graphics::image::load(handle) {
                Ok(image) => image,
                Err(error) => {
                    let _ = entry.insert(None);

                    return Err(error);
                }
            };

            if image.width() == 0 || image.height() == 0 {
                return Err(raster::Error::Empty);
            }

            let mut buffer =
                vec![0u32; image.width() as usize * image.height() as usize];

            for (i, pixel) in image.pixels().enumerate() {
                let [r, g, b, a] = pixel.0;

                buffer[i] = bytemuck::cast(
                    tiny_skia::ColorU8::from_rgba(b, g, r, a).premultiply(),
                );
            }

            let _ = entry.insert(Some(Entry {
                width: image.width(),
                height: image.height(),
                pixels: buffer,
            }));
        }

        let _ = self.hits.insert(id);

        Ok(self
            .entries
            .get(&id)
            .unwrap()
            .as_ref()
            .map(|entry| {
                tiny_skia::PixmapRef::from_bytes(
                    bytemuck::cast_slice(&entry.pixels),
                    entry.width,
                    entry.height,
                )
                .expect("Build pixmap from image bytes")
            })
            .expect("Image should be allocated"))
    }

    fn trim(&mut self) {
        self.entries.retain(|key, _| self.hits.contains(key));
        self.hits.clear();
    }
}

#[derive(Debug)]
struct Entry {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}
