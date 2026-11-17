//! A server's Stickers settings: what it shows and what it asks for. Task X11
//! (docs/iced/tasks/settings.md). The Slint side: `StickersSection` and
//! `StickerDialog` (`ui/server_expressions.slint:286`, `:341`) over
//! `StickersPrompt`.

use crate::kind::Expected;

/// One sticker on its tile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub id: String,
    pub name: String,
    /// A sticker that can't be drawn here (a Lottie one) stays `Waiting` or
    /// `None` and keeps a picture's place.
    pub picture: Expected,
    /// Who uploaded it; empty when Discord doesn't say.
    pub uploader: String,
    /// This account may edit and delete it.
    pub may_change: bool,
}

/// A boost level's card: its title, how many slots are used, and its tiles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub title: String,
    /// "2 of 5 slots".
    pub status: String,
    pub unlocked: bool,
    /// The next sticker goes here: Upload Sticker shows after the tiles.
    pub next: bool,
    pub tiles: Vec<Tile>,
}

/// The sticker form: a new sticker's file, related emoji, name and
/// description, or those of one edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Form {
    /// An edit can't change the file.
    pub editing: bool,
    /// The sticker as chat will show it (the edited one's, or the file's).
    pub preview: Expected,
    pub has_file: bool,
    pub file_name: String,
    pub tags: String,
    pub name: String,
    pub description: String,
    pub saving: bool,
    pub may_save: bool,
    pub error: String,
}

/// A server's Stickers settings: what its screen draws. The window holds it
/// as `vm.settings.stickers`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stickers {
    pub cards: Vec<Card>,
    /// Upload Sticker shows: this account may.
    pub may_upload: bool,
    /// The id of the tile under the pointer, [`UPLOAD`] for Upload Sticker.
    pub hovered: Option<String>,
    /// The form, while open (`dialog::dialog`).
    pub form: Option<Form>,
}

/// Upload Sticker's id in [`Stickers::hovered`]: no sticker's is empty.
pub const UPLOAD: &str = "";

impl Stickers {
    /// The pointer entered (`inside`) or left the tile with `id`
    /// ([`Intent::Hover`]). Leaving clears only that tile's hover: crossing
    /// to a tile before it enters that one before this one is left.
    pub fn hover(&mut self, id: String, inside: bool) {
        if inside {
            self.hovered = Some(id);
        } else if self.hovered.as_ref() == Some(&id) {
            self.hovered = None;
        }
    }
}

/// A server's Stickers settings: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The pointer entered (`true`) or left (`false`) the tile with this id
    /// ([`UPLOAD`] for Upload Sticker): [`Stickers::hover`].
    Hover(String, bool),
    /// Open the form for a new sticker.
    Upload,
    /// Open the form on a sticker, by id.
    Edit(String),
    /// Delete a sticker, by id.
    Delete(String),
    /// Choose the file to upload.
    Browse,
    /// The form's related emoji changed.
    Tags(String),
    /// The form's name changed.
    Name(String),
    /// The form's description changed.
    Description(String),
    /// Save the form.
    Save,
    /// Close the form.
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_a_tile_after_entering_the_next_keeps_the_next() {
        let mut page = Stickers::default();
        page.hover("2".into(), true);
        page.hover("1".into(), true);
        page.hover("2".into(), false);
        assert_eq!(page.hovered.as_deref(), Some("1"));
        page.hover("1".into(), false);
        assert_eq!(page.hovered, None);
    }
}
