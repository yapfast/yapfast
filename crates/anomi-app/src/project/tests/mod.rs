//! The tests of the projection's row builders and wording, ported from the
//! Slint app's `src/ui/tests.rs` (task Q01, docs/iced/tasks/integration.md):
//! one module per area, each test named as the Slint one it ports. What an
//! area's own tests already cover isn't repeated here.

mod blocks;
mod cards;
mod chat;
mod friends;
mod people;
mod places;
mod wording;

use std::collections::HashSet;

use yapfast_core::application::{Action, AppState, Application};
use yapfast_core::chat::{Attachment, ChatMessage, Placement, Reacts};

use anomi_vm::chat::message::{Message, Size};

use super::chat::kept::Kept;
use super::chat::message::{self, Context, Look};

/// The preview's state: its rooms are the patterns rooms are made from.
fn preview() -> Application {
    let mut core = Application::default();
    drop(core.dispatch(Action::Preview));
    core
}

/// How rows look with the settings left alone, in a channel this account
/// writes and reacts in.
fn look(state: &AppState) -> Look {
    Look {
        twelve_hour: false,
        seconds: false,
        compact: false,
        size: Size::Normal,
        link_previews: true,
        collapse_long: false,
        cosmetics: state.cosmetics,
        in_server: false,
        preview: false,
        writes: true,
        reacts: Reacts::Any,
        forwards: true,
        threads: true,
        today: None,
        date_format: None,
    }
}

/// `message`'s row, its spoilers in `revealed` shown (`message_row`).
fn row_revealed(state: &AppState, message: &ChatMessage, revealed: &[&str]) -> Message {
    let revealed: HashSet<String> = revealed.iter().map(|&item| item.to_owned()).collect();
    let look = look(state);
    let kept = Kept::default();
    let cx = Context {
        state,
        look: &look,
        revealed: &revealed,
        now: 0,
        kept: &kept,
    };
    message::build(&cx, message, 1)
}

/// `message`'s row with nothing revealed (`message_row_plain`).
fn row(message: &ChatMessage) -> Message {
    row_revealed(&AppState::default(), message, &[])
}

/// A file of a message, as the Slint tests write one.
fn attachment(name: &str, placement: Placement) -> Attachment {
    Attachment {
        name: name.into(),
        size: None,
        image: false,
        dimensions: None,
        preview: None,
        preview_failed: false,
        preview_percent: 100,
        placement,
        gif: None,
        spoiler: false,
        video: false,
        url: None,
        description: None,
    }
}
