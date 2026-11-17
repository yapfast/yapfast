//! A channel's settings frame and its overview: what it shows and what it
//! asks for (`ChannelSettingsPrompt`, `ui/channel_settings.slint:35`, filled
//! by `src/ui/channel_settings.rs`). Task X13
//! (docs/iced/tasks/settings.md).

use super::frame::Unsaved;
use super::permissions::Permissions;
use crate::icon::Icon;

/// A page of the channel's sections: what a click on a row chooses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Overview,
    Permissions,
    /// Delete Channel, last in the sections: it closes the settings and
    /// asks (`DELETE_CHANNEL`, `src/ui/channel_settings.rs:19`).
    Delete,
}

/// A row of the sections: a group's heading, the line before the dangerous
/// one, or a page.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Heading(String),
    Line,
    Page {
        text: String,
        icon: Icon,
        page: Page,
        danger: bool,
    },
}

/// What the settings are of: a text channel has a topic, a slowmode and an
/// age restriction, a voice channel and a category only a name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    Voice,
    Category,
}

/// How many slowmodes Discord offers (`SLOWMODES`): off, 5 s up to 6 h.
pub const SLOWMODES: usize = 14;

/// A channel's settings frame and its overview: what its screen draws. The
/// window holds it as `vm.settings.channel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {
    /// The sections, headed by the channel's name in capitals.
    pub rows: Vec<Row>,
    pub page: Page,
    pub kind: Kind,
    /// The page's title over its column: "Overview", or the permissions'.
    pub title: String,
    /// The close button's tooltip.
    pub close: String,
    /// The channel's settings are on their way.
    pub loading: bool,
    /// What Discord refused, and why a draft can't be saved, in red.
    pub error: String,
    pub problem: String,
    /// The draft is on its way: its fields wait.
    pub saving: bool,
    /// The bar for unsaved changes, while the draft differs or is sent.
    pub unsaved: Option<Unsaved>,
    pub name: String,
    pub topic: String,
    /// An index of the slowmodes, below [`SLOWMODES`].
    pub slowmode: usize,
    pub nsfw: bool,
    /// The Permissions page, which X14 draws in this frame.
    pub permissions: Permissions,
}

impl Channel {
    /// Whether the page is the overview of a text channel's extras: topic,
    /// slowmode and age restriction.
    pub fn is_text(&self) -> bool {
        self.kind == Kind::Text
    }
}

/// A channel's settings frame and its overview: what a click or a key on it
/// asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A section was chosen (Delete Channel closes and asks).
    Choose(Page),
    /// The close button; Escape is the layer's.
    Close,
    /// The name was written (`input` 0).
    Name(String),
    /// The topic was written (`input` 1).
    Topic(String),
    /// A slowmode was chosen, by index (`input` 11).
    Slowmode(usize),
    /// Age restriction was set (`input` 12).
    Nsfw(bool),
    /// The unsaved bar's Reset (`input` 30).
    Undo,
    /// The unsaved bar's Save Changes (`input` 31).
    Save,
}
