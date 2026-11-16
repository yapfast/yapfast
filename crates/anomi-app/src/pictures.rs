//! Pictures the core decodes, kept in buffers iced draws without a copy.
//!
//! The core makes every picture's pixels with the function it's handed
//! ([`yapfast_core::chat::set_pixel_buffers`]), as the Slint app hands it
//! one making Slint's buffers. Here a buffer holds its pixels as shared
//! bytes and an iced handle over the same bytes, made once, so each row
//! that shows the picture clones the handle and the renderer caches it by
//! the handle's id. The pixels are copied once, as they come.

use bytes::Bytes;
use iced::widget::image;
use yapfast_core::application::AppState;
use yapfast_core::chat::{self, Picture, PixelBuffer};

/// A picture's pixels and the handle that draws them.
struct Drawn {
    handle: image::Handle,
    pixels: Bytes,
}

impl PixelBuffer for Drawn {
    fn bytes(&self) -> &[u8] {
        &self.pixels
    }
}

fn make(width: u32, height: u32, rgba: &[u8]) -> Box<dyn PixelBuffer> {
    let pixels = Bytes::copy_from_slice(rgba);
    Box::new(Drawn {
        handle: image::Handle::from_rgba(width, height, pixels.clone()),
        pixels,
    })
}

/// Has the core decode every picture from now on into buffers iced draws.
/// Call it before the backend starts.
pub fn install() {
    chat::set_pixel_buffers(make);
}

/// The handle that draws a picture; none for one decoded before
/// [`install`].
pub fn handle(picture: &Picture) -> Option<image::Handle> {
    picture
        .rgba
        .buffer::<Drawn>()
        .map(|drawn| drawn.handle.clone())
}

/// The picture at an address, once it arrived.
pub fn find(state: &AppState, address: Option<&str>) -> Option<image::Handle> {
    state
        .pictures
        .get(address?)
        .and_then(|picture| handle(picture))
}
