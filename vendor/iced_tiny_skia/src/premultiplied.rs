//! Anomi port (patch 1): pictures drawn from their own bytes.
//!
//! Stock, the raster cache converts each `Rgba` picture it draws into a
//! copy in tiny-skia's own format, premultiplied BGRA, and keeps the copy
//! while the picture is drawn, so every picture on screen is held twice: the
//! app's pixels and the renderer's. Yapfast's patch takes every `Rgba`
//! handle to be in that format already. Here a picture says so for itself:
//! [`from_rgba`] converts its bytes in place, as the cache would, and puts
//! them in a buffer that registers its address while it lives; the raster
//! pipeline draws a handle whose bytes are such a buffer straight from them.
//! Any other `Rgba` handle is straight RGBA and is converted as before.
//!
//! Why an address can be trusted: a buffer is registered when it's made and
//! unregistered in its `drop`, before its memory is freed, so while an
//! address is registered no other allocation can begin there. A handle's
//! `Bytes` keep the buffer they point into alive while any clone of the
//! handle does (the renderer's layers hold clones while they draw), and the
//! buffer lends its bytes only to be read, so they don't change while the
//! renderer may read them. The address and the length must both match, so
//! a part of a buffer isn't taken for it; an empty buffer, whose address
//! isn't its own, is never registered.
//!
//! Patch 19: the registry also keeps whether every pixel of a buffer is
//! opaque, found while it's converted, so the blit copies an opaque
//! picture's rows without looking at each pixel's alpha.

use crate::core::Bytes;
use crate::core::image::Handle;

use rustc_hash::FxHashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

/// The address and length of each live [`Buffer`], and whether all its
/// pixels are opaque.
static LIVE: LazyLock<Mutex<FxHashMap<(usize, usize), bool>>> = LazyLock::new(Mutex::default);

fn live() -> MutexGuard<'static, FxHashMap<(usize, usize), bool>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key(bytes: &[u8]) -> (usize, usize) {
    (bytes.as_ptr() as usize, bytes.len())
}

/// Premultiplied BGRA pixels, registered while they live. Its bytes can't
/// be written once it's made.
struct Buffer(Vec<u8>);

impl Buffer {
    fn new(pixels: Vec<u8>, opaque: bool) -> Self {
        if !pixels.is_empty() {
            let _ = live().insert(key(&pixels), opaque);
        }
        Self(pixels)
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // Before the field, and with it the memory, goes.
        if !self.0.is_empty() {
            let _ = live().remove(&key(&self.0));
        }
    }
}

/// A `width` by `height` picture of straight RGBA bytes as a handle the
/// renderer draws from these very bytes, without a copy of its own. The
/// bytes are turned in place into premultiplied BGRA exactly as the raster
/// cache converts a picture, so it draws the same pixels; whatever else
/// reads the handle's bytes reads that format. Bytes that aren't `width *
/// height * 4`, or none, make a stock handle of them as they are, which
/// draws (or fails to) as `Handle::from_rgba` does.
pub fn from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Handle {
    let size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if rgba.is_empty() || size != Some(rgba.len()) {
        return Handle::from_rgba(width, height, rgba);
    }
    let mut opaque = true;
    for pixel in rgba.chunks_exact_mut(4) {
        opaque &= pixel[3] == u8::MAX;
        let converted = premultiply([pixel[0], pixel[1], pixel[2], pixel[3]]);
        pixel.copy_from_slice(&converted);
    }
    Handle::from_rgba(width, height, Bytes::from_owner(Buffer::new(rgba, opaque)))
}

/// Whether `handle` draws from its own bytes: made by [`from_rgba`], its
/// bytes are premultiplied BGRA.
pub fn is_premultiplied(handle: &Handle) -> bool {
    match handle {
        Handle::Rgba { pixels, .. } => registered(pixels),
        Handle::Path(..) | Handle::Bytes(..) => false,
    }
}

/// Whether `bytes` are the whole of a live buffer of [`from_rgba`].
pub(crate) fn registered(bytes: &[u8]) -> bool {
    opaque(bytes).is_some()
}

/// Patch 19: whether every pixel of `bytes`, the whole of a live buffer of
/// [`from_rgba`], is opaque; none for any other bytes.
pub(crate) fn opaque(bytes: &[u8]) -> Option<bool> {
    if bytes.is_empty() {
        return None;
    }
    live().get(&key(bytes)).copied()
}

/// A straight RGBA pixel as the raster cache stores it: premultiplied BGRA,
/// tiny-skia's `ColorU8::premultiply` of it with red and blue swapped.
pub(crate) fn premultiply([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let pixel = tiny_skia::ColorU8::from_rgba(b, g, r, a).premultiply();
    [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_converted_in_place_and_known_while_it_lives() {
        let rgba = vec![200, 100, 50, 255, 200, 100, 50, 128, 9, 9, 9, 0];
        let address = rgba.as_ptr();
        let handle = from_rgba(3, 1, rgba);
        let Handle::Rgba { pixels, .. } = &handle else {
            panic!("pixels");
        };
        assert_eq!(pixels.as_ptr(), address, "the same bytes, not a copy");
        assert_eq!(
            &pixels[..],
            &[50, 100, 200, 255, 25, 50, 100, 128, 0, 0, 0, 0]
        );
        assert!(is_premultiplied(&handle));
        assert_eq!(opaque(pixels), Some(false), "two pixels see-through");
        let key = key(pixels);

        // A clone shares them; a part of them, or the same bytes copied,
        // isn't the buffer.
        let clone = handle.clone();
        let part = Handle::from_rgba(2, 1, pixels.slice(..8));
        let copied = Handle::from_rgba(3, 1, pixels.to_vec());
        assert!(is_premultiplied(&clone));
        assert!(!is_premultiplied(&part));
        assert!(!is_premultiplied(&copied));

        // Gone with the last handle, and the part that kept it alive.
        drop((handle, clone));
        assert!(live().contains_key(&key), "a part keeps the buffer");
        drop(part);
        assert!(!live().contains_key(&key));
    }

    #[test]
    fn an_opaque_picture_is_known_as_one() {
        let handle = from_rgba(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]);
        let Handle::Rgba { pixels, .. } = &handle else {
            panic!("pixels");
        };
        assert_eq!(opaque(pixels), Some(true));
        let copied: Vec<u8> = pixels.iter().copied().collect();
        assert_eq!(opaque(&copied), None, "a copy isn't the buffer");
    }

    #[test]
    fn bytes_of_the_wrong_size_stay_as_they_are() {
        for (width, height, bytes) in [(2, 2, vec![1; 12]), (0, 0, vec![]), (1, 1, vec![1; 8])] {
            let handle = from_rgba(width, height, bytes.clone());
            assert!(!is_premultiplied(&handle), "{width}x{height}");
            let Handle::Rgba { pixels, .. } = &handle else {
                panic!("pixels");
            };
            assert_eq!(&pixels[..], &bytes[..]);
        }
    }

    #[test]
    fn the_conversion_is_tiny_skias() {
        for a in [0, 1, 77, 128, 254, 255] {
            for c in [0, 1, 100, 200, 255] {
                let [b, g, r, alpha] = premultiply([c, c / 2, 255 - c, a]);
                let expected = tiny_skia::ColorU8::from_rgba(c, c / 2, 255 - c, a).premultiply();
                assert_eq!(
                    [r, g, b, alpha],
                    [
                        expected.red(),
                        expected.green(),
                        expected.blue(),
                        expected.alpha()
                    ]
                );
            }
        }
    }
}
