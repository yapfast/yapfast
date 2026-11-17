//! Samples of editing a message in place for tests and shots, after the fake
//! Discord's data: task C13's. Alice's "See you at nine" closes Bob's
//! history in #general, and she edits it: with Bob mentioned and marked in
//! the Mods' blue, empty (the placeholder shows), typing "@Bo" with Bob and
//! the app offered over the field, or ":par" with the emoji.

use crate::Vm;
use crate::chat::inline_edit::InlineEdit;
use crate::chat::message::Message;
use crate::chat::timeline::group::key;
use crate::chat::timeline::{Row, Timeline};
use crate::fixtures::chat::message::{self as messages, compact as compact_message};
use crate::fixtures::shell;
use crate::fixtures::text::MODS;

use super::suggestions as offered;

/// The key of Alice's message, the one edited.
pub fn edited() -> u64 {
    key("edit")
}

/// The edit as it stands: "@Bob" marked in his role's colour.
pub fn open() -> InlineEdit {
    let mut edit = InlineEdit::editing(edited(), "@Bob see you at nine, at the old place");
    if let Some(field) = edit.field.as_mut() {
        field.marks = vec![(0..4, Some(MODS))];
    }
    edit
}

/// Everything deleted: the field says "Edit your message".
pub fn empty() -> InlineEdit {
    InlineEdit::editing(edited(), "")
}

/// Typing "@Bo": Bob and the app offered over the field.
pub fn suggesting() -> InlineEdit {
    let mut edit = InlineEdit::editing(edited(), "See you at nine, @Bo");
    if let Some(field) = edit.field.as_mut() {
        field.suggestions = offered::members();
    }
    edit
}

/// Typing ":par": the emoji offered over the field.
pub fn emoji() -> InlineEdit {
    let mut edit = InlineEdit::editing(edited(), "See you at nine :par");
    if let Some(field) = edit.field.as_mut() {
        field.suggestions = offered::emoji();
    }
    edit
}

/// Alice's message, sent and edited once already, as `edit` shows it.
pub fn message(edit: InlineEdit) -> Message {
    Message {
        key: edited(),
        time: "01.10.2026 16:57".into(),
        short_time: "16:57".into(),
        delivery: None,
        edited: true,
        editing: edit,
        ..messages::failed()
    }
}

/// Bob's history with Alice's message after it, being edited as `edit`
/// says; the timeline keeps it in view.
pub fn timeline(edit: InlineEdit) -> Timeline {
    let mut timeline = messages::history();
    let message = message(edit);
    timeline.rows.push(Row {
        key: message.key,
        first: true,
        message,
        ..Row::default()
    });
    timeline.shown = None;
    timeline.editing = Some(edited());
    timeline
}

/// #general with `edit` open on Alice's message.
pub fn channel_with(edit: InlineEdit) -> Vm {
    let mut vm = shell::server_home();
    vm.chat.timeline = timeline(edit);
    vm
}

/// The edit with Bob marked in it.
pub fn channel() -> Vm {
    channel_with(open())
}

/// Bob and the app offered over the field.
pub fn channel_suggesting() -> Vm {
    channel_with(suggesting())
}

/// The emoji offered over the field.
pub fn channel_emoji() -> Vm {
    channel_with(emoji())
}

/// The compact display: the body beside the name hides as the field opens.
pub fn channel_compact() -> Vm {
    let mut vm = channel();
    let timeline = &mut vm.chat.timeline;
    timeline.compact = true;
    for row in &mut timeline.rows {
        row.message = compact_message(row.message.clone());
    }
    vm
}
