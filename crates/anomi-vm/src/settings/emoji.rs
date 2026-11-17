//! A server's Emoji settings: what it shows and what it asks for. Task X10
//! (docs/iced/tasks/settings.md). The Slint side: `EmojiSection` and
//! `EmojiUploadDialog` (`ui/server_expressions.slint:90`, `:182`) over
//! `EmojiPrompt`.

use crate::kind::{Avatar, Expected};

/// One emoji of a table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub picture: Expected,
    /// Who uploaded it; empty when Discord doesn't say.
    pub uploader: String,
    pub uploader_avatar: Avatar,
    /// This account may rename and delete it.
    pub may_change: bool,
}

/// A table: Emoji, or Animated Emoji, under its heading and its slots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub heading: String,
    /// "3 of 50 slots".
    pub slots: String,
    pub rows: Vec<Row>,
}

/// The upload dialog: the picture as chat will show it, and its name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Upload {
    pub preview: Expected,
    pub name: String,
    pub saving: bool,
    /// The name is valid and a picture is chosen.
    pub may_finish: bool,
    pub error: String,
}

/// A server's Emoji settings: what its screen draws. The window holds it as
/// `vm.settings.emoji`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Emoji {
    /// Slots for each kind, for Discord's words above the tables.
    pub slots: u32,
    /// Both tables, newest first.
    pub tables: Vec<Table>,
    /// Upload Emoji shows: this account may, and a slot is free.
    pub may_upload: bool,
    /// The id of the row under the pointer.
    pub hovered: Option<String>,
    /// A name being typed: the row's id and the text.
    pub editing: Option<(String, String)>,
    /// The upload dialog, while open (`dialog::dialog`).
    pub upload: Option<Upload>,
}

/// A server's Emoji settings: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The pointer entered (`true`) or left (`false`) the row with this id;
    /// a field getting the keys enters its row. Leaving clears `hovered`
    /// only while it is still that row, as the next row's entering may come
    /// first.
    Hover(String, bool),
    /// A row's name field changed: its id and the text.
    Edit(String, String),
    /// Rename this emoji by what was typed (Enter, or the field lost focus);
    /// nothing when `editing` isn't this row or holds its name unchanged.
    Rename(String),
    /// Delete this emoji, by id.
    Delete(String),
    /// Open the upload dialog.
    Upload,
    /// The dialog's name changed.
    Name(String),
    /// Upload the emoji.
    Finish,
    /// Close the dialog.
    Cancel,
}
