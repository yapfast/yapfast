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
//!
//! Patch 13: a video frame ([`frame`]) is registered the same way while a
//! handle over it lives, over the `Arc` its writer keeps: its bytes can't
//! change while the handle holds a clone of the `Arc`, since the writer's
//! `Arc::make_mut` then copies them first. The registry counts a buffer's
//! owners, as several handles may be made over one frame, and knows a
//! frame as one, which the app's hook may scale ([`crate::video`]).

use crate::core::Bytes;
use crate::core::image::Handle;

use rustc_hash::FxHashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

/// What the registry knows of a live buffer.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// Patch 19: every pixel is opaque.
    opaque: bool,
    /// Patch 13: a video frame.
    frame: bool,
    /// Its owners alive.
    owners: usize,
}

/// Each live buffer of [`from_rgba`] or [`frame`], by its address and
/// length.
static LIVE: LazyLock<Mutex<FxHashMap<(usize, usize), Entry>>> = LazyLock::new(Mutex::default);

fn live() -> MutexGuard<'static, FxHashMap<(usize, usize), Entry>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key(bytes: &[u8]) -> (usize, usize) {
    (bytes.as_ptr() as usize, bytes.len())
}

/// Counts an owner of `bytes`, which mustn't change until it's gone
/// ([`unregister`]). An empty buffer, whose address isn't its own, isn't.
fn register(bytes: &[u8], opaque: bool, frame: bool) {
    if bytes.is_empty() {
        return;
    }
    live()
        .entry(key(bytes))
        .or_insert(Entry {
            opaque,
            frame,
            owners: 0,
        })
        .owners += 1;
}

/// An owner of `bytes` goes: the last takes them out of the registry, while
/// their memory is still there.
fn unregister(bytes: &[u8]) {
    let mut live = live();
    if let Some(entry) = live.get_mut(&key(bytes)) {
        entry.owners -= 1;
        if entry.owners == 0 {
            let _ = live.remove(&key(bytes));
        }
    }
}

/// Premultiplied BGRA pixels, registered while they live. Its bytes can't
/// be written once it's made.
struct Buffer(Vec<u8>);

impl Buffer {
    fn new(pixels: Vec<u8>, opaque: bool) -> Self {
        register(&pixels, opaque, false);
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
        unregister(&self.0);
    }
}

/// Patch 13: a video frame's pixels, shared with their writer, registered
/// while this owner lives.
struct Frame<T: AsRef<[u8]>>(T);

impl<T: AsRef<[u8]>> AsRef<[u8]> for Frame<T> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<T: AsRef<[u8]>> Drop for Frame<T> {
    fn drop(&mut self) {
        // While the owner, the field, still holds the memory.
        unregister(self.0.as_ref());
    }
}

/// The bytes of an `Arc<Vec<u8>>`, which only lends `Vec<u8>`.
struct Shared(Arc<Vec<u8>>);

