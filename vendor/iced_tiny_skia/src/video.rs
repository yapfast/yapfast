//! Anomi port (patch 13): video frames the app scales into the window itself.
//!
//! A video frame is a new picture at every frame, made at the size it
//! shows where the app can ([`crate::premultiplied::frame`]): drawn at its
//! own size, the blit copies it in a row at a time with its corners cut,
//! from the app's own bytes. Drawn at another size (a stream larger or
//! smaller than its tile, a camera a pixel or two short of it), stock
//! tiny-skia would scale it through its pattern pipeline, several times the
//! cost of a scaler made for it. The app sets a hook, once, which is offered
//! each opaque frame drawn unturned at another size, at full opacity, and
//! may scale it into the window itself (with libyuv, which the core's
//! libwebrtc archive links in), touching only the part of the window given;
//! anything it declines draws as before. The renderer cuts the corners
//! around what it draws, as the blit cuts a picture's.
//!
//! Yapfast's hook is offered every picture and recognises its frames by
//! their memory, which it registers itself, and takes their bytes as RGBA,
//! swapping red and blue as it scales; here the frames are marked and
//! already premultiplied BGRA, so it's offered only frames, in the frame's
//! own order.

use std::sync::OnceLock;

/// A frame to draw, and where.
pub struct Draw<'a> {
    /// The frame's pixels as its handle holds them, premultiplied BGRA,
    /// opaque, `width * 4` bytes a row.
    pub source: &'a [u8],
    pub width: u32,
    pub height: u32,
    /// Where it's drawn, in the target's pixels; it may reach outside it.
    pub x: i32,
    pub y: i32,
    pub to_width: u32,
    pub to_height: u32,
    /// The part of the target to write: x, y, width and height, inside the
    /// target and the drawn rectangle, never empty.
    pub clip: [u32; 4],
    /// The window's pixels, premultiplied BGRA, `target_width * 4` bytes a
    /// row.
    pub target: &'a mut [u8],
    pub target_width: u32,
}

