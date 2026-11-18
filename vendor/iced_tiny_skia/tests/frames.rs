//! Anomi port (patch 13): video frames drawn through the whole renderer, as
//! the window draws them: a new handle each frame over the memory the core
//! wrote, the damage the frame's rectangle.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use iced_tiny_skia::core::image::{self, Renderer as _};
use iced_tiny_skia::core::renderer::{self, Renderer as _, Scale};
use iced_tiny_skia::core::{Bytes, Color, Rectangle, Size};
use iced_tiny_skia::graphics::Viewport;
use iced_tiny_skia::{Renderer, premultiplied};

const WINDOW: (u32, u32) = (1280, 720);

/// Held while a test draws: what the renderers hold converted is counted
/// for the whole process.
static DRAWING: Mutex<()> = Mutex::new(());

/// The bytes a stock handle reads: the frame's, shared, as
/// `anomi-app/src/pictures/frames.rs` handed them before patch 13.
struct Shared(Arc<Vec<u8>>);

impl AsRef<[u8]> for Shared {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Four frame buffers, as the core's feed keeps, each opaque and of its
/// own colour, `width` by `height`: premultiplied BGRA to a marked frame,
/// straight RGBA to a stock one, which costs the same.
fn buffers(width: u32, height: u32) -> Vec<Arc<Vec<u8>>> {
    (0..4u8)
        .map(|n| {
            let pixel = [40 + n * 50, 200 - n * 40, 90 + n * 30, 255];
            Arc::new(pixel.repeat(width as usize * height as usize))
        })
        .collect()
}

/// What `frames` frames cost the renderer: its time, the bytes of copies it
/// converted, and the window's pixels after the last.
fn draw(
    frames: usize,
    frame: impl Fn(usize) -> image::Handle,
    bounds: Rectangle,
) -> (Duration, usize, Vec<u8>) {
    let _drawing = DRAWING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut renderer = Renderer::new(renderer::Settings::default());
    let mut pixels = tiny_skia::Pixmap::new(WINDOW.0, WINDOW.1).expect("a window");
    let mut mask = tiny_skia::Mask::new(WINDOW.0, WINDOW.1).expect("a mask");
    let viewport = Viewport::with_physical_size(Size::new(WINDOW.0, WINDOW.1), Scale::default());
    let window = Rectangle::with_size(Size::new(WINDOW.0 as f32, WINDOW.1 as f32));
    let (mut spent, mut converted) = (Duration::ZERO, 0);
    for at in 0..frames {
        let handle = frame(at);
        let start = Instant::now();
        renderer.reset(window);
        renderer.draw_image(image::Image::new(handle), bounds, bounds);
        renderer.draw(
            &mut pixels.as_mut(),
            &mut mask,
            &viewport,
            &[bounds],
            Color::BLACK,
        );
        spent += start.elapsed();
        // The copies the draw made and kept: each frame is a picture of its
        // own, so the copy held after it is the one its draw converted.
        converted += iced_tiny_skia::converted_bytes();
    }
    (spent, converted, pixels.data().to_vec())
}

/// Thirty frames of 1280 by 720, each a new handle on one of the feed's
/// buffers: none is converted or copied, and each shows.
#[test]
fn marked_frames_are_neither_converted_nor_copied() {
    let (width, height) = (1280, 720);
    let buffers = buffers(width, height);
    let whole = Rectangle::with_size(Size::new(width as f32, height as f32));
    for frames in [1, 2, 3, 4, 30] {
        let (_, converted, window) = draw(
            frames,
            |at| premultiplied::frame(width, height, buffers[at % 4].clone(), true),
            whole,
        );
        assert_eq!(converted, 0, "{frames} frames");
        let last = &buffers[(frames - 1) % 4];
        assert!(window == last[..], "frame {frames} shows");
    }
}

/// Times 300 frames of 1280 by 720, one to one, each a new handle. Run it
/// alone, optimized: `cargo test --release --test frames -- --ignored`.
#[test]
#[ignore = "a measurement"]
fn three_hundred_frames_of_720p() {
    const FRAMES: usize = 300;
    let (width, height) = (1280, 720);
    let buffers = buffers(width, height);
    let whole = Rectangle::with_size(Size::new(width as f32, height as f32));
    let shrunk = Rectangle::new([160.0, 90.0].into(), Size::new(960.0, 540.0));
    for (name, bounds) in [("one to one", whole), ("scaled to 960x540", shrunk)] {
        let stock = draw(
            FRAMES,
            |at| {
                image::Handle::from_rgba(
                    width,
                    height,
                    Bytes::from_owner(Shared(buffers[at % 4].clone())),
                )
            },
            bounds,
        );
        let marked = draw(
            FRAMES,
            |at| premultiplied::frame(width, height, buffers[at % 4].clone(), true),
            bounds,
        );
        for (kind, (spent, converted, _)) in [("stock", stock), ("marked", marked)] {
            println!(
                "{kind}, {name}: {:.2} ms a frame, {} MB converted",
                spent.as_secs_f64() * 1000.0 / FRAMES as f64,
                converted / 1_000_000
            );
        }
    }
}
