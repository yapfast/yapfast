use crate::core::image as raster;
use crate::core::{Rectangle, Size};
use crate::graphics;

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::hash_map;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Anomi port: the bytes of the converted copies every raster cache in the
/// process holds, for measuring; see [`converted_bytes`].
static CONVERTED: AtomicUsize = AtomicUsize::new(0);

/// The bytes of the premultiplied copies of pictures that the raster caches
/// of every renderer in the process hold now. A picture drawn from its own
/// bytes (`premultiplied::from_rgba`) has none.
pub fn converted_bytes() -> usize {
    CONVERTED.load(Ordering::Relaxed)
}

#[derive(Debug)]
pub struct Pipeline {
    cache: RefCell<Cache>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            cache: RefCell::new(Cache::default()),
        }
    }

    pub fn load(&self, handle: &raster::Handle) -> Result<raster::Allocation, raster::Error> {
        // Anomi port (patch 1): see `native`.
        if let Some(image) = native(handle) {
            #[allow(unsafe_code)]
            return Ok(unsafe {
                raster::allocate(handle, Size::new(image.width(), image.height()))
            });
        }

        let mut cache = self.cache.borrow_mut();
        let image = cache.allocate(handle)?;

        #[allow(unsafe_code)]
        Ok(unsafe { raster::allocate(handle, Size::new(image.width(), image.height())) })
    }

    pub fn dimensions(&self, handle: &raster::Handle) -> Option<Size<u32>> {
        if let Some(image) = native(handle) {
            return Some(Size::new(image.width(), image.height()));
        }

        let mut cache = self.cache.borrow_mut();
        let image = cache.allocate(handle).ok()?;

        Some(Size::new(image.width(), image.height()))
    }

    /// Anomi port (patches 2, 6, 7, 19): draws `handle`, unrotated at
    /// `bounds` (physical), by copying its pixels onto the frame when it's
    /// drawn at its own size on whole pixels ([`blit::whole`]), within
    /// `clip_bounds` and cut to `rounded`; false when it's drawn scaled,
    /// which [`Pipeline::draw`] then draws. tiny-skia drew every picture
    /// through its pattern pipeline in f32 with the window's clip mask, and
    /// ignored the corners (`TODO: Border radius`), so `anomi_widgets`
    /// cleared them in its copies. A picture is put on whole pixels as a
    /// clip's edges are, rounded: stock truncated its corner, so a picture
    /// at half a pixel lost its top row to its own clip.
    ///
    /// [`blit::whole`]: crate::blit::whole
    pub fn blit(
        &mut self,
        handle: &raster::Handle,
        bounds: Rectangle,
        clip_bounds: Rectangle,
        rounded: &crate::blit::Rounded,
        opacity: f32,
        pixels: &mut tiny_skia::PixmapMut<'_>,
    ) -> bool {
        let mut cache = self.cache.borrow_mut();
        let Some((image, opaque)) = picture(&mut cache, handle) else {
            return true;
        };
        let (left, top, right, bottom) = crate::blit::whole(bounds);
        if (right - left, bottom - top) != (image.width() as i32, image.height() as i32) {
            return false;
        }
        crate::blit::picture(
            image.data(),
            image.width(),
            image.height(),
            (left, top),
            clip_bounds,
            rounded,
            opacity,
            opaque,
            pixels,
        );
        true
    }

    /// Draws `handle` scaled to `bounds` through tiny-skia, as stock.
    /// Anomi port (patch 10): with `rounded`, the picture's own clip and its
    /// corners (physical), it fills that rounded rectangle, anti-aliased,
    /// with the picture as its pattern, where stock filled the picture's
    /// whole rectangle. The clip mask should then hold no more than the
    /// picture's rectangle, which the pattern's padding would smear past.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        handle: &raster::Handle,
        filter_method: raster::FilterMethod,
        bounds: Rectangle,
        opacity: f32,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        transform: tiny_skia::Transform,
        clip_mask: Option<&tiny_skia::Mask>,
        rounded: Option<(Rectangle, [f32; 4])>,
    ) {
        let mut cache = self.cache.borrow_mut();

        let Some((image, _)) = picture(&mut cache, handle) else {
            return;
        };

        let width_scale = bounds.width / image.width() as f32;
        let height_scale = bounds.height / image.height() as f32;

        // Anomi port: the picture's corner at its own, where stock drew it
        // at `(bounds.x / width_scale) as i32` in the picture's pixels: up
        // to one of them (scaled) up and to the left, which a clip at its
        // rectangle then cut off.
        let transform = transform
            .pre_translate(bounds.x, bounds.y)
            .pre_scale(width_scale, height_scale);

        let quality = match filter_method {
            raster::FilterMethod::Linear => tiny_skia::FilterQuality::Bilinear,
            raster::FilterMethod::Nearest => tiny_skia::FilterQuality::Nearest,
        };

        if let Some((clip, radius)) = rounded {
            // What `draw_pixmap` fills its rectangle with, laid on the
            // frame's pixels.
            let pattern = tiny_skia::Pattern::new(
                image,
                tiny_skia::SpreadMode::Pad,
                quality,
                opacity,
                transform,
            );
            let most = clip.width.min(clip.height) / 2.0;
            pixels.fill_path(
                &crate::engine::rounded_rectangle(clip, radius.map(|radius| radius.min(most))),
                &tiny_skia::Paint {
                    shader: pattern,
                    anti_alias: true,
                    ..tiny_skia::Paint::default()
                },
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                clip_mask,
            );
            return;
        }

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

    pub fn trim_cache(&mut self) {
        self.cache.borrow_mut().trim();
    }
}