/// Draws the frame within its clip and says so, or returns false, having
/// written nothing, for the stock path.
pub type Hook = fn(&mut Draw<'_>) -> bool;

static HOOK: OnceLock<Hook> = OnceLock::new();

/// Sets the hook, once; a second call is ignored.
pub fn set_hook(hook: Hook) {
    let _ = HOOK.set(hook);
}

/// Whether the app set a hook.
pub(crate) fn hooked() -> bool {
    HOOK.get().is_some()
}

/// Offers the frame `source`, `width` by `height`, drawn at `drawn` (whole
/// pixels: left, top, right, bottom) and seen in `visible` of `pixels`, to
/// the hook.
pub(crate) fn draw(
    source: &[u8],
    (width, height): (u32, u32),
    drawn: (i32, i32, i32, i32),
    visible: (i32, i32, i32, i32),
    pixels: &mut tiny_skia::PixmapMut<'_>,
) -> bool {
    let Some(hook) = HOOK.get() else {
        return false;
    };
    let (x, y, x_end, y_end) = drawn;
    let (left, top, right, bottom) = visible;
    let target_width = pixels.width();
    hook(&mut Draw {
        source,
        width,
        height,
        x,
        y,
        to_width: (x_end - x) as u32,
        to_height: (y_end - y) as u32,
        clip: [
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        ],
        target: pixels.data_mut(),
        target_width,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blit::{Rounded, whole};
    use crate::core::image::{self as raster, FilterMethod};
    use crate::core::{Radians, Rectangle, Transformation};
    use crate::engine::Engine;
    use crate::premultiplied;
    use std::cell::Cell;
    use std::sync::Arc;

    const FRAME: (u32, u32) = (72, 48);

    thread_local! {
        /// The test hook scales only on a thread that turned it on, so the
        /// other tests of the crate draw as stock.
        static ON: Cell<bool> = const { Cell::new(false) };
        static OFFERED: Cell<usize> = const { Cell::new(0) };
    }

    /// Nearest neighbour, within the clip; it declines a frame 3 wide.
    fn nearest(draw: &mut Draw<'_>) -> bool {
        if !ON.get() || draw.width == 3 {
            return false;
        }
        OFFERED.set(OFFERED.get() + 1);
        let [left, top, width, height] = draw.clip;
        for y in top..top + height {
            for x in left..left + width {
                let from = source_pixel(draw, x as i32 - draw.x, y as i32 - draw.y);
                let to = (y * draw.target_width + x) as usize * 4;
                draw.target[to..to + 4].copy_from_slice(&draw.source[from..from + 4]);
            }
        }
        true
    }

    /// Where the pixel `x`, `y` of the scaled frame comes from.
    fn source_pixel(draw: &Draw<'_>, x: i32, y: i32) -> usize {
        let sx = x as usize * draw.width as usize / draw.to_width as usize;
        let sy = y as usize * draw.height as usize / draw.to_height as usize;
        (sy * draw.width as usize + sx) * 4
    }

    /// A `width` by `height` frame of opaque premultiplied BGRA, each pixel
    /// its own, and the same pixels as straight RGBA.
    fn frame(width: u32, height: u32) -> (Vec<u8>, Vec<u8>) {
        let bgra: Vec<u8> = (0..width * height)
            .flat_map(|i| {
                let v = i.wrapping_add(3).wrapping_mul(2_654_435_761);
                [v as u8, (v >> 8) as u8, (v >> 16) as u8, 255]
            })
            .collect();
        let rgba = bgra
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        (bgra, rgba)
    }

    /// A ground of its own colour at each pixel, as premultiplied BGRA.
    fn ground() -> tiny_skia::Pixmap {
        let mut pixmap = tiny_skia::Pixmap::new(FRAME.0, FRAME.1).expect("a frame");
        for (i, pixel) in pixmap.data_mut().chunks_exact_mut(4).enumerate() {
            let (x, y) = (i as u32 % FRAME.0, i as u32 / FRAME.0);
            pixel.copy_from_slice(&[(x * 3) as u8, (y * 5) as u8, 90, 255]);
        }
        pixmap
    }

    struct Look {
        bounds: Rectangle,
        clip: Rectangle,
        radius: [f32; 4],
        opacity: f32,
        filter: FilterMethod,
        rotation: f32,
        scale: f32,
    }

    impl Look {
        fn at(bounds: Rectangle) -> Self {
            Self {
                bounds,
                clip: bounds,
                radius: [0.0; 4],
                opacity: 1.0,
                filter: FilterMethod::Linear,
                rotation: 0.0,
                scale: 1.0,
            }
        }
    }

    /// The frame after the engine draws `handle` as `look` says, on the
    /// ground, the whole frame its region.
    fn drawn(handle: &raster::Handle, look: &Look) -> Vec<u8> {
        let mut engine = Engine::new();
        let mut pixmap = ground();
        let mut mask = tiny_skia::Mask::new(FRAME.0, FRAME.1).expect("a mask");
        let mut image = raster::Image::new(handle.clone());
        let [top_left, top_right, bottom_right, bottom_left] = look.radius;
        image.border_radius = crate::core::border::Radius {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        };
        image.opacity = look.opacity;
        image.filter_method = look.filter;
        image.rotation = Radians(look.rotation);
        let region = Rectangle::with_size([FRAME.0 as f32, FRAME.1 as f32].into());
        engine.draw_image(
            &crate::graphics::Image::Raster {
                image,
                bounds: look.bounds,
                clip_bounds: look.clip,
            },
            Transformation::scale(look.scale),
            &mut pixmap.as_mut(),
            &mut mask,
            region,
        );
        pixmap.data().to_vec()
    }

    #[test]
    fn a_marked_frame_draws_as_its_stock_picture() {
        let (width, height) = (24, 14);
        let (bgra, rgba) = frame(width, height);
        let marked = premultiplied::frame(width, height, Arc::new(bgra), true);
        let stock = raster::Handle::from_rgba(width, height, rgba);
        let own = Rectangle::new([5.0, 4.0].into(), [24.0, 14.0].into());
        let half = Rectangle::new([5.5, 3.5].into(), [24.0, 14.0].into());
        let larger = Rectangle::new([3.25, 2.5].into(), [37.0, 22.0].into());
        let smaller = Rectangle::new([6.0, 5.0].into(), [17.5, 10.0].into());
        for bounds in [own, half, larger, smaller] {
            for radius in [[0.0; 4], [5.0; 4], [2.0, 7.0, 0.0, 4.5]] {
                for clip in [
                    bounds,
                    Rectangle::new([9.0, 6.5].into(), [12.0, 30.0].into()),
                ] {
                    for scale in [1.0, 2.0] {
                        let look = Look {
                            radius,
                            clip,
                            scale,
                            ..Look::at(bounds)
                        };
                        assert!(
                            drawn(&marked, &look) == drawn(&stock, &look),
                            "{bounds:?} {radius:?} {clip:?} at {scale}x"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_frame_drawn_at_another_size_is_scaled_by_the_hook_and_cut_as_the_blit_cuts() {
        set_hook(nearest);
        ON.set(true);
        let (width, height) = (20, 12);
        let (bgra, _) = frame(width, height);
        let marked = premultiplied::frame(width, height, Arc::new(bgra.clone()), true);
        let larger = Rectangle::new([3.25, 2.5].into(), [33.0, 21.0].into());
        let smaller = Rectangle::new([6.0, 5.0].into(), [15.5, 9.0].into());
        for bounds in [larger, smaller] {
            for radius in [[0.0; 4], [5.0; 4], [2.0, 7.0, 0.0, 4.5]] {
                let cut = Rectangle::new([4.0, 6.5].into(), [20.0, 30.0].into());
                for clip in [bounds, cut] {
                    for scale in [1.0, 2.0] {
                        let look = Look {
                            radius,
                            clip,
                            scale,
                            ..Look::at(bounds)
                        };
                        OFFERED.set(0);
                        let ours = drawn(&marked, &look);
                        assert_eq!(OFFERED.get(), 1, "offered once");

                        // The frame scaled whole, laid by the blit.
                        let (x, y, right, bottom) = whole(bounds * Transformation::scale(scale));
                        let (to_width, to_height) = ((right - x) as u32, (bottom - y) as u32);
                        let probe = Draw {
                            source: &bgra,
                            width,
                            height,
                            x: 0,
                            y: 0,
                            to_width,
                            to_height,
                            clip: [0; 4],
                            target: &mut [],
                            target_width: 0,
                        };
                        let scaled: Vec<u8> = (0..to_height as i32)
                            .flat_map(|y| (0..to_width as i32).map(move |x| (x, y)))
                            .flat_map(|(x, y)| {
                                let from = source_pixel(&probe, x, y);
                                bgra[from..from + 4].to_vec()
                            })
                            .collect();
                        let mut expected = ground();
                        let physical = clip * Transformation::scale(scale);
                        crate::blit::picture(
                            &scaled,
                            to_width,
                            to_height,
                            (x, y),
                            physical,
                            &Rounded::new(whole(physical), radius.map(|r| r * scale)),
                            1.0,
                            true,
                            &mut expected.as_mut(),
                        );
                        assert!(
                            ours == expected.data(),
                            "{bounds:?} {radius:?} {clip:?} at {scale}x"
                        );
                    }
                }
            }
        }
        ON.set(false);
    }

    #[test]
    fn only_opaque_frames_drawn_unturned_at_another_size_are_offered() {
        set_hook(nearest);
        ON.set(true);
        let (bgra, rgba) = frame(20, 12);
        let marked = premultiplied::frame(20, 12, Arc::new(bgra.clone()), true);
        let larger = Rectangle::new([3.0, 2.0].into(), [30.0, 18.0].into());
        let offered = |handle: &raster::Handle, look: Look| {
            OFFERED.set(0);
            let _ = drawn(handle, &look);
            OFFERED.get()
        };
        assert_eq!(offered(&marked, Look::at(larger)), 1);
        let own = Rectangle::new([3.0, 2.0].into(), [20.0, 12.0].into());
        assert_eq!(offered(&marked, Look::at(own)), 0, "its own size: the blit");
        let picture = premultiplied::from_rgba(20, 12, rgba.clone());
        assert_eq!(offered(&picture, Look::at(larger)), 0, "not a frame");
        let clear = premultiplied::frame(20, 12, Arc::new(bgra.clone()), false);
        assert_eq!(offered(&clear, Look::at(larger)), 0, "not opaque");
        for look in [
            Look {
                opacity: 0.5,
                ..Look::at(larger)
            },
            Look {
                filter: FilterMethod::Nearest,
                ..Look::at(larger)
            },
            Look {
                rotation: 0.3,
                ..Look::at(larger)
            },
        ] {
            assert_eq!(offered(&marked, look), 0);
        }

        // Declined, a frame draws as stock.
        let (narrow, narrow_rgba) = frame(3, 12);
        let declined = premultiplied::frame(3, 12, Arc::new(narrow), true);
        let stock = raster::Handle::from_rgba(3, 12, narrow_rgba);
        let look = Look {
            radius: [4.0; 4],
            ..Look::at(larger)
        };
        assert!(drawn(&declined, &look) == drawn(&stock, &look));
        ON.set(false);
    }
}
