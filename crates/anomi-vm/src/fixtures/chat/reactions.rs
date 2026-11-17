//! Samples of a message's reactions, and the Reactions window for tests and
//! shots, after the fake Discord's data: task C03's.

use iced::Color;

use crate::Vm;
use crate::chat::reactions::{Chip, Dialog, Face, Pill, Reactions, Reactor};
use crate::fixtures::chat::timeline;
use crate::fixtures::pictures::pictured;
use crate::fixtures::text::{PARTY, TALK, badge};
use crate::fixtures::{avatar, shell};
use crate::kind::Expected;
use crate::shell::Layer;

/// The message the samples sit under.
pub const ITEM: u64 = 7;

fn chip(key: &str, name: &str, count: u64, mine: bool) -> Chip {
    Chip {
        key: key.into(),
        face: Face::Glyphs(key.into()),
        name: name.into(),
        count,
        mine,
        burst: None,
        who: String::new(),
    }
}

fn custom(id: &str, name: &str, count: u64, mine: bool) -> Chip {
    Chip {
        key: format!("{name}:{id}"),
        face: Face::Picture(Expected::Ready(badge(id))),
        name: format!(":{name}:"),
        count,
        mine,
        burst: None,
        who: String::new(),
    }
}

/// Seven reactions: two of this account's, three custom emoji (one whose picture
/// is on its way), a super reaction and a count of three digits.
pub fn chips() -> Reactions {
    let mut waiting = custom("1400000000000000999", "wave", 3, false);
    waiting.face = Face::Picture(Expected::Waiting);
    let mut seen = chip("\u{1F44D}", ":thumbsup:", 12, true);
    seen.who = "Alice, Bob, and 10 others reacted by :thumbsup:".into();
    Reactions {
        item: ITEM,
        chips: vec![
            seen,
            chip("\u{1F602}", ":joy:", 2, false),
            custom(TALK, "talk", 1, false),
            custom(PARTY, "party", 4, true),
            waiting,
            Chip {
                burst: Some(Color::from_rgb8(0xF4, 0x90, 0x0C)),
                ..chip("\u{1F525}", ":fire:", 5, false)
            },
            chip("\u{2764}\u{FE0F}", ":heart:", 128, false),
        ],
        enabled: true,
    }
}

/// The same where this account may not react.
pub fn locked() -> Reactions {
    Reactions {
        enabled: false,
        ..chips()
    }
}

fn person(name: &str, removable: bool) -> Reactor {
    Reactor {
        id: name.to_lowercase(),
        name: name.into(),
        username: format!("{}_user", name.to_lowercase()),
        avatar: pictured(avatar(&name[..1], 0x5865F2), name),
        removable,
    }
}

/// The window on three reactions, the first chosen, a page of people.
pub fn dialog() -> Dialog {
    let pill = |key: &str, count| Pill {
        key: key.into(),
        face: Some(Face::Glyphs(key.into())),
        name: String::new(),
        count,
    };
    Dialog {
        pills: vec![
            pill("\u{1F44D}", 12),
            pill("\u{1F602}", 2),
            Pill {
                key: format!("talk:{TALK}"),
                face: Some(Face::Picture(Expected::Ready(badge(TALK)))),
                name: String::new(),
                count: 1,
            },
        ],
        chosen: "\u{1F44D}".into(),
        people: vec![
            person("Alice", true),
            person("Bob", false),
            person("Carol", false),
            person("Erin", true),
        ],
        ..Dialog::default()
    }
}

/// The next page is on its way.
pub fn loading() -> Dialog {
    Dialog {
        loading: true,
        more: true,
        ..dialog()
    }
}

/// The people could not be fetched.
pub fn failed() -> Dialog {
    Dialog {
        people: Vec::new(),
        error: Some("Couldn't load the reactions".into()),
        ..dialog()
    }
}

/// A poll's voters, by answer, in wide tabs.
pub fn voters() -> Dialog {
    let answer = |key: &str, glyph: Option<&str>, name: &str, count| Pill {
        key: key.into(),
        face: glyph.map(|glyph| Face::Glyphs(glyph.into())),
        name: name.into(),
        count,
    };
    Dialog {
        pills: vec![
            answer("1", Some("\u{1F355}"), "Pizza", 5),
            answer("2", None, "Sushi, the long way round to the harbour", 3),
        ],
        voters: true,
        chosen: "1".into(),
        people: vec![person("Alice", false), person("Bob", false)],
        ..dialog()
    }
}

/// #general with chips under its last two messages.
pub fn in_channel() -> Vm {
    let mut vm = shell::server_home();
    vm.chat.timeline = timeline::general();
    let rows = &mut vm.chat.timeline.rows;
    let last = rows.len();
    rows[last - 1].message.reactions = chips();
    rows[last - 2].message.reactions = Reactions {
        chips: chips().chips[..2].to_vec(),
        ..chips()
    };
    vm
}

/// The Reactions window over it.
pub fn window(dialog: Dialog) -> Vm {
    let mut vm = shell::open(in_channel(), Layer::Reactions);
    vm.chat.reactions = dialog;
    vm
}
