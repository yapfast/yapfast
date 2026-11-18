//! The profile editor: what it shows and what it asks for. Task P14
//! (docs/iced/tasks/people.md), after `ProfileEditorDialog`
//! (`ui/server.slint:1585`) and the `ProfileEditorPrompt` (`:613`) that
//! `show_profile_editor` fills (`src/ui/refresh.rs:3246`).
//!
//! The editor shows one profile as its card would wear it ([`Editor::card`],
//! which the app builds from the edited texts and pictures as Discord's
//! preview does), with the texts as written beside it, and the styles panel
//! on the left. What only the editor's screen knows (which text is being
//! edited, the banner colour's box, the panel folded) is held here too: the
//! app keeps it across refreshes and clears it with the editor.

use crate::kind::{Avatar, Nameplate, Tag};
use crate::menu::Spot;
use crate::people::profile::Card;

/// A text of the profile that can be written
/// (`ProfileText`, `src/ui/callbacks.rs:1521`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Pronouns,
    Bio,
    /// The banner colour, as a hex code.
    Colour,
}

/// A picture of the profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picture {
    Avatar,
    Banner,
}

/// A theme colour's handle on its tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Top,
    Bottom,
}

/// What this account may change: the nickname needs permission in a server,
/// the rest Nitro, as Discord has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct May {
    pub name: bool,
    pub avatar: bool,
    pub banner: bool,
    pub bio: bool,
    /// The theme and the name style.
    pub look: bool,
    /// The banner colour, which the main profile has without Nitro.
    pub colour: bool,
}

/// A server whose tag can be worn: its icon, name and tag, as the
/// chooser's row shows them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub id: String,
    pub name: String,
    pub icon: Avatar,
    pub tag: Tag,
}

impl Choice {
    /// Whether its name or tag holds what was searched for, as Slint's
    /// `NameStyles.contains` matched them: in any case, the search trimmed.
    pub fn found(&self, search: &str) -> bool {
        let search = search.trim().to_lowercase();
        self.name.to_lowercase().contains(&search) || self.tag.text.to_lowercase().contains(&search)
    }
}

/// The server tag's chooser, while open: the chip it opened under, in the
/// window, and what its search holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TagMenu {
    pub at: iced::Rectangle,
    pub search: String,
}

/// The profile editor: what its screen draws. The window holds it as
/// `vm.people.editor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Editor {
    /// The live preview beside the rows, and its colours: P15. Wired by F01; keep it.
    pub preview: super::editor_preview::EditorPreview,
    /// The profile as it would show: texts as written (or what they fall
    /// back to), pictures chosen, theme, name style, tag, badges, status.
    pub card: Card,
    /// "Edit profile" or "Edit server profile", and which profile it is.
    pub title: String,
    pub detail: String,
    /// It edits the main profile, not a server's.
    pub main: bool,
    /// The texts as written, which the card shows with fallbacks.
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    pub colour: String,
    /// What shows while a text is empty: the username, the pronouns the
    /// main profile has, the bio's invitation.
    pub name_placeholder: String,
    pub pronouns_fallback: String,
    pub bio_placeholder: String,
    /// Discord sends the profile; Discord takes the change.
    pub loading: bool,
    pub saving: bool,
    pub error: String,
    pub may: May,
    /// Discord's Nitro offer over a server's fields, and whether it asks a
    /// lesser Nitro to upgrade.
    pub offer: bool,
    pub upgrade: bool,
    /// The banner shows with the Nitro it takes, and the colour of the main
    /// profile without.
    pub banner_shown: bool,
    /// The colour as written isn't one.
    pub colour_wrong: bool,
    /// There is an avatar or banner to remove (main) or reset (a server).
    pub avatar_reset: bool,
    pub banner_reset: bool,
    /// A chosen picture is being read.
    pub reading: Option<Picture>,
    /// How many characters the bio has left; below 0, too many.
    pub bio_left: i32,
    /// The name or pronouns are longer than Discord takes.
    pub too_long: String,
    /// Saving would change something.
    pub changed: bool,
    /// How many times leaving was refused for the changes: the bar shakes
    /// red each time it goes up.
    pub nudge: u32,
    /// What the account wears that only Discord's shop sets.
    pub decoration_set: bool,
    pub effect_set: bool,
    pub frame_set: bool,
    /// The account's equipped nameplate.
    pub nameplate: Nameplate,
    /// The servers whose tags can be worn, and the one chosen.
    pub tags: Vec<Choice>,
    pub tag_chosen: String,
    /// The tags' chooser, while open.
    pub tag_menu: Option<TagMenu>,
    /// Which text is being edited in place.
    pub editing: Option<Field>,
    /// The banner colour's box shows under the banner.
    pub colour_open: bool,
    /// The styles panel is out: it starts folded in a window too narrow for
    /// it beside the profile and its tabs.
    pub side_open: bool,
    /// Which theme colour's picker is open (P15's card, which the editor
    /// draws beside the handle).
    pub handle: Option<Handle>,
    /// Where the clicked handle's middle is, from the window's top
    /// (`theme-handle-y`): the picker's middle goes there.
    pub handle_y: f32,
}

impl Editor {
    /// Nothing can be changed while Discord sends or takes the profile.
    pub fn ready(&self) -> bool {
        !self.loading && !self.saving
    }

    /// The banner can be clicked: allowed, no read under way.
    pub fn banner_editable(&self) -> bool {
        self.ready() && (self.may.banner || self.may.colour) && self.reading.is_none()
    }

    /// The banner shows but this account lacks the Nitro for it.
    pub fn banner_locked(&self) -> bool {
        self.banner_shown && !self.may.banner
    }

    pub fn avatar_editable(&self) -> bool {
        self.ready() && self.may.avatar && self.reading.is_none()
    }

    /// Saving may be asked for (`may-save`).
    pub fn may_save(&self) -> bool {
        self.ready()
            && self.reading.is_none()
            && self.too_long.is_empty()
            && self.bio_left >= 0
            && !self.colour_wrong
    }
}

/// The profile editor: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A text changed (`write`, `src/ui/callbacks.rs:1521`).
    Write(Field, String),
    /// A text was clicked: it becomes a field. `None` ends the editing.
    Edit(Option<Field>),
    /// The banner colour's box opened or closed.
    ColourBox(bool),
    /// Choose a picture, or go back to the fallback's.
    Pick(Picture),
    Reset(Picture),
    /// A picture was clicked where it has one to remove: the app opens its
    /// menu (Change, Remove or Reset) under it.
    PictureMenu(Picture, Spot),
    /// The server tag's chooser opens under its chip; each row sends
    /// [`Intent::ChooseTag`] (empty for none), which closes it.
    TagMenu(Spot),
    /// What its search holds.
    TagSearch(String),
    /// A press beside it, or Escape.
    CloseTagMenu,
    ChooseTag(String),
    /// The card's "More" menu: Copy User ID.
    MoreMenu(Spot),
    CopyId,
    /// A theme colour's handle was clicked, its middle this far from the
    /// window's top: the app sets `handle` and `handle_y` and starts
    /// `preview.colour` on that colour (P15's picker).
    Handle(Handle, f32),
    /// A press beside the open picker: the app clears `handle`.
    CloseHandle,
    /// The name style window (P16).
    OpenStyle,
    /// The status thought was clicked: change the custom status.
    EditStatus,
    /// The note was clicked.
    EditNote,
    /// The shop, the connections page and Nitro's offer open in a browser.
    OpenLink(String),
    UnlockNitro,
    /// Fold or bring back the styles panel.
    Side(bool),
    /// Undo every change, as the bar's Reset does.
    Undo,
    Save,
    Close,
}
