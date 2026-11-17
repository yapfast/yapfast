//! Making a channel or a category, and the two questions Slint's
//! `ChannelDialog` asks beside it (copying a channel, deleting one): what the
//! dialog shows and what it asks for. Slint's `ChannelPrompt`
//! (`ui/server.slint:489`) filled by `show_channel_dialog`
//! (`src/ui/refresh.rs:3149`), already translated. Leaving a server and the
//! move-sync question are the same component but N13's (`places::leave`).
//! The window holds it as `vm.settings.create_channel`.

/// What a new channel is: the two segments of "Channel type" (Slint's `kind`:
/// 0 for text, 1 for voice).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelType {
    #[default]
    Text,
    Voice,
}

/// The dialog: the fields of Slint's `ChannelPrompt` these asks use, and the
/// two it owns (`name`, `kind`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateChannel {
    /// "Create channel", "Create category", "Duplicate channel", "Delete channel".
    pub title: String,
    /// Where it goes ("In Server"), or what deleting it does.
    pub detail: String,
    /// The button that carries it out, such as "Create channel".
    pub confirm: String,
    /// Destructive (a delete): the confirm button is red.
    pub danger: bool,
    /// The words of the two types, Text then Voice, when it offers them (a
    /// channel it makes, not a category, a copy or a delete); empty else.
    pub kinds: Vec<String>,
    /// The type chosen, while `kinds`.
    pub kind: ChannelType,
    /// It asks for a name (everything but a delete); the heading over the
    /// field, and what it shows while empty.
    pub named: bool,
    pub field: String,
    pub placeholder: String,
    /// What is written in the name field.
    pub name: String,
    /// It was sent and waits for Discord: nothing may be asked again.
    pub saving: bool,
    /// Why Discord refused; empty for none.
    pub error: String,
}

/// What a click or a key on it asks for (`ui/server.slint:518`: `save`,
/// `close`, and the `kind` and `name` it binds). While it is open they are
/// `Action::SaveChannelDialog`-style actions of the app's channel dialog
/// (`src/ui/callbacks.rs:1380`: `Action::CloseChannelDialog`). Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A segment of "Channel type".
    Kind(ChannelType),
    /// The name was typed.
    Name(String),
    /// The main button, or Enter.
    Confirm,
    /// Cancel, the X, Escape or a press outside.
    Close,
}
