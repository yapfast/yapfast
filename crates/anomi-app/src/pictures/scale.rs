//! The renderer's hook for video frames drawn at a size other than their
//! own (`vendor/iced_tiny_skia`, patch 13): a stream larger or smaller than
//! its tile, a camera a pixel short of it. libyuv, which the core's
//! libwebrtc archive links in, scales the frame bilinear straight into the
//! window, only inside the part the renderer gives, which then cuts the
//! corners; tiny-skia's pattern pipeline would cost several times more.
//! Frames are premultiplied BGRA and opaque already (`frames`), libyuv's
//! "ARGB", so nothing is swapped. Yapfast's `video.rs` `draw`, without its
//! swap and its own registry of frame memory.

use std::ffi::c_int;

use iced_tiny_skia::video::Draw;

/// libyuv's `kFilterBilinear`.
const BILINEAR: c_int = 2;

unsafe extern "C" {
    /// Scales a picture of four bytes a pixel, writing only `clip` of the
    /// scaled one, its place within it; 0 when it did.
    fn ARGBScaleClip(
        source: *const u8,
        source_stride: c_int,
        source_width: c_int,
        source_height: c_int,
        target: *mut u8,
        target_stride: c_int,
        target_width: c_int,
        target_height: c_int,
        clip_x: c_int,
        clip_y: c_int,
        clip_width: c_int,
        clip_height: c_int,
        filtering: c_int,
    ) -> c_int;
}

