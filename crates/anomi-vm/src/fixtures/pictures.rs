//! The fake Discord's pictures, drawn in code as `testing/fake_discord.ts`
//! draws them, so sample view-models carry pictures where the reference
//! screenshots show them: two tones split across the diagonal for avatars,
//! server icons and banners ([`picture`]; by name, [`person`] and
//! [`server`]), a ring for custom emoji and stickers ([`badge`]). A moving
//! picture needs the widgets' picture store:
//! `anomi_widgets::picture::sample::bouncing`. Task F05's.
//!
//! Each is made once a thread and handed out again, so its handle keeps its
//! id and the copies made of it are shared, as the app's pictures are.

use std::cell::RefCell;
use std::collections::HashMap;

use iced::widget::image;

use crate::kind::Avatar;

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

/// A server's icon as the fake serves it: `/icons/{id}/{hash}.png`.
pub fn icon(server: &str, hash: &str) -> image::Handle {
    picture(&format!("/icons/{server}/{hash}.png"))
}

/// The avatar of one of the fake's people, by the name a row shows: Alice,
/// Bob, Carol and Erin, and those its switches add (`Person N` of `--dms`,
/// `Member N` of `--members`, `Writer N` of `--authors`). None for those
/// without one (Dave, Free Nitro, a group), who show their initial.
pub fn person(name: &str) -> Option<image::Handle> {
    let numbered = |prefix: &str, base: u64, letter: char| {
        let n: u64 = name.strip_prefix(prefix)?.parse().ok()?;
        Some(avatar(&(base + n).to_string(), &format!("{letter}{n}")))
    };
    match name {
        "Alice" => Some(avatar("1400000000000000001", "alice1")),
        "Bob" => Some(avatar("1400000000000000002", "bob1")),
        "Carol" => Some(avatar("1400000000000000003", "carol1")),
        "Erin" => Some(avatar("1400000000000000005", "erin1")),
        _ => numbered("Person ", 1_600_000_000_000_000_000, 'p')
            .or_else(|| numbered("Member ", 1_750_000_000_000_000_000, 'm'))
            .or_else(|| numbered("Writer ", 1_700_000_000_000_000_000, 'w')),
    }
}

/// The icon of one of the fake's servers, by name: `Server` has one, the
/// fillers of `--servers` none.
pub fn server(name: &str) -> Option<image::Handle> {
    (name == "Server").then(|| icon("1400000000000000100", "server1"))
}

/// `avatar` with its picture, if the fake's person `name` has one.
pub fn pictured(mut avatar: Avatar, name: &str) -> Avatar {
    avatar.picture = person(name).or(avatar.picture);
    avatar
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
        let colour = ring_colour(id);
        draw(side, side, |x, y| ring(colour, side, x, y))
    })
}

/// A custom emoji twice as wide as it's tall: two of [`badge`]'s rings side
/// by side, which a square box would squeeze into ovals.
pub fn wide_badge(id: &str, height: u32) -> image::Handle {
    made(format!("wide badge {id} {height}"), || {
        let colour = ring_colour(id);
        draw(height * 2, height, |x, y| {
            ring(colour, height, x % height, y)
        })
    })
}

fn ring_colour(id: &str) -> [u8; 3] {
    let hue = crc32(id.as_bytes());
    [hue >> 16, hue >> 8, hue].map(|channel| 96 + ((channel & 255) % 160) as u8)
}

/// The pixel at `x`, `y` of a ring `side` across.
fn ring(colour: [u8; 3], side: u32, x: u32, y: u32) -> [u8; 4] {
    let half = side as f32 / 2.0;
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
    fn the_fakes_people_and_servers_have_theirs() {
        assert_eq!(
            person("Bob").map(|bob| bob.id()),
            Some(avatar("1400000000000000002", "bob1").id())
        );
        assert_eq!(
            person("Person 5").map(|five| five.id()),
            Some(avatar("1600000000000000005", "p5").id())
        );
        assert!(person("Dave").is_none() && person("Weekend plans").is_none());
        assert!(server("Server").is_some() && server("F2").is_none());
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
}