/// Anomi port (patch 1): a picture whose bytes are a buffer of
/// `premultiplied::from_rgba`, already premultiplied BGRA as tiny-skia
/// stores pixels, drawn from the handle's own bytes: no converted copy in
/// the cache, and nothing to convert again after a frame whose damage
/// missed it (the cache keeps only pictures drawn in the last frame). The
/// handle borrowed here keeps the bytes alive and unchanged; see
/// `premultiplied` for why the address can be trusted. Any other `Rgba`
/// handle is straight RGBA and goes through the cache as in stock. Yapfast
/// takes every `Rgba` handle to be premultiplied BGRA; ours aren't (the
/// core's pictures, the fixtures).
fn native(handle: &raster::Handle) -> Option<tiny_skia::PixmapRef<'_>> {
    native_opaque(handle).map(|(image, _)| image)
}

/// [`native`], and whether every pixel of it is opaque (patch 19).
fn native_opaque(handle: &raster::Handle) -> Option<(tiny_skia::PixmapRef<'_>, bool)> {
    match handle {
        raster::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => {
            let opaque = crate::premultiplied::opaque(pixels)?;
            Some((
                tiny_skia::PixmapRef::from_bytes(pixels, *width, *height)?,
                opaque,
            ))
        }
        _ => None,
    }
}

/// `handle`'s pixels as drawn, from its own bytes or the cache's copy, and
/// whether every one is opaque; none when it can't be read.
fn picture<'a>(
    cache: &'a mut Cache,
    handle: &'a raster::Handle,
) -> Option<(tiny_skia::PixmapRef<'a>, bool)> {
    match native_opaque(handle) {
        Some(image) => Some(image),
        None => cache.allocate_opaque(handle).ok(),
    }
}

#[derive(Debug, Default)]
struct Cache {
    entries: FxHashMap<raster::Id, Option<Entry>>,
    hits: FxHashSet<raster::Id>,
    /// Anomi port: what its entries' pixels take, in bytes.
    bytes: usize,
}

