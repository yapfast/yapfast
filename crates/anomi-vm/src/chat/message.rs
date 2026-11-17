//! A message: what it shows and what a click on it asks for. The Slint
//! side: `MessageContent`, `MessageBody`, `BotTag` and `ThreadChip`
//! (`ui/messages.slint:3015`, `:2948`, `:2031`, `:1989`) over the
//! `MessageRow` they draw (`:808`), the row's tints and its hover bar in the
//! timeline (`ui/yapfast.slint:3804`-`:3826`, `:3951`-`:4039`), and the glue
//! that marks a row (`Marks` and `Decoration`, `src/ui/messages.rs:41`,
//! `:54`) and hides a blocked one (`:885`).
//!
//! The projection builds a message once and changes only what changed, as
//! `Decoration::apply` does: the author, the time and the body are made
//! when it comes or is edited; [`Message::first`], [`Message::lit`] and
//! [`Message::highlighted`] as the rows around it and the pointer move; the
//! hover bar ([`Bar`]) only on the one message the pointer or the keyboard
//! is on, so the frequent reactions it offers aren't copied into every row.
//! A blocked person's message comes as one: "Blocked user" on a grey "?",
//! with "*Blocked message.* Show it" as its body, the link revealing it.
//! Task C02.

mod bar;
mod parts;

use crate::menu::Spot;
use crate::text::{Link, Text};

pub use bar::{Bar, Edge, May, Quick};
pub use parts::{Author, Command, Delivery, Header, Notice, Reply, Thinking, Thread};

/// A message: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    /// Its identity, the timeline row's key (`timeline::group::key` of its
    /// item): every intent names the message by it.
    pub key: u64,
    /// It starts a group: its author's picture and name show over it.
    /// Later messages from the same person follow without them (`first`).
    pub first: bool,
    /// What lies over it in the timeline, set from the row's marks.
    pub edge: Edge,
    /// The compact display (Settings → Messages): no pictures, every
    /// message's time in a wider gutter, a group's author beside its first
    /// line (`ClientStyle.compact-messages`).
    pub compact: bool,
    /// The size of messages' text (Settings → Appearance).
    pub size: Size,
    /// Who wrote it: shown on the first of a group, and in "{author} is
    /// thinking…"; the projection may leave the rest's default.
    pub author: Author,
    /// When, beside the author: "01.10.2026 16:55", or a day's word and the
    /// time (`time-styled`).
    pub time: String,
    /// The time alone, "16:55", in the gutter of a message under the
    /// pointer, or of each in the compact display (`short-time`).
    pub short_time: String,
    /// A line over it: the message it replies to, or the command it answers.
    pub header: Header,
    /// A notice in place of a message, as that someone pinned one: its icon
    /// stands in for the picture, and its body (with its author's name and
    /// time, linked) is dimmer.
    pub notice: Option<Notice>,
    /// What it says. Empty for a message of files alone.
    pub body: Text,
    /// Whether its body folds while long.
    pub fold: Fold,
    /// Forwarded on Discord: the body, files and embeds are the original's.
    pub forwarded: bool,
    /// Edited since it was sent: "(edited)" under it.
    pub edited: bool,
    /// Deleted while shown: its body is dimmed and it takes no action.
    pub deleted: bool,
    /// Still going out, or failed to: its body is dimmed and it takes no
    /// action until it's sent.
    pub delivery: Option<Delivery>,
    /// An app's answer still to come.
    pub thinking: Option<Thinking>,
    /// Says a card is hidden, for a message that is only one, with link
    /// previews off (`hidden-card`).
    pub hidden_card: Option<String>,
    /// Why the last bot control used on it failed, under its controls
    /// (`bot-failed-text`).
    pub failed: Option<String>,
    /// The thread it started or tells of.
    pub thread: Option<Thread>,
    /// An app's answer only this account sees, with "Only you can see this
    /// · Dismiss message" under it.
    pub ephemeral: bool,
    /// Calls on this account: it mentions it or a group it's in, or, while
    /// it's new, replies to its message. It lies on marigold with a marigold
    /// edge.
    pub highlighted: bool,
    /// The message the pointer is on, or was last on (`active-item`): it
    /// lies on the row hover's colour, shows its time in the gutter and
    /// plays its author's picture.
    pub lit: bool,
    /// The local preview of the interface: "Dismiss message" does nothing.
    pub preview: bool,
    /// The hover bar, on the lit message, or the one whose menu, reaction
    /// picker or deletion is open; none on one deleted or on its way.
    pub bar: Option<Bar>,
    /// Its reactions: C03. Wired by F01; keep it.
    pub reactions: super::reactions::Reactions,
    /// Its pictures, videos, files and stickers: C04. Wired by F01; keep it.
    pub attachments: super::attachments::Attachments,
    /// Its embeds: C05. Wired by F01; keep it.
    pub embeds: super::embeds::Embeds,
    /// A bot's components: C06. Wired by F01; keep it.
    pub components: super::components::Components,
    /// Its poll: C07. Wired by F01; keep it.
    pub poll: super::poll::Poll,
    /// The editor it turns into while edited: C13. Wired by F01; keep it.
    pub editing: super::inline_edit::InlineEdit,
}

