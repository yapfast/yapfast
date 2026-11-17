//! The message menu: what it shows and what it asks for. The Slint side is
//! `MessageMenu` (`ui/messages.slint:1394`); the app fills it from the
//! message right-clicked (`src/ui/menus.rs:110`, the rows' facts; `:560`,
//! the apps).
//!
//! The app opens `Layer::MessageMenu` at the pointer (or at the hover bar's
//! More button) and fills [`Menu`] with facts about the message; the rows
//! and their order (Discord's) are the screen's, as they carry its text:
//! `anomi_chat::menu::build` fills [`Menu::rows`] from the facts, and the
//! app calls it when it opens the menu.
//! Every [`Intent`] is about the message the menu was
//! opened on, [`Menu::key`].

use crate::chat::emoji_picker::Emoji;
use crate::menu::Row;

/// A message's menu: what its screen draws. The window holds it as
/// `vm.chat.menu`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    /// The message it is about (`menu-message.item`); every intent names it.
    pub key: u64,
    /// What this account may do with it and what it has.
    pub facts: Facts,
    /// The reactions this device uses most, four of them, as tiles over the
    /// rows; none leaves them out (`menu-reactions`).
    pub reactions: Vec<Emoji>,
    /// The channel's apps' message commands, which Apps lists. Empty for a
    /// message that takes none: not a Discord message, still sending, an
    /// answer only this account sees, or Yapfast's own "Sending command...".
    pub apps: Vec<App>,
    /// The rows under the tiles, as `anomi_chat::menu::build` makes them from
    /// the facts and the apps: its Apps row holds the submenu.
    pub rows: Vec<Row<Intent>>,
    /// The Apps submenu shows (`apps-open`): it opens on hover or a click.
    pub apps_open: bool,
    /// The submenu opens left of the menu, for want of room on its right
    /// (`flip`).
    pub flip: bool,
}

/// What the message is and what may be done with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// React to it (`menu-can-react`).
    pub can_react: bool,
    /// It has reactions to look at.
    pub has_reactions: bool,
    /// This account may edit it, delete it, reply to it, forward it, pin
    /// it, start a thread from it.
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_reply: bool,
    pub can_forward: bool,
    pub can_pin: bool,
    pub pinned: bool,
    /// The conversation allows threads (`can-start-thread`).
    pub can_start_thread: bool,
    /// The thread it started, by its room.
    pub thread: Option<String>,
    /// The composer takes text here (`composer-enabled`): edit, reply and
    /// delete need it.
    pub composer_enabled: bool,
    /// Forwarding is allowed (Matrix forwards only from a verified device).
    pub forward_allowed: bool,
    /// It's sent, not on its way (`delivery == ""`).
    pub delivered: bool,
    /// Its text, for Copy text; empty when it has none.
    pub body: String,
    /// The text selected in it, for Copy selection; empty for none.
    pub selection: String,
    /// It's a poll this account may close.
    pub can_end_poll: bool,
    pub can_mark_unread: bool,
    /// A Discord message's link and id; empty for any other message.
    pub link: String,
    pub id: String,
    /// The picture or video right-clicked.
    pub media: Option<Media>,
    /// Its address on the web, or the link's, or empty when it has none.
    pub address: String,
    /// The menu is over a preview (a link's or the composer's): no saving.
    pub preview: bool,
}

/// The attachment right-clicked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Media {
    /// Its index in the message's attachments.
    pub index: usize,
    pub video: bool,
}

/// An app's message command, a row of the Apps submenu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct App {
    /// What running it names.
    pub id: String,
    pub name: String,
    /// The app, under the name.
    pub app: String,
}

/// The message menu: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant. Each is about [`Menu::key`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// One of the quick reactions, by its key (`react`).
    React(String),
    /// The reaction picker (`open-picker`).
    AddReaction,
    ViewReactions,
    Edit,
    Reply,
    Forward,
    StartThread,
    /// The thread it started, by its room (`view-thread`).
    ViewThread(String),
    /// Copy text, or the selection, or a link, or an id: what goes to the
    /// clipboard (`copy-text`).
    Copy(String),
    EndPoll,
    /// Pin it, or unpin it (`pin-message`).
    Pin,
    MarkUnread,
    /// The picture, by its attachment's index (`copy-image`).
    CopyImage(usize),
    /// A picture or video (`save-attachment`).
    Save(usize),
    OpenLink(String),
    /// Asks to make sure first (`request-delete`).
    Delete,
    /// An app's command, by its id (`run-app-command`).
    RunApp(String),
    /// The pointer is on the Apps row (true) or on another row (false).
    Apps(bool),
}
