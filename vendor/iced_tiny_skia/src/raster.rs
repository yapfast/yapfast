use crate::core::image as raster;
use crate::core::{Rectangle, Size};
use crate::graphics;

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::hash_map;

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

        // Yapfast port: a picture drawn at its own size, unrotated, on whole
        // pixels is copied in with source-over, its corners cut to
        // `border_radius` (which tiny-skia's path ignores). tiny-skia's
        // pattern pipeline costs several times more per pixel. Round 3:
        // partly transparent ones too (faded offline avatars, nameplates).
        if transform.sx == 1.0
            && transform.sy == 1.0
            && transform.kx == 0.0
            && transform.ky == 0.0
            && (bounds.width - image.width() as f32).abs() < 0.01
            && (bounds.height - image.height() as f32).abs() < 0.01
        {
            crate::blit::blit(
                image.data(),
                image.width(),
                image.height(),
                (bounds.x + transform.tx).round() as i32,
                (bounds.y + transform.ty).round() as i32,
                clip_bounds,
                border_radius,
                (opacity.clamp(0.0, 1.0) * 256.0).round() as u32,
                pixels,
            );
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

        let transform = transform.pre_scale(width_scale, height_scale);

        pixels.draw_pixmap(
            (bounds.x / width_scale) as i32,
            (bounds.y / height_scale) as i32,
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
