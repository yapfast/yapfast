//! The fake Discord's profile effect, "Petals", drawn in code as
//! `effectPicture` in `testing/fake_discord.ts` draws it, for shots and
//! tests: Bob's, as `profile/popout_member.png` shows its still. Its
//! layers are an intro that plays once and a loop that waits a second
//! between rounds, timed as the fake's product (`PROFILE_EFFECT`).
//!
//! The still is the fake's own, 450 by 880, which the card draws at its
//! width. The layers' frames, which only a hovered card in the app shows,
//! are drawn at a fifth of that and five to a round, so a sample holds
//! half a MiB of them rather than the fake's 80.

use std::cell::OnceCell;
use std::sync::Arc;

use iced::widget::image;

use super::{Effect, EffectLayer};
use crate::picture::{Frames, register};

/// The fake's pictures' size.
const WIDTH: u32 = 450;
const HEIGHT: u32 = 880;
/// How much smaller the samples' moving layers are.
const SHRINK: u32 = 5;
const FRAMES: u32 = 5;

/// `crc32` of each picture's name, which shades its petals in the fake.
const STILL: u32 = 1_447_535_550;
const INTRO: u32 = 436_585_760;
const LOOP: u32 = 2_707_365_358;

thread_local! {
    static PETALS: OnceCell<Effect> = const { OnceCell::new() };
}

/// Bob's effect: the still, an intro from the start for two seconds, and
/// from 1.5 s a loop 20 px lower showing three seconds in every four. Made
/// once a thread, so its pictures keep their ids.
pub fn petals() -> Effect {
    PETALS.with(|made| made.get_or_init(make).clone())
}

fn make() -> Effect {
    let still = image::Handle::from_rgba(WIDTH, HEIGHT, petals_at(STILL, 0, WIDTH, HEIGHT));
    Effect {
        still: Some(still),
        layers: vec![
            EffectLayer {
                picture: Some(moving(INTRO)),
                duration: 2000,
                ..EffectLayer::default()
            },
            EffectLayer {
                picture: Some(moving(LOOP)),
                y: 20.0,
                start: 1500,
                duration: 3000,
                looped: true,
                loop_delay: 1000,
                ..EffectLayer::default()
            },
        ],
    }
}

/// A layer's picture, its petals falling a little each frame, round and
/// round as the fake's APNG.
fn moving(shade: u32) -> image::Handle {
    let (width, height) = (WIDTH / SHRINK, HEIGHT / SHRINK);
    let frames: Vec<Vec<u8>> = (0..FRAMES)
        .map(|frame| petals_at(shade, frame * 4, width, height))
        .collect();
    let still = image::Handle::from_rgba(width, height, frames[0].clone());
    let delay = 1000 / FRAMES;
    let delays = vec![delay; frames.len()];
    register(
        &still,
        Arc::new(Frames::new(width, height, frames, delays, 0)),
    );
    still
}

/// The fake's `petals(fall)`: clear but for a petal where its pattern
/// says, pink shaded by the picture's name. Its arithmetic is JavaScript's,
/// whose remainder keeps a negative's sign, as Rust's does.
fn petals_at(shade: u32, fall: u32, width: u32, height: u32) -> Vec<u8> {
    let pink = [255, 150 + (shade & 63) as u8, 190, 220];
    let offset = i64::from(shade & 255);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..i64::from(height) {
        for x in 0..i64::from(width) {
            let petal = (x * 7 + (y - i64::from(fall)) * 13 + offset) % 97 < 2 && (x + y) % 5 < 3;
            pixels.extend_from_slice(if petal { &pink } else { &[0; 4] });
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_still_is_the_fakes() {
        let effect = petals();
        let Some(image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        }) = &effect.still
        else {
            panic!("a still of pixels");
        };
        assert_eq!((*width, *height), (450, 880));
        // (0, 0): 190 % 97 = 93, clear. The top row's first petal is where
        // 7x + 190 lands on 0 or 1 past a multiple of 97, at x = 56.
        assert_eq!(&pixels[..4], &[0; 4]);
        let first = (0..450u32)
            .find(|&x| (x * 7 + 190) % 97 < 2 && x % 5 < 3)
            .expect("a petal on the top row");
        let at = (first * 4) as usize;
        assert_eq!(&pixels[at..at + 4], &[255, 212, 190, 220]);
    }

    #[test]
    fn the_layers_are_timed_as_the_fakes_product() {
        let effect = petals();
        assert_eq!(effect.end(), 4500);
        assert!(effect.loops());
        assert_eq!(petals(), effect, "made once a thread");
    }

    #[test]
    fn petals_above_the_fall_fill_the_top_left_as_the_fake_does() {
        // JavaScript's `%` of a negative is negative, so below 2: rows the
        // petals haven't reached yet are dense with them at the left.
        let pixels = petals_at(INTRO, 16, 90, 176);
        let (x, y) = (2usize, 0usize);
        assert!((x + y) % 5 < 3);
        assert_eq!(pixels[(y * 90 + x) * 4 + 3], 220);
    }
}