/// Scales `draw`'s frame into its target within its clip, or declines one
/// whose sizes don't hold, having written nothing.
pub(super) fn scale(draw: &mut Draw<'_>) -> bool {
    let [clip_x, clip_y, clip_width, clip_height] = draw.clip;
    let stride = draw.target_width as usize * 4;
    let source_stride = draw.width as usize * 4;
    let end = (clip_y + clip_height) as usize * stride;
    let fits = |side: u32| c_int::try_from(side).is_ok();
    if clip_width == 0
        || clip_height == 0
        || draw.source.len() < source_stride * draw.height as usize
        || draw.target.len() < end
        || clip_x + clip_width > draw.target_width
        || ![
            draw.width,
            draw.height,
            draw.to_width,
            draw.to_height,
            draw.target_width,
        ]
        .into_iter()
        .all(fits)
    {
        return false;
    }
    // The scaled frame's corner, which may lie outside the target; libyuv
    // writes only the clip, which lies inside it (checked above).
    let corner = draw
        .target
        .as_mut_ptr()
        .wrapping_offset(draw.y as isize * stride as isize + draw.x as isize * 4);
    // SAFETY: libyuv reads the whole source, `height` rows of `width`
    // pixels, which it holds (checked above), and writes `clip_height` rows
    // of `clip_width` pixels at the clip's place in the target, which holds
    // them; the source is a frame's memory, the target the window's, so they
    // don't overlap.
    let done = unsafe {
        ARGBScaleClip(
            draw.source.as_ptr(),
            source_stride as c_int,
            draw.width as c_int,
            draw.height as c_int,
            corner,
            stride as c_int,
            draw.to_width as c_int,
            draw.to_height as c_int,
            clip_x as c_int - draw.x,
            clip_y as c_int - draw.y,
            clip_width as c_int,
            clip_height as c_int,
            BILINEAR,
        )
    };
    if done != 0 {
        return false;
    }
    // Its filter weighs two pixels by sevenths of 128 summing to 127, so
    // an opaque frame comes out with alpha 253: opaque again, as the blit
    // leaves a frame at its own size. Its colours stay a level or two
    // darker at most, as Yapfast's do.
    for row in draw.target[clip_y as usize * stride..end].chunks_exact_mut(stride) {
        let line = &mut row[clip_x as usize * 4..(clip_x + clip_width) as usize * 4];
        for pixel in line.chunks_exact_mut(4) {
            pixel[3] = u8::MAX;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of one colour scaled into the middle of a target: it fills
    /// the clip, opaque, its colour two levels darker at most, and nothing
    /// else changes.
    #[test]
    fn a_frame_fills_its_clip_and_nothing_more() {
        let colour = [10, 120, 230, 255];
        let source = colour.repeat(16 * 9);
        let (width, height) = (40u32, 30u32);
        let ground = [1, 2, 3, 255];
        let mut target = ground.repeat((width * height) as usize);
        // Drawn from (-4, 2) at 32 by 18, seen from (3, 5), 20 by 10.
        let clip = [3, 5, 20, 10];
        let drawn = scale(&mut Draw {
            source: &source,
            width: 16,
            height: 9,
            x: -4,
            y: 2,
            to_width: 32,
            to_height: 18,
            clip,
            target: &mut target,
            target_width: width,
        });
        assert!(drawn);
        for (at, pixel) in target.chunks_exact(4).enumerate() {
            let (x, y) = (at as u32 % width, at as u32 / width);
            if !((3..23).contains(&x) && (5..15).contains(&y)) {
                assert_eq!(pixel, ground, "{x}, {y}");
                continue;
            }
            assert_eq!(pixel[3], 255, "{x}, {y}");
            for channel in 0..3 {
                let darker = colour[channel] - pixel[channel];
                assert!(darker <= 2, "{x}, {y}: {pixel:?}");
            }
        }
    }

    #[test]
    fn a_clip_past_the_target_is_declined() {
        let source = [9u8; 4 * 4];
        let mut target = vec![0u8; 8 * 8 * 4];
        let drawn = scale(&mut Draw {
            source: &source,
            width: 2,
            height: 2,
            x: 0,
            y: 0,
            to_width: 16,
            to_height: 16,
            clip: [0, 0, 16, 16],
            target: &mut target,
            target_width: 8,
        });
        assert!(!drawn);
        assert!(target.iter().all(|&byte| byte == 0));
    }

    /// Times 300 frames of 1280 by 720 drawn through the renderer at 960
    /// by 540, scaled here, and the swap that makes each the renderer's
    /// format where it's decoded. Run it alone: `cargo test --profile fast
    /// -p anomi-app timed -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement"]
    fn timed_300_frames_of_720p() {
        use std::sync::Arc;
        use std::time::Instant;

        use iced::advanced::image::{self as raster, Renderer as _};
        use iced::advanced::renderer::Renderer as _;
        use iced::{Color, Rectangle, Size};
        use iced_tiny_skia::graphics::Viewport;

        const FRAMES: usize = 300;
        crate::pictures::install();
        let (width, height) = (1280u32, 720u32);
        let mut buffers: Vec<Arc<Vec<u8>>> = (0..4u8)
            .map(|n| Arc::new([40 + n * 50, 200, 90, 255].repeat((width * height) as usize)))
            .collect();
        let start = Instant::now();
        for at in 0..FRAMES {
            super::super::frames::to_bgra(Arc::make_mut(&mut buffers[at % 4]).as_mut_slice());
        }
        let swap = start.elapsed();

        let mut renderer = iced_tiny_skia::Renderer::new(Default::default());
        let mut pixels = tiny_skia::Pixmap::new(width, height).expect("a window");
        let mut mask = tiny_skia::Mask::new(width, height).expect("a mask");
        let viewport = Viewport::with_physical_size(Size::new(width, height), Default::default());
        let window = Rectangle::with_size(Size::new(width as f32, height as f32));
        let tile = Rectangle::new([160.0, 90.0].into(), Size::new(960.0, 540.0));
        let start = Instant::now();
        for at in 0..FRAMES {
            let frame =
                iced_tiny_skia::premultiplied::frame(width, height, buffers[at % 4].clone(), true);
            renderer.reset(window);
            renderer.draw_image(raster::Image::new(frame), tile, tile);
            renderer.draw(
                &mut pixels.as_mut(),
                &mut mask,
                &viewport,
                &[tile],
                Color::BLACK,
            );
        }
        let scaled = start.elapsed();
        let per_frame = |spent: std::time::Duration| spent.as_secs_f64() * 1000.0 / FRAMES as f64;
        println!(
            "swapped where decoded: {:.2} ms a frame; drawn at 960x540: {:.2} ms a frame, {} bytes converted",
            per_frame(swap),
            per_frame(scaled),
            iced_tiny_skia::converted_bytes()
        );
    }
}