impl AsRef<[u8]> for Shared {
    fn as_ref(&self) -> &[u8] {
        &self.0
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

/// Patch 13: a video frame, `width` by `height` pixels of premultiplied
/// BGRA (straight BGRA, for an opaque frame), as a handle the renderer
/// draws from these very bytes, never converted or copied. Its writer
/// keeps the `Arc` and writes the next frame into it with `Arc::make_mut`,
/// which copies them first while this handle, or a clone of it, still
/// holds them; once they're gone it writes in place. `opaque` says every
/// pixel is, which the blit then copies a row at a time. Bytes that aren't
/// `width * height * 4` make a stock handle, read as straight RGBA.
pub fn frame(width: u32, height: u32, pixels: Arc<Vec<u8>>, opaque: bool) -> Handle {
    frame_of(width, height, Shared(pixels), opaque)
}

/// Patch 13: as [`frame`], over memory `owner` keeps and lends, such as a
/// slot of a mapping another process writes frames into: the handle keeps
/// `owner`, and with it the memory, alive. Its bytes mustn't be written
/// while the renderer may still draw the handle; the owner's writer waits
/// until the window drew a newer one. Bytes that aren't `width * height *
/// 4` make a stock handle of a copy, read as straight RGBA.
pub fn frame_of<T>(width: u32, height: u32, owner: T, opaque: bool) -> Handle
where
    T: AsRef<[u8]> + Send + 'static,
{
    let pixels = owner.as_ref();
    let size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if pixels.is_empty() || size != Some(pixels.len()) {
        return Handle::from_rgba(width, height, pixels.to_vec());
    }
    register(pixels, opaque, true);
    Handle::from_rgba(width, height, Bytes::from_owner(Frame(owner)))
}

/// Whether `handle` draws from its own bytes: made by [`from_rgba`] or
/// [`frame`], its bytes are premultiplied BGRA.
pub fn is_premultiplied(handle: &Handle) -> bool {
    match handle {
        Handle::Rgba { pixels, .. } => registered(pixels),
        Handle::Path(..) | Handle::Bytes(..) => false,
    }
}

/// Whether `bytes` are the whole of a live buffer of [`from_rgba`] or
/// [`frame`].
pub(crate) fn registered(bytes: &[u8]) -> bool {
    opaque(bytes).is_some()
}

/// Patch 19: whether every pixel of `bytes`, the whole of a live buffer of
/// [`from_rgba`] or [`frame`], is opaque; none for any other bytes.
pub(crate) fn opaque(bytes: &[u8]) -> Option<bool> {
    entry(bytes).map(|entry| entry.opaque)
}

/// Patch 13: whether `bytes` are the whole of a live [`frame`] whose every
/// pixel is opaque.
pub(crate) fn is_opaque_frame(bytes: &[u8]) -> bool {
    entry(bytes).is_some_and(|entry| entry.frame && entry.opaque)
}

fn entry(bytes: &[u8]) -> Option<Entry> {
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
    fn a_frame_is_drawn_from_its_writers_bytes_while_a_handle_holds_them() {
        let mut pixels = Arc::new(vec![10, 20, 30, 255, 40, 50, 60, 255]);
        let address = pixels.as_ptr();
        let handle = frame(2, 1, pixels.clone(), true);
        let Handle::Rgba { pixels: bytes, .. } = &handle else {
            panic!("pixels");
        };
        assert_eq!(bytes.as_ptr(), address, "the writer's bytes, not a copy");
        assert_eq!(
            &bytes[..],
            &[10, 20, 30, 255, 40, 50, 60, 255],
            "as they are"
        );
        assert!(is_premultiplied(&handle) && is_opaque_frame(bytes));
        assert_eq!(opaque(bytes), Some(true));

        // Two handles over one frame: marked until both go.
        let again = frame(2, 1, pixels.clone(), true);
        drop(handle);
        assert!(is_premultiplied(&again));

        // Written while it shows, the writer writes a copy; what shows stays.
        Arc::make_mut(&mut pixels)[0] = 99;
        assert_ne!(pixels.as_ptr(), address);
        let Handle::Rgba { pixels: shown, .. } = &again else {
            panic!("pixels");
        };
        assert_eq!(shown[0], 10);
        drop(again);
        assert!(!live().contains_key(&(address as usize, 8)));

        // Once nothing shows a frame, its writer writes it in place.
        let handle = frame(2, 1, pixels.clone(), true);
        let written = pixels.as_ptr();
        drop(handle);
        Arc::make_mut(&mut pixels)[1] = 7;
        assert_eq!(pixels.as_ptr(), written, "in place");

        // A picture isn't a frame, and a frame of the wrong size isn't marked.
        let picture = from_rgba(1, 1, vec![1, 2, 3, 255]);
        let Handle::Rgba { pixels: bytes, .. } = &picture else {
            panic!("pixels");
        };
        assert!(registered(bytes) && !is_opaque_frame(bytes));
        assert!(!is_premultiplied(&frame(3, 1, Arc::new(vec![0; 8]), true)));
        let clear = frame(1, 1, Arc::new(vec![0; 4]), false);
        let Handle::Rgba { pixels: bytes, .. } = &clear else {
            panic!("pixels");
        };
        assert!(registered(bytes) && !is_opaque_frame(bytes), "not opaque");
    }

    /// Memory of someone else's, lent while its owner lives.
    struct Lent(Arc<[u8; 8]>, Arc<std::sync::atomic::AtomicBool>);

    impl AsRef<[u8]> for Lent {
        fn as_ref(&self) -> &[u8] {
            &self.0[..]
        }
    }

    impl Drop for Lent {
        fn drop(&mut self) {
            self.1.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[test]
    fn a_frame_over_an_owner_is_drawn_from_its_memory_while_it_lives() {
        let memory = Arc::new([1, 2, 3, 255, 4, 5, 6, 255]);
        let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let address = memory.as_ptr();
        let handle = frame_of(2, 1, Lent(memory.clone(), gone.clone()), true);
        let Handle::Rgba { pixels: bytes, .. } = &handle else {
            panic!("pixels");
        };
        assert_eq!(bytes.as_ptr(), address, "its memory, not a copy");
        assert!(is_premultiplied(&handle) && is_opaque_frame(bytes));
        let again = handle.clone();
        drop(handle);
        assert!(is_premultiplied(&again) && !gone.load(std::sync::atomic::Ordering::Relaxed));
        drop(again);
        assert!(
            gone.load(std::sync::atomic::Ordering::Relaxed),
            "the owner goes"
        );
        assert!(!live().contains_key(&(address as usize, 8)), "unmarked");
        let wrong = frame_of(3, 1, Lent(memory, gone), true);
        assert!(!is_premultiplied(&wrong), "a copy of the wrong size");
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
