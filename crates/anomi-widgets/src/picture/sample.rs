//! The fake Discord's pictures, drawn in code as `testing/fake_discord.ts`
//! draws them, so sample view-models carry pictures where the reference
//! screenshots show them: two tones split across the diagonal for avatars,
//! icons and banners (`picture`), a ring for custom emoji and stickers
//! (`badge`), and a dot going to and fro for the moving GIF (`bouncing`).
//!
//! Each is made once a thread and handed out again, so its handle keeps its
//! id and the copies made of it are shared, as the app's pictures are.
//! A screen's `shots()` fills its sample's pictures with these; `anomi-vm`'s
//! fixtures can't, since the vocabulary crate comes before the widgets.

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

/// A user's avatar as the fake serves it: `/avatars/{id}/{hash}.png`, as
/// Bob's `avatar("1400000000000000002", "bob1")`.
pub fn avatar(user: &str, hash: &str) -> image::Handle {
    picture(&format!("/avatars/{user}/{hash}.png"))
}

/// The picture the fake draws for an avatar, an icon or a banner at `path`:
/// a light tone above the diagonal and half of it below, picked from the
/// path's CRC; 128 pixels a side, or 600 by 240 for a banner or a splash.
pub fn picture(path: &str) -> image::Handle {
    made(format!("picture {path}"), || {
        let hue = crc32(path.as_bytes());
        let light = [hue >> 16, hue >> 8, hue].map(|channel| 96 + ((channel & 255) % 160) as u8);
        let dark = light.map(|channel| channel >> 1);
        let wide = path.contains("/banners/") || path.contains("/splashes/");
        let (width, height) = if wide { (600, 240) } else { (128, 128) };
        draw(width, height, |x, y| {
            let across = x as f32 / width as f32 + y as f32 / height as f32;
            let tone = if across < 1.0 { light } else { dark };
            [tone[0], tone[1], tone[2], 255]
        })
    })
}

/// A custom emoji's or sticker's picture as the fake draws it: a ring of
/// a colour picked from its id, white in the middle, clear around it.
pub fn badge(id: &str, side: u32) -> image::Handle {
    made(format!("badge {id} {side}"), || {
        let hue = crc32(id.as_bytes());
        let colour = [hue >> 16, hue >> 8, hue].map(|channel| 96 + ((channel & 255) % 160) as u8);
        let half = side as f32 / 2.0;
        draw(side, side, |x, y| {
            let distance = (x as f32 + 0.5 - half).hypot(y as f32 + 0.5 - half) / half;
            if distance > 1.0 {
                [0, 0, 0, 0]
            } else if distance > 0.8 {
                [colour[0] >> 1, colour[1] >> 1, colour[2] >> 1, 255]
            } else if distance < 0.35 {
                [255; 4]
            } else {
                [colour[0], colour[1], colour[2], 255]
            }
        })
    })
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

/// The Matrix fixture's sunset gradient the fake sends as a spoiler.
pub fn sunset(width: u32, height: u32) -> image::Handle {
    made(format!("sunset {width} {height}"), || {
        draw(width, height, |x, y| {
            let sx = x * 800 / width;
            let sy = y * 500 / height;
            [((sx / 4) & 255) as u8, ((sy / 3) & 255) as u8, 160, 255]
        })
    })
}

fn draw(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> image::Handle {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend(pixel(x, y));
        }
    }
    image::Handle::from_rgba(width, height, rgba)
}

/// CRC-32 as PNG and the fake compute it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(handle: &image::Handle, x: u32, y: u32) -> [u8; 4] {
        let image::Handle::Rgba { width, pixels, .. } = handle else {
            panic!("drawn as pixels");
        };
        let at = ((y * width + x) * 4) as usize;
        pixels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn the_crc_is_the_standard_one() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn an_avatar_is_light_above_the_diagonal_and_half_as_light_below() {
        let bob = avatar("1400000000000000002", "bob1");
        let (light, dark) = (pixel(&bob, 2, 2), pixel(&bob, 125, 125));
        assert_eq!(dark, [light[0] >> 1, light[1] >> 1, light[2] >> 1, 255]);
        assert!(light[..3].iter().all(|&channel| channel >= 96));
        assert_eq!(
            bob.id(),
            avatar("1400000000000000002", "bob1").id(),
            "made once"
        );
    }

    #[test]
    fn a_banner_is_wide() {
        let banner = picture("/banners/1400000000000000002/bob-banner.png");
        assert!(matches!(
            banner,
            image::Handle::Rgba {
                width: 600,
                height: 240,
                ..
            }
        ));
    }

    #[test]
    fn a_badge_is_clear_around_its_ring() {
        let ring = badge("1400000000000000300", 48);
        assert_eq!(pixel(&ring, 0, 0)[3], 0);
        assert_eq!(pixel(&ring, 24, 24), [255; 4]);
    }

    #[test]
    fn the_bouncing_dot_moves() {
        let still = bouncing(120, 12, [255, 191, 71]);
        let moving = moving::find(&still).expect("registered");
        assert_eq!(moving.delays(), [100; 12]);
        let frames: Vec<_> = moving.frames().collect();
        assert_ne!(frames[0], frames[3]);
    }
}
