//! Samples of a message's gallery, files and stickers for tests and shots,
//! after the fake Discord's data: task C04's. Pictures are still ones; the
//! moving samples live in the screens crate's `shots()`.

use crate::chat::attachments::{Attachments, Cell, File, Gallery, Gif, Sticker};
use crate::chat::message::Message;
use crate::chat::timeline::group::key;
use crate::chat::timeline::{Row, Timeline};
use crate::chat::video::Video;
use crate::chat::voice_message::VoiceMessage;
use crate::fixtures::chat::message::{bobs, lit};
use crate::fixtures::chat::timeline as rows;
use crate::fixtures::chat::voice_message;
use crate::fixtures::pictures;
use crate::fixtures::text as fixtures_text;
use crate::kind::Expected;
use crate::text::Source;

/// The key the samples' messages carry, which the intents name.
pub const ITEM: u64 = 7;

fn photo(index: usize, name: &str, width: u32, height: u32) -> Cell {
    Cell {
        index,
        name: name.into(),
        picture: Expected::Ready(pictures::picture(&format!("photos/{name}"))),
        width,
        height,
        ..Cell::default()
    }
}

fn with(gallery: Vec<Cell>) -> Attachments {
    Attachments {
        item: ITEM,
        gallery: Gallery::mosaic(gallery),
        ..Attachments::default()
    }
}

/// Three photos: the first tall on the left, two on the right.
pub fn gallery() -> Attachments {
    with(vec![
        photo(0, "lake.jpg", 0, 0),
        photo(1, "hills.jpg", 0, 0),
        photo(2, "town.jpg", 0, 0),
    ])
}

/// Two photos side by side.
pub fn pair() -> Attachments {
    with(vec![
        photo(0, "lake.jpg", 0, 0),
        photo(1, "hills.jpg", 0, 0),
    ])
}

/// Five: a row of its own above the pairs.
pub fn five() -> Attachments {
    with(
        (0..5)
            .map(|i| photo(i, &format!("shot{i}.jpg"), 0, 0))
            .collect(),
    )
}

/// One picture at its own size.
pub fn single() -> Attachments {
    with(vec![photo(0, "lake.jpg", 400, 300)])
}

/// A picture that has not come: its skeleton at the size it will take.
pub fn loading() -> Attachments {
    let mut cell = photo(0, "lake.jpg", 400, 300);
    cell.picture = Expected::Waiting;
    with(vec![cell])
}

/// A picture under its spoiler.
pub fn spoiler() -> Attachments {
    let mut cell = photo(0, "secret.jpg", 400, 300);
    cell.spoiler = true;
    with(vec![cell])
}

/// A video, its poster the picture.
pub fn video() -> Attachments {
    let mut cell = photo(0, "clip.mp4", 400, 225);
    cell.video = Some(Video::default());
    with(vec![cell])
}

/// A GIF, kept among the favourites or not.
pub fn gif(favorite: bool) -> Attachments {
    let mut cell = photo(0, "dance.gif", 320, 240);
    cell.gif = Some(Gif { favorite });
    with(vec![cell])
}

/// A file card, a file without a size, and a voice message.
pub fn files() -> Attachments {
    let file = |index, name: &str, detail: &str| File {
        index,
        name: name.into(),
        detail: detail.into(),
        voice: None,
    };
    Attachments {
        item: ITEM,
        files: vec![
            file(0, "trip-notes.pdf", "2.4 MB"),
            file(1, "archive.zip", ""),
            File {
                index: 2,
                name: "voice-message.ogg".into(),
                voice: Some(VoiceMessage {
                    item: ITEM,
                    ..voice_message::idle()
                }),
                ..File::default()
            },
        ],
        ..Attachments::default()
    }
}

/// Two stickers and one on its way.
pub fn stickers() -> Attachments {
    let sticker = |index, name: &str, picture| Sticker {
        index,
        name: name.into(),
        picture,
        width: 160,
        height: 160,
    };
    Attachments {
        item: ITEM,
        stickers: vec![
            sticker(0, "Wave", Expected::Ready(pictures::badge("wave", 160))),
            sticker(1, "Party", Expected::Ready(pictures::badge("party", 160))),
            sticker(2, "Coming", Expected::Waiting),
        ],
        ..Attachments::default()
    }
}

/// [`gallery`] with its message under the pointer: Save shows on each.
pub fn lit_gallery() -> Attachments {
    Attachments {
        lit: true,
        ..gallery()
    }
}

/// The same, for a message still going out: nothing saves.
pub fn undelivered() -> Attachments {
    Attachments {
        undelivered: true,
        lit: true,
        ..gallery()
    }
}

/// #general with Bob's messages carrying each kind in turn.
pub fn showcase() -> Timeline {
    timeline_of([
        ("Three from today", lit_gallery()),
        ("One picture", single()),
        ("Don't look", spoiler()),
        ("A clip", video()),
        ("Files", files()),
        ("Stickers", stickers()),
    ])
}

/// GIFs, kept and not, and the shapes of five, a pair and a picture on its
/// way, for the shots that play their pictures.
pub fn moving() -> Timeline {
    timeline_of([
        ("A GIF", gif(false)),
        ("A favourite", gif(true)),
        ("Two", pair()),
        ("Five", five()),
        ("Coming", loading()),
        ("Stickers", stickers()),
    ])
}

/// #general's showcase as the golden `chan/showcase_emoji_stickers_gif` has
/// it: Bob's spoilers and custom emoji, then his three stickers (the message
/// under the pointer), a GIF 120 px square and a covered picture, each
/// under the one before.
pub fn showcase_stickers() -> Timeline {
    let mut stuck = stickers();
    stuck.stickers[2].picture = Expected::Ready(pictures::badge("coming", 160));
    let mut moves = photo(0, "moves.gif", 120, 120);
    moves.gif = Some(Gif::default());
    let said = |words| crate::text::markdown(Source::new(words));
    let kinds = [
        (fixtures_text::ending(), Attachments::default()),
        (fixtures_text::ship_it(), Attachments::default()),
        (fixtures_text::jumbo(), Attachments::default()),
        (crate::text::Text::default(), stuck),
        (said("It moves"), with(vec![moves])),
        (said("Don't look"), spoiler()),
    ];
    let built = kinds
        .into_iter()
        .enumerate()
        .map(|(index, (body, attachments))| {
            let first = index == 0;
            let message = bobs(40 + index, first);
            let message = Message {
                body,
                attachments: Attachments {
                    item: message.key,
                    lit: index == 3,
                    ..attachments
                },
                time: "01:10".into(),
                short_time: "01:10".into(),
                ..message
            };
            let message = if index == 3 { lit(message) } else { message };
            Row {
                key: message.key,
                first,
                message,
                ..Row::default()
            }
        });
    Timeline {
        rows: built.collect(),
        shown: None,
        ..rows::general()
    }
}

/// #general with a message of Bob's for each of `kinds`, the first under
/// the pointer.
fn timeline_of<const N: usize>(kinds: [(&str, Attachments); N]) -> Timeline {
    let built = kinds
        .into_iter()
        .enumerate()
        .map(|(index, (said, attachments))| {
            let message = Message {
                key: key(said),
                body: crate::text::markdown(Source::new(said)),
                attachments: Attachments {
                    item: key(said),
                    ..attachments
                },
                ..bobs(30 + index, true)
            };
            let message = if index == 0 { lit(message) } else { message };
            Row {
                key: message.key,
                first: true,
                message,
                ..Row::default()
            }
        });
    Timeline {
        rows: built.collect(),
        shown: None,
        ..rows::general()
    }
}
