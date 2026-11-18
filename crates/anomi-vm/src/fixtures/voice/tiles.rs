//! Samples of a call's tiles for tests and shots, after the fake Discord's
//! Lounge (`testing/fake_discord.ts:2134`): Alice in the call, Bob sharing
//! his screen, Carol muted, Dave muted and deafened. Task V03's.
//!
//! Each golden screenshot of the stage has its tiles here in the stage's
//! order ([`lounge`], [`speaking`], [`watching`], [`with_camera`],
//! [`quiet`]), and
//! the states the goldens don't show have theirs ([`sharing`],
//! [`waiting`], [`offered`], [`starting`], [`joining`]). A stream's
//! preview and a camera's picture are drawn here once a thread, as the fake
//! draws them, so their handles keep their ids.

use std::cell::RefCell;
use std::collections::HashMap;

use iced::widget::image;
use yapfast_lang::tr;

use crate::fixtures::avatar;
use crate::fixtures::pictures::{picture, pictured};
use crate::kind::{Avatar, Live};
use crate::voice::tiles::{Chip, Fill, Key, Mark, Outline, Tile, Viewers};

const ALICE: &str = "1400000000000000001";
const BOB: &str = "1400000000000000002";
const CAROL: &str = "1400000000000000003";
const DAVE: &str = "1400000000000000004";
const ERIN: &str = "1400000000000000005";

pub fn alice_avatar() -> Avatar {
    pictured(avatar("A", 0x3a9357), "Alice")
}

pub fn bob_avatar() -> Avatar {
    pictured(avatar("B", 0xeb459e), "Bob")
}

/// Carol set another avatar for the server (`testing/fake_discord.ts:959`).
pub fn carol_avatar() -> Avatar {
    Avatar {
        picture: Some(picture(
            "/guilds/1400000000000000100/users/1400000000000000003/avatars/carol-server.png",
        )),
        ..avatar("C", 0x1abc9c)
    }
}

/// Dave has no avatar of his own: Discord's default for his id.
pub fn dave_avatar() -> Avatar {
    Avatar {
        picture: Some(picture("/embed/avatars/2.png")),
        ..avatar("D", 0x5865f2)
    }
}

/// Someone's own tile by participant id, showing their picture, outlined:
/// what every call's tiles start from.
pub fn person(id: &str, name: &str, avatar: Avatar) -> Tile {
    Tile {
        key: Key::Person(id.into()),
        fill: Fill::Avatar {
            avatar,
            connected: true,
        },
        outline: Outline::Line,
        chip: Chip {
            name: name.into(),
            ..Chip::default()
        },
        ..Tile::default()
    }
}

/// This account, listening.
pub fn alice() -> Tile {
    person(ALICE, "Alice", alice_avatar())
}

/// Bob sharing his screen without a camera: its preview fills his tile,
/// which a click watches.
pub fn bob() -> Tile {
    let mut bob = person(BOB, "Bob", bob_avatar());
    bob.fill = Fill::Preview(stream_preview());
    bob.watch = true;
    bob.chip.live = true;
    bob
}

pub fn carol() -> Tile {
    let mut carol = person(CAROL, "Carol", carol_avatar());
    carol.chip.mark = Some(Mark::Muted);
    carol
}

pub fn dave() -> Tile {
    let mut dave = person(DAVE, "Dave", dave_avatar());
    dave.chip.mark = Some(Mark::Deafened);
    dave
}

/// The Lounge's stage once Alice joined (`voice/stage.png`).
pub fn lounge() -> Vec<Tile> {
    vec![alice(), bob(), carol(), dave()]
}

/// The Lounge once Bob left (`light/voice_stage.png`).
pub fn quiet() -> Vec<Tile> {
    vec![alice(), carol(), dave()]
}

/// Bob speaks (`voice/speaking_ring.png`).
pub fn speaking() -> Vec<Tile> {
    let mut tiles = lounge();
    tiles[1].outline = Outline::Speaking;
    tiles
}

/// Alice watching, the one viewer.
fn alice_watches() -> Viewers {
    Viewers {
        faces: vec![alice_avatar()],
        count: 1,
        names: tr!("Watching: {}", "Alice"),
    }
}

/// Alice watches Bob's stream, large in his tile (`voice/watching_stream.png`).
pub fn watching() -> Vec<Tile> {
    let mut tiles = lounge();
    let bob = &mut tiles[1];
    bob.fill = Fill::Watched {
        frame: Some(Live::new(frame(640, 360))),
        status: String::new(),
    };
    bob.watch = false;
    bob.viewers = alice_watches();
    bob.focused = true;
    bob.can_hide_others = true;
    tiles
}

