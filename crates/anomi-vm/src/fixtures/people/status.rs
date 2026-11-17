//! Samples of the status dialog and menu for tests and shots, after the fake
//! Discord's data: task P10's. The first is the golden screenshot's
//! (`status/custom_dialog.png`): Alice's status "Testing Yapfast" with the
//! test tube, set already.

use crate::account::{Choice, Game};
use crate::icon::Icon;
use crate::menu::Row;
use crate::people::status::{ClearAfter, Emoji, Intent, Menu, Status};
use yapfast_lang::tr;

/// The dialog as it opens over a status that is set.
pub fn set() -> Status {
    Status {
        text: "Testing Yapfast".into(),
        emoji: Some(Emoji::unicode("\u{1f9ea}", "Test tube")),
        has_custom: true,
        ..Status::default()
    }
}

/// The dialog as it opens with no status: no emoji, no words, no "Clear
/// status".
pub fn empty() -> Status {
    Status::default()
}

/// A status that clears after an hour, with a server's emoji.
pub fn timed() -> Status {
    Status {
        emoji: Some(Emoji::custom(
            "talk:1",
            ":talk:",
            Some(crate::fixtures::pictures::badge("talk", 44)),
        )),
        clear_after: ClearAfter::AnHour,
        ..set()
    }
}

/// Saving: the buttons wait.
pub fn saving() -> Status {
    Status {
        saving: true,
        ..set()
    }
}

/// Discord refused.
pub fn failed() -> Status {
    Status {
        error: "Custom status is too long.".into(),
        ..set()
    }
}

/// The classic menu with a game going.
pub fn menu() -> Menu {
    Menu {
        choice: Choice::DoNotDisturb,
        rows: rows(true),
        game: Some(Game {
            name: "Factorio".into(),
            duration: "47 minutes".into(),
        }),
    }
}

/// The menu before the game's duration is known.
pub fn menu_just_started() -> Menu {
    Menu {
        game: Some(Game {
            name: "Factorio".into(),
            duration: String::new(),
        }),
        rows: rows(false),
        ..Menu::default()
    }
}

/// The menu's two rows, by whether a custom status is set.
pub fn rows(has_custom: bool) -> Vec<Row<Intent>> {
    vec![
        Row::new(
            if has_custom {
                tr!("Change custom status")
            } else {
                tr!("Set a custom status")
            },
            Intent::CustomStatus,
        )
        .icon(Icon::SmilePlus),
        Row::new(tr!("Edit Profile"), Intent::EditProfile).icon(Icon::UserPen),
    ]
}
