//! Samples of a profile's badges for tests and shots, after the fake
//! Discord's data: task P08's.

use iced::widget::image;

use crate::fixtures::pictures::picture;
use crate::kind::Expected;
use crate::people::badges::{Badge, Badges, Level};

/// A badge drawn as the fake serves badge icons: `/badge-icons/{icon}.png`.
fn plain(icon: &str, description: &str, link: &str) -> Badge {
    Badge {
        icon: Expected::Ready(picture(&format!("/badge-icons/{icon}.png"))),
        description: description.to_owned(),
        link: link.to_owned(),
        level: None,
    }
}

/// The fake's level picture: a 156x100 rounded grey-blue rectangle (#7a8a99).
fn level_picture() -> image::Handle {
    thread_local! {
        static MADE: image::Handle = paint();
    }
    MADE.with(Clone::clone)
}

fn paint() -> image::Handle {
    const WIDTH: u32 = 156;
    const HEIGHT: u32 = 100;
    const RADIUS: f32 = 16.0;
    const GREY: [u8; 3] = [0x7a, 0x8a, 0x99];
    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            // Distance outside the corner's circle, if in a corner.
            let dx = (RADIUS - (x as f32 + 0.5)).max((x as f32 + 0.5) - (WIDTH as f32 - RADIUS));
            let dy = (RADIUS - (y as f32 + 0.5)).max((y as f32 + 0.5) - (HEIGHT as f32 - RADIUS));
            let outside = dx > 0.0 && dy > 0.0 && dx.hypot(dy) > RADIUS;
            let alpha = if outside { 0 } else { 255 };
            rgba.extend([GREY[0], GREY[1], GREY[2], alpha]);
        }
    }
    image::Handle::from_rgba(WIDTH, HEIGHT, rgba)
}

/// Bob's three badges in the fake: one with a page, one plain, one that
/// levels up (`testing/fake_discord.ts:1078`).
pub fn few() -> Badges {
    Badges {
        badges: vec![
            plain(
                "8a88d63823d8a71cd5e390baa45efa02",
                "HypeSquad Bravery",
                "https://support.discord.com/hc/en-us/articles/360035962891",
            ),
            plain("6bdc42827a38498929a4920da12695d9", "Active Developer", ""),
            levelled(),
        ],
    }
}

/// A badge that levels up, with its card's three lines: Bob's Spielzeit.
pub fn levelled() -> Badge {
    Badge {
        level: Some(Level {
            kind: "Spielzeit".to_owned(),
            title: "EISERN".to_owned(),
            progress: "1.816 Stunden gespielt".to_owned(),
            picture: Expected::Ready(level_picture()),
        }),
        ..plain("208e1bb66e311286cae364bc6f19277a", "Spielzeit", "")
    }
}

/// `count` badges, the levelled one first, for rows that wrap at twelve.
pub fn many(count: usize) -> Badges {
    let mut badges = vec![levelled()];
    badges.extend((1..count).map(|n| plain(&format!("{n:032x}"), &format!("Badge {n}"), "")));
    badges.truncate(count);
    Badges { badges }
}

/// Badges whose pictures have not arrived: their places stay empty.
pub fn waiting() -> Badges {
    let mut badges = few();
    for badge in &mut badges.badges {
        badge.icon = Expected::Waiting;
    }
    badges
}