impl Message {
    /// Whether it takes actions: its menu, a right click, the hover bar.
    /// One deleted or not yet sent doesn't.
    pub fn acts(&self) -> bool {
        !self.deleted && self.delivery.is_none()
    }

    /// Whether its body shows dimmed: deleted, or not yet sent.
    pub fn dimmed(&self) -> bool {
        !self.acts()
    }

    /// Whether its author's picture and name show over it.
    pub fn headed(&self) -> bool {
        self.first && self.notice.is_none()
    }
}

/// The size of messages' text, as Settings → Appearance sets it
/// (`message-font-size`, `ui/yapfast.slint:1467`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Size {
    Small,
    #[default]
    Normal,
    Large,
}

impl Size {
    /// In logical pixels: 14, 15 or 18.
    pub fn px(self) -> f32 {
        match self {
            Self::Small => 14.0,
            Self::Normal => 15.0,
            Self::Large => 18.0,
        }
    }
}

/// Whether a long body folds to twelve lines with Show more under it
/// ("Collapse long messages", `collapse-long` and `expanded`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fold {
    /// Long messages don't fold.
    #[default]
    Never,
    /// It folds while long.
    Folded,
    /// Show more was clicked: it shows whole, with Show less.
    Open,
}

/// Whose card a click opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    /// The message's author: their picture or name.
    Author,
    /// Whoever used the command an app's answer names.
    Commander,
}

/// A message: what a click or a key on it asks for. Each names the message
/// by its key. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The pointer came onto it: it's the lit one, with its time and its
    /// hover bar (`active-item`).
    Point(u64),
    /// A person's card, beside their picture or name (`author-card`).
    Card(u64, Who, Spot),
    /// The author's menu, at the pointer (`author-menu`).
    AuthorMenu(u64, Spot),
    /// The card of the server whose tag the author wears, beside the tag.
    TagCard(u64, Spot),
    /// The message it replies to: jump to it (`jump-to`).
    Quoted(u64),
    /// The slash command an app's answer names: write it in the composer
    /// (`use-command`).
    UseCommand(u64),
    /// A link, a mention, an emoji or a spoiler in its body, or a notice's
    /// author or message (`link-clicked`); "Show it" on a blocked message.
    Link(u64, Link),
    /// Show more (true) or Show less (`fold`).
    Fold(u64, bool),
    /// The thread it started (`open-thread`).
    Thread(u64),
    /// Send it again (`retry`), or give up sending it (`cancel-send`).
    Retry(u64),
    CancelSend(u64),
    /// "Dismiss message" under an answer only this account sees.
    Dismiss(u64),
    /// Its menu: the hover bar's More, at the button, or a right click, at
    /// the pointer (`open-menu`).
    Menu(u64, Spot),
    /// One of the hover bar's frequent reactions, by its key (`react`).
    React(u64, String),
    /// The reaction picker, beside its button (`open-picker`).
    Picker(u64, Spot),
    Edit(u64),
    Reply(u64),
    Forward(u64),
    /// Pin it, or unpin it (`pin-message`).
    Pin(u64),
    /// A thread from it (`new-thread`).
    NewThread(u64),
    /// Shift's Delete: at once, without asking (`delete-now`).
    DeleteNow(u64),
    /// "Delete for everyone?": Delete (`confirm-delete-message`) or Cancel.
    ConfirmDelete(u64),
    CancelDelete(u64),
}
