//! The fake Discord's moving GIF, drawn in code as `testing/fake_discord.ts`
//! draws it, for shots and tests of what plays. Its stills (avatars, icons,
//! banners, emoji) are `anomi_vm::fixtures::pictures`, so fixtures can carry
//! them; this one needs the picture store, which is here.
//!
//! Made once a thread and handed out again, so its handle keeps its id.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use iced::widget::image;

use super::moving::{self, Frames};

thread_local! {
    static MADE: RefCell<HashMap<String, image::Handle>> = RefCell::default();
}

fn made(name: String, make: impl FnOnce() -> image::Handle) -> image::Handle {
    if let Some(handle) = MADE.with_borrow(|made| made.get(&name).cloned()) {
        return handle;
    }
    let handle = make();
    MADE.with_borrow_mut(|made| made.insert(name, handle.clone()));
    handle
}

/// The fake's moving GIF (`bouncing` in `testing/fake_discord.ts`): a dot of
/// `colour` going to and fro on a dark ground, `count` frames a tenth of a
/// second each. The still is its first frame; it plays where it's drawn
/// with a [`Motion`](anomi_theme::Motion) that plays.
pub fn bouncing(side: u32, count: u32, colour: [u8; 3]) -> image::Handle {
    let name = format!("bouncing {side} {count} {colour:?}");
    made(name, || {
        let frame = |index: u32| {
            let phase = index as f32 / count as f32;
            let swing = if phase < 0.5 {
                phase * 2.0
            } else {
                2.0 - phase * 2.0
            };
            let centre = side as f32 * (0.2 + 0.6 * swing);
            let mut rgba = Vec::with_capacity((side * side * 4) as usize);
            for y in 0..side {
                for x in 0..side {
                    let inside = (x as f32 - centre).hypot(y as f32 - side as f32 / 2.0)
                        < side as f32 * 0.15;
                    let [r, g, b] = if inside { colour } else { [30, 31, 36] };
                    rgba.extend([r, g, b, 255]);
                }
            }
            rgba
        };
        let frames: Vec<Vec<u8>> = (0..count).map(frame).collect();
        let still = image::Handle::from_rgba(side, side, frames[0].clone());
        let delays = vec![100; count as usize];
        moving::register(&still, Arc::new(Frames::new(side, side, frames, delays, 0)));
        still
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bouncing_dot_moves() {
        let still = bouncing(120, 12, [255, 191, 71]);
        let moving = moving::find(&still).expect("registered");
        assert_eq!(moving.delays(), [100; 12]);
        let frames: Vec<_> = moving.frames().collect();
        assert_ne!(frames[0], frames[3]);
    }
}
