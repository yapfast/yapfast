//! A bot's forms and select menus: what they show and what they ask for.
//! Ports the `ModalField` and `SelectChoice` structs and the properties of
//! `BotModalDialog` and `BotSelectDropdown` (`ui/interactions.slint:10`,
//! `:155`, `:24`), as `show_bot_interactions` fills them
//! (`src/ui/refresh.rs:784`). Task C20.
//!
//! The window holds one [`BotForms`]: the form open over it
//! (`Layer::BotModal`) and the select menu open, from a message or from a
//! field of the form (`Layer::BotSelect`). The app fills each while its layer
//! is open; a select opened from a form's field sits over the form.

use iced::widget::image;

use crate::menu::Spot;
use crate::text::Text;

/// A bot's forms and select menus: what its screen draws. The window holds it
/// as `vm.chat.bot_forms`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotForms {
    pub form: Form,
    pub select: Select,
}

/// A bot's form: its title, its fields in order, and how it stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Form {
    pub heading: String,
    pub fields: Vec<Field>,
    /// Every part of it is one Yapfast can fill; if not, Submit is off and
    /// the form says to use Discord.
    pub supported: bool,
    /// Submitted, and waiting for Discord: nothing may change, and the
    /// button reads "Submitting…".
    pub saving: bool,
    /// Why Discord refused it, in red above the buttons.
    pub error: String,
}

/// What a field is, as Discord numbers its component (`ModalField.kind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A part Yapfast can't fill (0, or a number it doesn't know).
    #[default]
    Unsupported,
    /// A select menu (3, 5-8): a box showing what's picked, which opens its
    /// dropdown.
    Select,
    /// A text input (4), one line or a paragraph.
    Input,
    /// Text to read (10).
    Text,
    /// A file upload (19).
    Files,
    /// A radio group (21).
    Radio,
    /// A checkbox group (22).
    Checks,
    /// One checkbox (23).
    Check,
}

impl Kind {
    /// The kind of Discord's component `number`.
    pub fn of(number: u8) -> Self {
        match number {
            3 | 5..=8 => Self::Select,
            4 => Self::Input,
            10 => Self::Text,
            19 => Self::Files,
            21 => Self::Radio,
            22 => Self::Checks,
            23 => Self::Check,
            _ => Self::Unsupported,
        }
    }
}

/// A field of a form.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Field {
    pub kind: Kind,
    pub label: String,
    /// Under the label, muted.
    pub description: String,
    /// An input's text.
    pub value: String,
    pub placeholder: String,
    /// The label ends in a star.
    pub required: bool,
    /// An input of many lines.
    pub paragraph: bool,
    /// A select's picks, as their labels joined; empty shows its placeholder.
    pub summary: String,
    /// The text to read, parsed.
    pub text: Text,
    /// A radio or checkbox group's options.
    pub choices: Vec<Choice>,
    /// A checkbox's own state.
    pub checked: bool,
    /// The files chosen, by name.
    pub files: Vec<File>,
    /// How many files it takes.
    pub max_files: usize,
}

/// A file chosen for an upload field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct File {
    /// Its name.
    pub name: String,
    /// What the button that takes it out is called ("Remove {}"), as the
    /// app translates it.
    pub remove_label: String,
}

/// An option of a select menu, radio group or checkbox group, by its place
/// among the menu's choices (`SelectChoice`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub index: usize,
    pub label: String,
    /// What a radio pick or a checkbox toggle sends.
    pub value: String,
    pub description: String,
    pub selected: bool,
    /// A Unicode emoji before the label, drawn as Twemoji.
    pub emoji: String,
    /// A server's emoji, when the option's is one: it takes the emoji's place.
    pub picture: Option<image::Handle>,
}

/// A bot's select menu as it shows open: a dropdown under the box that opened
/// it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Select {
    pub heading: String,
    /// What was typed to find more, for members, roles and channels.
    pub query: String,
    pub min: usize,
    /// One takes the option clicked; several take what changed when it
    /// closes.
    pub max: usize,
    /// Members, roles or channels: typing finds more.
    pub searchable: bool,
    /// The options matching the query.
    pub choices: Vec<Choice>,
    /// The width of the box it opened under, which sets its own.
    pub anchor: f32,
}

/// A bot's forms and select menus: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The form's field at this place changed: an input's whole text, a
    /// radio's or a checkbox group's option by its value, a checkbox's own
    /// toggle by an empty text (`Action::SetModalField`).
    Edit(usize, String),
    /// A select field's box was pressed, which opens its dropdown at `Spot`
    /// (`Action::OpenModalSelect`).
    OpenSelect(usize, Spot),
    /// The file dialog for a field's upload.
    ChooseFiles(usize),
    /// Take a chosen file (by place) out of a field.
    RemoveFile(usize, usize),
    /// A link in the form's text.
    Link(crate::text::Link),
    Submit,
    Close,
    /// An option of the open select menu, by its place among the menu's
    /// choices (`Action::ToggleSelect`).
    Toggle(usize),
    /// The dropdown's search field changed (`Action::SearchSelect`).
    Search(String),
}
