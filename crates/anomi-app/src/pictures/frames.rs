//! Moving pictures' frames, such as a camera in a call or a video playing,
//! made by the core straight into memory iced draws without a copy. Ported
//! from `SlintFrame` and `make_frames_in_slint_buffers` in
//! `src/ui/pictures.rs`; the feeds that write them are task I09's and V03's.

use std::sync::Arc;

use bytes::Bytes;
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
        // went with the view that drew them, so this never copies.
        Arc::make_mut(&mut self.pixels).as_mut_slice()
    }
}

/// The pixels a handle reads: the frame's, shared.
struct Shared(Arc<Vec<u8>>);

impl AsRef<[u8]> for Shared {
    fn as_ref(&self) -> &[u8] {
        &self.0
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

/// A frame the core made, drawn without a copy; none for a buffer made
/// before [`super::install`]. Make it once a frame: each handle is a new
/// picture to the renderer, converted once while it shows.
pub fn frame(buffer: &dyn FrameBuffer) -> Option<image::Handle> {
    let frame = (buffer as &dyn std::any::Any).downcast_ref::<Frame>()?;
    let (width, height) = frame.size;
    let pixels = Bytes::from_owner(Shared(frame.pixels.clone()));
    Some(image::Handle::from_rgba(width, height, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_drawn_from_the_buffer_the_core_wrote() {
        let mut buffer = make(2, 1);
        buffer
            .pixels_mut()
            .copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let drawn = frame(&*buffer).expect("an iced frame");
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
        assert_eq!(&pixels[..], &[1, 2, 3, 4, 5, 6, 7, 8]);
        // Written again while that frame still shows, it's copied first, so
        // what shows doesn't change under the renderer.
        buffer.pixels_mut()[0] = 9;
        assert_eq!(pixels[0], 1);
    }
}
