//! Moving pictures' frames, such as a camera in a call or a video playing,
//! made by the core straight into memory iced draws without a copy. Ported
//! from `SlintFrame` and `make_frames_in_slint_buffers` in
//! `src/ui/pictures.rs`; the feeds that write them are task I09's and V03's.
//!
//! The core writes a frame's pixels as RGBA; once they're all written
//! ([`FrameBuffer::written`]), on the thread that decoded them, red and
//! blue change places, which makes them the renderer's own format,
//! premultiplied BGRA, as every frame is opaque. A handle over them is
//! marked as a frame (`iced_tiny_skia::premultiplied::frame`), so the
//! renderer draws it from these bytes: copied in a row at a time at its own
//! size, its corners cut, or scaled by libyuv at another (`scale`), never
//! converted or copied into a cache of its own.

use std::sync::Arc;

use iced::widget::image;
use yapfast_core::picture::FrameBuffer;

/// A frame's pixels, which the handles drawing it share.
struct Frame {
    size: (u32, u32),
    pixels: Arc<Vec<u8>>,
}

impl FrameBuffer for Frame {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        // The feed writes only buffers no longer on screen, whose handles
        // went with the frames that drew them, so this never copies; one a
        // handle still holds is copied first, so what shows doesn't change
        // under the renderer.
        Arc::make_mut(&mut self.pixels).as_mut_slice()
    }

    fn written(&mut self) {
        to_bgra(Arc::make_mut(&mut self.pixels).as_mut_slice());
    }
}

/// Swaps red and blue in each pixel, in place.
pub(super) fn to_bgra(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

/// Makes a buffer for a `width` by `height` frame; see
/// [`yapfast_core::picture::set_frame_buffers`].
pub(super) fn make(width: u32, height: u32) -> Box<dyn FrameBuffer> {
    Box::new(Frame {
        size: (width, height),
        pixels: Arc::new(vec![0; width as usize * height as usize * 4]),
    })
}

/// A frame the core made, drawn from its own bytes; none for a buffer made
/// before [`super::install`]. Make it once a frame: each handle is a new
/// picture to the renderer.
pub fn frame(buffer: &dyn FrameBuffer) -> Option<image::Handle> {
    let frame = (buffer as &dyn std::any::Any).downcast_ref::<Frame>()?;
    let (width, height) = frame.size;
    // Every frame is opaque: libyuv writes each pixel's alpha whole.
    Some(iced_tiny_skia::premultiplied::frame(
        width,
        height,
        frame.pixels.clone(),
        true,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_core::picture::Feed;

    #[test]
    fn a_frame_is_drawn_from_the_buffer_the_core_wrote_turned_into_bgra() {
        super::super::install();
        let feed = Feed::new();
        feed.show_at(Some((2, 1)));
        feed.write(2, 1, |rgba| {
            rgba.copy_from_slice(&[1, 2, 3, 255, 5, 6, 7, 255]);
        });
        let drawn = feed
            .take(|buffer| frame(buffer).expect("an iced frame"))
            .expect("a frame");
        let image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } = &drawn
        else {
            panic!("pixels");
        };
        assert_eq!((*width, *height), (2, 1));
        assert_eq!(&pixels[..], &[3, 2, 1, 255, 7, 6, 5, 255], "BGRA, in place");
        assert!(iced_tiny_skia::premultiplied::is_premultiplied(&drawn));

        // Written again while that frame still shows, it's copied first, so
        // what shows doesn't change under the renderer.
        let mut buffer = make(2, 1);
        buffer
            .pixels_mut()
            .copy_from_slice(&[1, 2, 3, 255, 5, 6, 7, 255]);
        buffer.written();
        let shown = frame(&*buffer).expect("an iced frame");
        buffer.pixels_mut()[0] = 9;
        let image::Handle::Rgba { pixels, .. } = &shown else {
            panic!("pixels");
        };
        assert_eq!(pixels[0], 3);
    }
}
