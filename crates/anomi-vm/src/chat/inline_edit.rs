//! Editing a message in place: what it shows and what it asks for. The
//! Slint side: `InlineEditor` and its global `InlineEdit`
//! (`ui/messages.slint:2873`, `:2837`), filled by `show_inline_edit`
//! (`src/ui/refresh.rs:4125`) and answered by `inline_edit`
//! (`src/ui/callbacks.rs:3966`) and `insert_edit_emoji` (`:4035`).
//!
//! Every message holds one ([`super::message::Message::editing`]); only the
//! message being edited has its [`Field`], so the rest hold a null pointer.
//! The row then shows, in its body's place, the field with the edit's text
//! and an emoji button, and under it "escape to cancel • enter to save",
//! whose two words are clicked too. The message's body, its "(edited)" and
//! the compact display's body beside the name hide meanwhile
//! ([`InlineEdit::open`]).
//!
//! The edit is the app's, not the row's: the text, its marks and the
//! suggestions over it come from here each time the row is built, so a row
//! the list builds again (scrolled away and back, its room opened again)
//! shows the edit as it was. The timeline keeps the row in view while it is
//! edited (`timeline::Timeline::editing`). The app gives the field the keys
//! with its caret at the end as an edit starts, and only then: a row built
//! again leaves the keys where they are (`focus-tick`, `focused-tick`).
//!
//! The suggestions over the field are the window's, as the composers' are:
//! the app shows them here while this field was typed in last
//! (`suggesting-edit`, `ui/yapfast.slint:4674`), and their keys come from it.

use super::composer::{Mark, Suggest};
use super::suggestions::Suggestions;
use crate::menu::Spot;

/// Editing a message in place: what its row draws. Closed for every message
/// but the one edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineEdit {
    /// The field, while this message is edited (`InlineEdit.item`).
    pub field: Option<Box<Field>>,
}

impl InlineEdit {
    /// An edit of the message `key` holding `text`, with nothing marked or
    /// suggested.
    pub fn editing(key: u64, text: impl Into<String>) -> Self {
        Self {
            field: Some(Box::new(Field {
                key,
                text: text.into(),
                ..Field::default()
            })),
        }
    }

    /// The message is being edited: its row shows the field in its body's
    /// place, and hides the body and its "(edited)" (`root.editing`,
    /// `ui/messages.slint:3082`).
    pub fn open(&self) -> bool {
        self.field.is_some()
    }
}

/// The field of the message being edited and what shows over it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Field {
    /// Whose edit this is, as the message's key: a new key shows its text
    /// afresh, and every change the field reports carries it.
    pub key: u64,
    /// The text being edited (`InlineEdit.text`), the message's own as the
    /// edit starts.
    pub text: String,
    /// The people and roles the text mentions, marked as the message will
    /// show them (`mark_mentions_of(Field::Edit)`, `src/ui/refresh.rs:4074`).
    pub marks: Vec<Mark>,
    /// Mentions or emoji offered over the field while it was typed in last
    /// (`suggesting`): Up, Down, Tab, Enter and Escape are theirs meanwhile.
    pub suggestions: Suggestions,
}

impl Field {
    /// Suggestions show over the field, and take its keys.
    pub fn suggesting(&self) -> bool {
        self.suggestions.showing()
    }
}

/// Editing a message in place: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The text changed (`edited`): whose edit, the new text and the caret's
    /// byte offset, which the `@`, `#` and `:` suggestions read.
    Edited {
        key: u64,
        text: String,
        caret: usize,
    },
    /// The caret moved alone (`caret`): the suggestions follow it, and an
    /// emoji from the picker goes in where it was (`edit_caret`).
    Caret { key: u64, caret: usize },
    /// Enter, or "save" under the field (`save`): the app sends the edit
    /// and closes the field (`Action::SaveEdit`).
    Save,
    /// Escape that no suggestion took, or "cancel" under the field
    /// (`cancel`): the field closes and the message stays as it was
    /// (`Action::CancelEdit`).
    Cancel,
    /// A key while suggestions show over the field (`suggestion-moved`,
    /// `suggestion-chosen`, `suggestion-dismissed`).
    Suggest(Suggest),
    /// A click on the suggestions over the field (C15's list:
    /// `mention-chosen`, `emoji-suggestion-chosen`): what is chosen goes in
    /// the edit's text where the word is.
    Offered(super::suggestions::Intent),
    /// The emoji button, from where it sits (`emoji`, `open-edit-picker`):
    /// the emoji picker opens beside it, and what is chosen goes in at the
    /// caret, the field taking the keys back as the picker closes
    /// (`insert_edit_emoji`, `refocus`).
    Emoji(Spot),
}
