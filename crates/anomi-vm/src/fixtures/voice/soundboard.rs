//! Samples of the soundboard for tests and shots, after the fake Discord's
//! data: task V12's. The Lounge's call as `voice/soundboard.png` has it.

use iced::{Point, Rectangle, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::kind::Expected;
use crate::menu::Spot;
use crate::shell::Layer;
use crate::voice::controls::MENU_SHIFT;
use crate::voice::soundboard::{Emoji, Sound, Soundboard};

use super::super::{pictures, shell};
use super::controls;

fn sound(id: &str, name: &str, emoji: &str, favorite: bool) -> Sound {
    Sound {
        id: id.into(),
        name: name.into(),
        emoji: if emoji.is_empty() {
            Emoji::None
        } else {
            Emoji::Glyph(emoji.into())
        },
        favorite,
        hear: tr!("Preview {}", name),
    }
}

/// A sound whose custom emoji is its picture.
fn custom(id: &str, name: &str, favorite: bool, picture: Expected) -> Sound {
    Sound {
        emoji: Emoji::Custom(picture),
        ..sound(id, name, "", favorite)
    }
}

fn applause() -> Sound {
    sound("1", "Applause", "\u{1f44f}", true)
}

fn airhorn() -> Sound {
    sound("2", "airhorn", "\u{1f50a}", true)
}

/// The favourites, the Lounge's server's sounds and Discord's own, with a
/// custom emoji in the server's, as `voice/soundboard.png` shows.
pub fn full() -> Soundboard {
    let mut board = Soundboard::default();
    board.section(tr!("Favorites"), [applause(), airhorn()]);
    board.section(
        "Server",
        [
            custom(
                "3",
                "Bob's laugh",
                false,
                Expected::Ready(pictures::badge("1400000000000000300", 36)),
            ),
            applause(),
        ],
    );
    board.section(
        tr!("Soundboard" => "Default"),
        [
            sound("4", "quack", "\u{1f986}", false),
            airhorn(),
            sound("5", "cricket", "\u{1f997}", false),
            sound("6", "golf clap", "\u{26f3}", false),
            sound("7", "sad horn", "\u{1f3ba}", false),
            sound("8", "ba dum tss", "\u{1f941}", false),
        ],
    );
    board
}

/// Before the sounds arrive: the title and "Loading sounds...".
pub fn loading() -> Soundboard {
    Soundboard::default()
}

/// A custom emoji's picture still on its way: its room kept, empty.
pub fn waiting() -> Soundboard {
    let mut board = Soundboard::default();
    board.section(
        "Server",
        [
            custom("3", "Bob's laugh", false, Expected::Waiting),
            sound("9", "no emoji", "", false),
        ],
    );
    board
}

/// Deafened: the note says why, and the sounds are dimmed.
pub fn deafened() -> Soundboard {
    Soundboard {
        note: tr!("Soundboard disabled when deafened").into(),
        ..full()
    }
}

/// Settings' picker of an entrance sound: its own title, nothing to hear or
/// star.
pub fn chooser() -> Soundboard {
    Soundboard {
        title: tr!("Choose an entrance sound").into(),
        chooser: true,
        drops: true,
        ..full()
    }
}

/// More sounds than fit: the list scrolls.
pub fn long() -> Soundboard {
    let mut board = full();
    let more: Vec<Sound> = (0..12)
        .map(|n| sound(&format!("x{n}"), &format!("Sound {n}"), "\u{1f50a}", false))
        .collect();
    board.section("More", more);
    board
}

/// Where the call's soundboard button sits in the golden window.
pub fn button() -> Rectangle {
    Rectangle::new(Point::new(833.0, 794.0), Size::new(44.0, 44.0))
}

/// The Lounge's window with `board` open above its soundboard button
/// (`voice/soundboard.png`).
pub fn window(board: Soundboard) -> Vm {
    let mut vm = controls::window(controls::joined());
    vm.voice.soundboard = board;
    let at = Rectangle {
        x: button().x - MENU_SHIFT,
        ..button()
    };
    shell::open_at(vm, Layer::Soundboard, Spot::above(at))
}
