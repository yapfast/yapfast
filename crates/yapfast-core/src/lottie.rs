//! Bounded, CPU-rendered Lottie stickers, decoded one frame at a time.

use std::{marker::PhantomData, ptr, sync::OnceLock};
use thorvg_sys as tvg;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_FRAMES: usize = 3600;

#[derive(serde::Deserialize)]
struct Header {
    w: u32,
    h: u32,
    fr: f64,
    ip: f64,
    op: f64,
}

pub(crate) struct Frames {
    pub width: u32,
    pub height: u32,
    pub delays: Vec<u32>,
    canvas: tvg::Tvg_Canvas,
    animation: tvg::Tvg_Animation,
    pixels: Vec<u32>,
    index: usize,
    // Native objects stay on the decoding thread that created them.
    _thread: PhantomData<*mut ()>,
}

fn success(result: tvg::Tvg_Result) -> Option<()> {
    (result == tvg::Tvg_Result::TVG_RESULT_SUCCESS).then_some(())
}

impl Frames {
    pub fn new(data: &[u8], width: u32, height: u32) -> Option<Self> {
        if data.len() > MAX_BYTES || width == 0 || height == 0 {
            return None;
        }
        let header: Header = serde_json::from_slice(data).ok()?;
        let count = header.op - header.ip;
        if header.w == 0
            || header.h == 0
            || header.w > 8192
            || header.h > 8192
            || !header.fr.is_finite()
            || !(1.0..=120.0).contains(&header.fr)
            || !count.is_finite()
            || !(1.0..=MAX_FRAMES as f64).contains(&count)
            || count / header.fr > 60.0
        {
            return None;
        }
        // The engine is shared for the process lifetime, like the image codecs.
        // No worker pool: decoding already runs off the UI thread.
        static ENGINE: OnceLock<bool> = OnceLock::new();
        if !ENGINE.get_or_init(|| {
            // SAFETY: OnceLock serializes the one initialization. The threads
            // feature provides locks for concurrent independent decoders.
            unsafe { success(tvg::tvg_engine_init(0)).is_some() }
        }) {
            return None;
        }
        let (width, height) =
            crate::picture::fit(header.w, header.h, (width.min(160), height.min(160)));
        let mut frames = Self {
            width,
            height,
            delays: Vec::new(),
            canvas: ptr::null_mut(),
            animation: ptr::null_mut(),
            pixels: vec![0; width as usize * height as usize],
            index: 0,
            _thread: PhantomData,
        };
        // SAFETY: All handles are checked before use and owned by `frames`.
        // ThorVG copies the JSON; file I/O and expressions are disabled. The
        // target allocation stays fixed until after the canvas is destroyed.
        unsafe {
            frames.animation = tvg::tvg_animation_new();
            if frames.animation.is_null() {
                return None;
            }
            let picture = tvg::tvg_animation_get_picture(frames.animation);
            if picture.is_null() {
                return None;
            }
            success(tvg::tvg_picture_load_data(
                picture,
                data.as_ptr().cast(),
                data.len() as u32,
                c"lottie".as_ptr(),
                ptr::null(),
                true,
            ))?;
            let mut total = 0.0;
            success(tvg::tvg_animation_get_total_frame(
                frames.animation,
                &mut total,
            ))?;
            if !total.is_finite() || !(1.0..=MAX_FRAMES as f32).contains(&total) {
                return None;
            }
            let count = total.ceil() as usize;
            // Cumulative rounding keeps non-integer frame rates on time.
            frames.delays = (0..count)
                .map(|i| {
                    let at = |n: usize| (n as f64 * 1000.0 / header.fr).round() as u32;
                    (at(i + 1) - at(i)).max(1)
                })
                .collect();
            success(tvg::tvg_picture_set_size(
                picture,
                width as f32,
                height as f32,
            ))?;
            frames.canvas =
                tvg::tvg_swcanvas_create(tvg::Tvg_Engine_Option::TVG_ENGINE_OPTION_DEFAULT);
            if frames.canvas.is_null() {
                return None;
            }
            success(tvg::tvg_swcanvas_set_target(
                frames.canvas,
                frames.pixels.as_mut_ptr(),
                width,
                width,
                height,
                tvg::Tvg_Colorspace::TVG_COLORSPACE_ABGR8888S,
            ))?;
            success(tvg::tvg_canvas_add(frames.canvas, picture))?;
        }
        Some(frames)
    }
}

impl Iterator for Frames {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.delays.len() {
            return None;
        }
        // SAFETY: Live, exclusively owned handles and a fixed target buffer.
        // sync finishes all drawing before the pixels are read.
        unsafe {
            if self.index > 0 {
                success(tvg::tvg_animation_set_frame(
                    self.animation,
                    self.index as f32,
                ))?;
            }
            success(tvg::tvg_canvas_update(self.canvas))?;
            success(tvg::tvg_canvas_draw(self.canvas, true))?;
            success(tvg::tvg_canvas_sync(self.canvas))?;
        }
        self.index += 1;
        Some(
            self.pixels
                .iter()
                .flat_map(|pixel| pixel.to_le_bytes())
                .collect(),
        )
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        // SAFETY: The canvas releases its picture reference before the
        // animation releases its own; the pixel allocation is still alive.
        unsafe {
            if !self.canvas.is_null() {
                tvg::tvg_canvas_destroy(self.canvas);
            }
            if !self.animation.is_null() {
                tvg::tvg_animation_del(self.animation);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{decode_picture, decode_picture_at};

    const STICKER: &[u8] = include_bytes!("../../../testing/fixtures/sticker.json");

    #[test]
    fn lottie_renders_transparency_motion_and_frame_timings() {
        let picture = decode_picture(STICKER, 400, 400).unwrap();
        assert_eq!((picture.width, picture.height), (160, 160));
        assert_eq!(picture.animation.as_ref().unwrap().delays, [33, 34, 33]);
        let frames: Vec<_> = picture.frames().collect();
        assert_eq!(frames.len(), 3);
        assert_eq!(&*picture.rgba, frames[0]);
        assert_ne!(frames[0], frames[1]);
        assert_eq!(frames[0], frames[2]);
        assert_eq!(&frames[0][..4], [0, 0, 0, 0]);
        let pixel = (30 * 160 + 30) * 4;
        assert_eq!(&frames[0][pixel..pixel + 3], [255, 0, 0]);
        assert!((126..=128).contains(&frames[0][pixel + 3]));
    }

    #[test]
    fn lottie_reopens_cached_json_and_limits_render_size() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sticker.json");
        std::fs::write(&file, STICKER).unwrap();
        let picture = decode_picture_at(&file, 80, 80).unwrap();
        assert_eq!((picture.width, picture.height), (80, 80));
        assert_eq!(picture.frames().count(), 3);
        assert_eq!(picture.frames().count(), 3, "playback can loop");
    }

    #[test]
    fn lottie_rejects_invalid_and_excessive_input() {
        for data in [b"{}".as_slice(), b"{broken", b"\0"] {
            assert!(Frames::new(data, 160, 160).is_none());
        }
        let source: serde_json::Value = serde_json::from_slice(STICKER).unwrap();
        for (field, value) in [("w", 0), ("w", 8193), ("fr", 0), ("op", 100_000)] {
            let mut json = source.clone();
            json[field] = value.into();
            assert!(Frames::new(&serde_json::to_vec(&json).unwrap(), 160, 160).is_none());
        }
        assert!(Frames::new(&vec![b' '; MAX_BYTES + 1], 160, 160).is_none());
    }
}
