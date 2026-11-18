//! Pictures: drawn at the size and in the shape asked, round and
//! rounded-square on any ground, and moving pictures playing on a shared
//! clock while they show and the window plays. The Slint side: `Picture`
//! (`ui/controls.slint:423`) in a clipping `Rectangle`, and
//! `src/ui/pictures.rs` with `src/ui/emoji_animation.rs` behind them.
//!
//! - [`picture`] is the widget, and [`live`] the same over a moving
//!   picture's newest frame, read as it's drawn; [`Player`] is what it
//!   keeps, for a widget of its own that draws pictures among other things.
//! - A copy (`look`) is made at the size drawn in physical pixels
//!   ([`set_scale`]), which the renderer copies in on whole pixels and cuts
//!   the corners of; one only shrunk is made at a mip level, which
//!   every size within twice of it shares. Copies and frames go to the
//!   renderer in its own format, which it draws without a copy of its own
//!   (`look::handle`). While a picture's size changes, as a pane is
//!   dragged, it keeps the copy of the size it held until the new one holds
//!   (`player`). Copies, blurs and frames are kept within eight MiB
//!   (`store`).
//! - A still moves when [`set_animations`] or [`register`] says so: its
//!   frames are decoded off the window's thread at the size drawn, a frame
//!   ahead, by runs that end by themselves once nothing takes their frames,
//!   and a small animation keeps them all after one round and decodes no
//!   more (`decode`). The frame showing comes from one clock that runs
//!   only while something plays (`clock`).
//! - [`sample`] draws the fake Discord's moving GIF; its stills are
//!   `anomi_vm::fixtures::pictures`, which fixtures fill avatars from.
//!
//! Nothing here ticks: a playing picture asks the window for a redraw when
//! its next frame is due, and one held still, scrolled away or in a window
//! behind another asks for none.

mod clock;
mod decode;
mod look;
mod moving;
mod player;
pub mod sample;
mod store;
mod widget;

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use iced::widget::image;

pub use look::Fit;
pub use moving::{Find, Frames, Moving, forget, register, set_animations};
pub use player::{Player, Shape};
pub use store::{BUDGET, bytes, clear};
pub use widget::{Picture, live, picture};

/// The window's scale factor, as bits of an `f32`; 1 until told.
static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// Has copies made for a window drawn at `scale` physical pixels a logical
/// one: the window's scale factor times its zoom, which iced's `Rescaled`
/// leaves out. The app is its one caller, as the window opens, rescales or
/// zooms.
pub fn set_scale(scale: f32) {
    if scale.is_finite() && scale > 0.0 {
        SCALE.store(scale.to_bits(), Ordering::Relaxed);
    }
}

/// The scale copies are made at.
pub fn scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

/// A spoiler's picture before it's revealed: shrunk to ten pixels or so on
/// its longer side, which drawing at full size smooths into a blur. Made
/// once a picture and kept with the copies; none for a picture that isn't
/// pixels. Ported from `blurred` in `src/ui/pictures.rs`.
pub fn blurred(handle: &image::Handle) -> Option<image::Handle> {
    const SIDE: u32 = 10;
    let image::Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = handle
    else {
        return None;
    };
    let shrink = ((*width).max(*height) / SIDE).max(1);
    let look = look::Look {
        width: width.div_ceil(shrink).max(1),
        height: height.div_ceil(shrink).max(1),
        cover: false,
        top: false,
    };
    let key = store::Key {
        source: handle.id(),
        look,
        kind: store::Kind::Blur,
    };
    store::still(key, Instant::now(), || {
        let small = look::make(pixels, *width, *height, &look);
        // Straight RGBA, unlike the copies: a blur is drawn through a copy
        // of its own at the size and in the shape shown, made from it.
        Some(image::Handle::from_rgba(look.width, look.height, small))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spoiler_blurs_to_ten_pixels_once() {
        let sunset = anomi_vm::fixtures::pictures::sunset(800, 500);
        let blur = blurred(&sunset).expect("pixels");
        assert!(
            matches!(
                blur,
                image::Handle::Rgba {
                    width: 10,
                    height: 7,
                    ..
                }
            ),
            "{blur:?}"
        );
        assert_eq!(blurred(&sunset).map(|again| again.id()), Some(blur.id()));
    }

    #[test]
    fn the_scale_takes_only_a_real_one() {
        let before = scale();
        set_scale(f32::NAN);
        set_scale(0.0);
        assert_eq!(scale(), before);
    }
}