/// Bob films too: his stream takes a tile of its own, large and watched,
/// and his own tile shows his camera (`voice/with_camera.png`).
pub fn with_camera() -> Vec<Tile> {
    let stream = Tile {
        key: Key::Stream(BOB.into()),
        fill: Fill::Watched {
            frame: Some(Live::new(frame(640, 360))),
            status: String::new(),
        },
        chip: Chip {
            name: "Bob".into(),
            mark: Some(Mark::Screen),
            live: true,
            note: String::new(),
        },
        viewers: alice_watches(),
        focused: true,
        can_hide_others: true,
        ..Tile::default()
    };
    let mut bob = person(BOB, "Bob", bob_avatar());
    bob.fill = Fill::Camera(Live::new(frame(320, 180)));
    vec![stream, alice(), bob, carol(), dave()]
}

/// Alice shares her screen, live, Bob watching it: her screen's tile.
pub fn sharing() -> Tile {
    Tile {
        key: Key::OwnScreen,
        fill: Fill::OwnScreen {
            frame: Some(Live::new(frame(640, 360))),
            status: tr!("Live").into(),
        },
        chip: Chip {
            name: tr!("Your screen").into(),
            live: true,
            ..Chip::default()
        },
        viewers: Viewers {
            faces: vec![bob_avatar()],
            count: 1,
            names: tr!("Watching: {}", "Bob"),
        },
        ..Tile::default()
    }
}

/// Her stream on its way, before its first picture.
pub fn sharing_starting() -> Tile {
    Tile {
        fill: Fill::OwnScreen {
            frame: None,
            status: tr!("Starting your stream...").into(),
        },
        chip: Chip {
            live: false,
            ..sharing().chip
        },
        viewers: Viewers::default(),
        ..sharing()
    }
}

/// Bob's stream watched before its first picture, watched by five.
pub fn waiting() -> Tile {
    let mut bob = watching().swap_remove(1);
    bob.fill = Fill::Watched {
        frame: None,
        status: tr!("Connecting to {}'s stream...", "Bob"),
    };
    bob.focused = false;
    bob.can_hide_others = false;
    bob.viewers = Viewers {
        faces: vec![alice_avatar(), carol_avatar(), dave_avatar()],
        count: 5,
        names: tr!("Watching: {}", "Alice, Carol, Dave, Erin, Person 1"),
    };
    bob
}

/// Bob's stream in a tile of its own, not watched: with its preview, or
/// without one.
pub fn offered(preview: bool) -> Tile {
    Tile {
        fill: Fill::Offer {
            preview: preview.then(stream_preview),
        },
        watch: true,
        viewers: Viewers::default(),
        focused: false,
        can_hide_others: false,
        ..with_camera().swap_remove(0)
    }
}

/// Alice's camera on its way.
pub fn starting() -> Tile {
    Tile {
        starting: true,
        ..alice()
    }
}

/// Erin joining, not connected yet, and Carol, whose connection was lost.
pub fn joining() -> Vec<Tile> {
    let mut erin = person(ERIN, "Erin", pictured(avatar("E", 0xf47b67), "Erin"));
    if let Fill::Avatar { connected, .. } = &mut erin.fill {
        *connected = false;
    }
    let mut carol = carol();
    carol.chip.note = tr!("Connection lost").into();
    vec![erin, carol]
}

thread_local! {
    static MADE: RefCell<HashMap<(&'static str, u32, u32), image::Handle>> = RefCell::default();
}

fn made(
    name: &'static str,
    width: u32,
    height: u32,
    pixel: impl Fn(u32, u32) -> [u8; 4],
) -> image::Handle {
    let key = (name, width, height);
    if let Some(handle) = MADE.with_borrow(|made| made.get(&key).cloned()) {
        return handle;
    }
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend(pixel(x, y));
        }
    }
    let handle = image::Handle::from_rgba(width, height, rgba);
    MADE.with_borrow_mut(|made| made.insert(key, handle.clone()));
    handle
}

/// The preview of someone's stream as the fake draws it: a desktop with a
/// window open (`streamPreviewPicture`, `testing/fake_discord.ts:3885`).
pub fn stream_preview() -> image::Handle {
    made("preview", 512, 288, |x, y| {
        let (x, y) = (x as i32, y as i32);
        if (96..416).contains(&x) && (48..232).contains(&y) {
            if y < 68 {
                return [54, 57, 70, 255];
            }
            let line = (y - 84).div_euclid(18);
            let written =
                y >= 84 && (y - 84) % 18 < 8 && line < 7 && x >= 116 && x < 196 + (line * 67) % 190;
            return if written {
                [120, 126, 140, 255]
            } else {
                [242, 243, 245, 255]
            };
        }
        if y >= 264 {
            return [32, 34, 42, 255];
        }
        [(40 + x / 8) as u8, (110 + y / 4) as u8, 170, 255]
    })
}

/// A frame of Bob's recorded stream, or of a camera: colour bars, and the
/// dark box its clock is drawn in.
pub fn frame(width: u32, height: u32) -> image::Handle {
    const BARS: [[u8; 4]; 6] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [255, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 0, 255, 255],
        [0, 255, 255, 255],
    ];
    made("frame", width, height, |x, y| {
        let [r, g, b, a] = BARS[(x * 6 / width) as usize];
        if x < width * 3 / 10 && y < height / 5 {
            [r / 4, g / 4, b / 4, a]
        } else {
            [r, g, b, a]
        }
    })
}