impl Cache {
    pub fn allocate(
        &mut self,
        handle: &raster::Handle,
    ) -> Result<tiny_skia::PixmapRef<'_>, raster::Error> {
        self.allocate_opaque(handle).map(|(image, _)| image)
    }

    /// [`Cache::allocate`], and whether every pixel of the copy is opaque,
    /// found as it's converted (patch 19).
    fn allocate_opaque(
        &mut self,
        handle: &raster::Handle,
    ) -> Result<(tiny_skia::PixmapRef<'_>, bool), raster::Error> {
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

            let mut buffer = vec![0u32; image.width() as usize * image.height() as usize];
            let mut opaque = true;

            for (i, pixel) in image.pixels().enumerate() {
                opaque &= pixel.0[3] == u8::MAX;
                // Anomi port (patch 1): the conversion `premultiplied` makes
                // in place, so the two draw the same pixels.
                buffer[i] = bytemuck::cast(crate::premultiplied::premultiply(pixel.0));
            }

            let bytes = buffer.len() * 4;
            self.bytes += bytes;
            let _ = CONVERTED.fetch_add(bytes, Ordering::Relaxed);

            let _ = entry.insert(Some(Entry {
                width: image.width(),
                height: image.height(),
                pixels: buffer,
                opaque,
            }));
        }

        let _ = self.hits.insert(id);

        Ok(self
            .entries
            .get(&id)
            .unwrap()
            .as_ref()
            .map(|entry| {
                let image = tiny_skia::PixmapRef::from_bytes(
                    bytemuck::cast_slice(&entry.pixels),
                    entry.width,
                    entry.height,
                )
                .expect("Build pixmap from image bytes");
                (image, entry.opaque)
            })
            .expect("Image should be allocated"))
    }

    fn trim(&mut self) {
        let before = self.bytes;
        self.entries.retain(|key, entry| {
            let kept = self.hits.contains(key);
            if !kept && let Some(entry) = entry {
                self.bytes -= entry.pixels.len() * 4;
            }
            kept
        });
        let _ = CONVERTED.fetch_sub(before - self.bytes, Ordering::Relaxed);
        self.hits.clear();
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        let _ = CONVERTED.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct Entry {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    /// Patch 19: every pixel is opaque.
    opaque: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::Clip;
    use crate::premultiplied;

    const FRAME: (u32, u32) = (64, 48);

    /// A `width` by `height` picture of straight RGBA, each pixel its own,
    /// opaque, clear and partly transparent ones among them.
    fn picture(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                let v = i.wrapping_add(7).wrapping_mul(2_654_435_761);
                let alpha = match i % 4 {
                    0 => 255,
                    1 => 0,
                    _ => (v >> 24) as u8,
                };
                [v as u8, (v >> 8) as u8, (v >> 16) as u8, alpha]
            })
            .collect()
    }

    struct Draw {
        bounds: Rectangle,
        opacity: f32,
        transform: tiny_skia::Transform,
        clip: Option<Rectangle>,
        filter: raster::FilterMethod,
    }

    /// The frame after `handle` is drawn on a ground as `draw` says, through
    /// a pipeline of its own, and how many copies its cache made.
    fn drawn(handle: &raster::Handle, draw: &Draw) -> (Vec<u8>, usize) {
        let mut pipeline = Pipeline::new();
        let mut frame = tiny_skia::Pixmap::new(FRAME.0, FRAME.1).expect("a frame");
        frame.fill(tiny_skia::Color::from_rgba8(30, 60, 90, 255));
        let mut mask = tiny_skia::Mask::new(FRAME.0, FRAME.1).expect("a mask");
        let mut clip = Clip::default();
        if let Some(bounds) = draw.clip {
            clip.set(&mut mask, bounds);
        }
        pipeline.draw(
            handle,
            draw.filter,
            draw.bounds,
            draw.opacity,
            &mut frame.as_mut(),
            draw.transform,
            draw.clip.map(|_| &mask),
            None,
        );
        let copies = pipeline.cache.borrow().entries.len();
        (frame.data().to_vec(), copies)
    }

    #[test]
    fn a_premultiplied_picture_draws_as_its_stock_copy() {
        let clips = [
            None,
            Some(Rectangle::new([5.5, 3.0].into(), [20.3, 11.0].into())),
        ];
        let transforms = [
            tiny_skia::Transform::identity(),
            tiny_skia::Transform::from_scale(1.25, 1.25),
            tiny_skia::Transform::from_scale(2.0, 2.0),
            tiny_skia::Transform::from_rotate_at(30.0, 12.0, 10.0),
        ];
        for (width, height) in [(1, 1), (7, 5), (16, 16), (33, 17)] {
            let rgba = picture(width, height);
            let marked = premultiplied::from_rgba(width, height, rgba.clone());
            let stock = raster::Handle::from_rgba(width, height, rgba.clone());
            let own = Rectangle::new([3.0, 2.0].into(), [width as f32, height as f32].into());
            let scaled = Rectangle::new(
                [2.5, 1.5].into(),
                [width as f32 * 1.7, height as f32 * 0.6].into(),
            );
            for bounds in [own, scaled] {
                for opacity in [1.0, 0.5, 0.731] {
                    for transform in transforms {
                        for clip in clips {
                            for filter in
                                [raster::FilterMethod::Linear, raster::FilterMethod::Nearest]
                            {
                                let draw = Draw {
                                    bounds,
                                    opacity,
                                    transform,
                                    clip,
                                    filter,
                                };
                                let (ours, copies) = drawn(&marked, &draw);
                                let (theirs, stock_copies) = drawn(&stock, &draw);
                                assert!(
                                    ours == theirs,
                                    "{width}x{height} at {bounds:?}, opacity {opacity}, \
                                     {transform:?}, clip {clip:?}, {filter:?}"
                                );
                                assert_eq!((copies, stock_copies), (0, 1), "copied once, by stock");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_picture_not_marked_is_converted_as_stock_and_left_alone() {
        let (width, height) = (7, 5);
        let rgba = picture(width, height);
        let handle = raster::Handle::from_rgba(width, height, rgba.clone());
        let pipeline = Pipeline::new();
        let mut cache = pipeline.cache.borrow_mut();
        let copy = cache.allocate(&handle).expect("a copy").data().to_vec();
        // iced's own conversion, as it was written before patch 1.
        let stock: Vec<u8> = rgba
            .chunks_exact(4)
            .flat_map(|pixel| {
                let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
                bytemuck::cast::<_, [u8; 4]>(
                    tiny_skia::ColorU8::from_rgba(b, g, r, a).premultiply(),
                )
            })
            .collect();
        assert_eq!(copy, stock);
        let raster::Handle::Rgba { pixels, .. } = &handle else {
            panic!("pixels");
        };
        assert_eq!(&pixels[..], &rgba[..], "its own bytes untouched");
        assert!(!premultiplied::is_premultiplied(&handle));
        assert!(native(&handle).is_none());

        // Its copy counts while it's drawn, and goes after a frame without it.
        assert_eq!(cache.bytes, (width * height * 4) as usize);
        cache.trim();
        assert_eq!(
            cache.bytes,
            (width * height * 4) as usize,
            "drawn in the last frame"
        );
        cache.trim();
        assert_eq!((cache.bytes, cache.entries.len()), (0, 0));
    }
}
