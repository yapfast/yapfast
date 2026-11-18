//! The message being edited in place, from the core's `edit()`:
//! `show_inline_edit` (`src/ui/refresh.rs:3915`). Its row shows the field
//! with the edit's text in its body's place, the timeline keeps it in view,
//! and as an edit starts its field takes the keys. The rows are built as
//! their messages change and come with no edit, so this runs after the
//! timeline's `show`, as its `light` does for the pointer's row: it touches
//! the edited row alone, and its text only when the core's changed.

use yapfast_core::application::AppState;
use yapfast_core::mention;

use anomi_vm::chat::composer::Mark;
use anomi_vm::chat::inline_edit::{Field, InlineEdit};
use anomi_vm::chat::timeline::{Timeline, group};

use crate::project::rows;

/// The edit shown last.
#[derive(Default)]
pub(crate) struct Shown {
    /// The edit's room and item, as it started.
    started: Option<(String, String)>,
    /// The row showing it, by key.
    row: Option<u64>,
}

/// Shows the edit in its row, and gives its field the keys as it starts.
pub(crate) fn show(
    state: &AppState,
    timeline: &mut Timeline,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    // The thread beside the channel edits in its own rows (I13).
    let edit = state
        .edit()
        .filter(|edit| state.selected.as_deref() == Some(edit.room.as_str()));
    let now = edit.map(|edit| (edit.room.as_str(), edit.item.as_str()));
    let was = shown
        .started
        .as_ref()
        .map(|(room, item)| (room.as_str(), item.as_str()));
    if now != was {
        if let Some(old) = shown.row.take() {
            close(timeline, old);
        }
        shown.started = edit.map(|edit| (edit.room.clone(), edit.item.clone()));
        shown.row = edit.and_then(|edit| {
            let message = state
                .messages
                .iter()
                .find(|found| found.item == edit.item)?;
            Some(group::key(&message.id))
        });
        // The field takes the keys as the edit starts, and only then.
        if shown.row.is_some() {
            *focus = Some(anomi_chat::inline_edit::FIELD_NAME);
        }
    }
    if timeline.editing != shown.row {
        timeline.editing = shown.row;
    }
    let (Some(edit), Some(key)) = (edit, shown.row) else {
        return;
    };
    let Some(row) = timeline.rows.iter_mut().find(|row| row.key == key) else {
        return;
    };
    match row.message.editing.field.as_deref_mut() {
        Some(field) if field.key == key => {
            if field.text != edit.text {
                field.text.clone_from(&edit.text);
                field.marks = marks(state, &edit.text);
            }
        }
        _ => {
            row.message.editing = InlineEdit {
                field: Some(Box::new(Field {
                    key,
                    text: edit.text.clone(),
                    marks: marks(state, &edit.text),
                    ..Field::default()
                })),
            };
        }
    }
}

/// The row `key` shows its message again, its edit over.
fn close(timeline: &mut Timeline, key: u64) {
    if let Some(row) = timeline.rows.iter_mut().find(|row| row.key == key)
        && row.message.editing.open()
    {
        row.message.editing = InlineEdit::default();
    }
}

/// The people and roles the edit mentions, coloured as the message will
/// show them (`mark_mentions_of(Field::Edit)`).
fn marks(state: &AppState, text: &str) -> Vec<Mark> {
    if !text.contains(['@', '#']) {
        return Vec::new();
    }
    mention::marks(text, &state.mention_candidates())
        .into_iter()
        .map(|(range, colour)| (range, colour.map(rows::rgb)))
        .collect()
}
