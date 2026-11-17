//! The composer's frame: what it shows and what it asks for. The Slint
//! side: the composer of the conversation column (`ui/yapfast.slint:4137`
//! -`:4318`), whose parts are `ComposerFrame`, `StagedTile` and
//! `RequestBar` (`ui/composer.slint:68`, `:12`, `:196`); what the app fills
//! them from is `show_composer` (`src/ui/refresh.rs:4161`).
//!
//! The frame holds the draft and whose it is, the reply being written, the
//! files waiting to go, which buttons it has and what each may do, who is
//! typing, the hint under it, and the three things that stand in its place:
//! the bar saying it is locked, a message request's bar, and slow mode's
//! wait. The mentions and commands being offered are
//! [`suggestions`](super::suggestions) and [`commands`](super::commands),
//! held here.
//!
//! Reading `show_composer`: `composer-enabled` is [`Composer::enabled`],
//! `compose-label` and `reply-ping` are [`Reply`], `staged` is
//! [`Composer::staged`], `can-attach`, `can-start-threads` and
//! `can-create-polls` are [`More`], `can-pick-gifs` and `can-pick-stickers`
//! are [`Composer::gifs`] and [`Composer::stickers`], `composer-hint` is
//! [`Hint`], `composer-locked-text` is [`Composer::locked`] and
//! `request-title` is [`Composer::request`].

use std::ops::Range;

use iced::Color;
use iced::widget::image;

use crate::menu::Spot;

/// The composer's frame: what its screen draws. The window holds it as
/// `vm.chat.composer`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Composer {
    /// Mentions, channels and emoji offered as one types: C15. Wired by F01; keep it.
    pub suggestions: super::suggestions::Suggestions,
    /// Slash commands offered as one types: C16. Wired by F01; keep it.
    pub commands: super::commands::Commands,
    /// Whose draft this is, as the room's id hashed: a new key shows its
    /// draft afresh, and every edit it reports carries it.
    pub room: u64,
    /// The draft (`draft`).
    pub draft: String,
    /// The people and roles the draft mentions, to show as the message will.
    pub marks: Vec<Mark>,
    /// What the empty field says: "Message #general", "Write a reply"...
    pub placeholder: String,
    /// The field takes text (`composer-enabled`). Without it the frame
    /// draws, and says why in its placeholder or its hint.
    pub enabled: bool,
    /// The field has the keys: the frame's outline lightens.
    pub focused: bool,
    /// A reply is being written: its bar sits on the frame.
    pub reply: Option<Reply>,
    /// Files waiting to go with the message, in the order they go.
    pub staged: Vec<Staged>,
    /// The room marks files as spoilers (`can-mark-spoilers`).
    pub can_spoil: bool,
    /// What the plus does.
    pub more: More,
    /// The GIF button shows (`can-pick-gifs`); the emoji button then opens
    /// the same picker.
    pub gifs: bool,
    /// The sticker button shows (`can-pick-stickers`).
    pub stickers: bool,
    /// The picker open from the frame: its button reads brighter.
    pub open: Option<Expression>,
    /// Something is being sent (`sending`).
    pub sending: bool,
    /// Suggestions show over the field: the keys Up, Down, Tab, Enter and
    /// Escape are theirs.
    pub suggesting: bool,
    /// The `/` picker shows over the field.
    pub picking: bool,
    /// Who else is typing, as the line says it; empty when nobody is.
    pub typing: String,
    /// The line under the frame.
    pub hint: Hint,
    /// Slow mode: the seconds before the next message may go.
    pub slow: Option<u32>,
    /// The frame says in its place that nothing may be sent here
    /// (`composer-locked-text`), as one bar with no buttons.
    pub locked: Option<String>,
    /// A message request is answered in place of the frame; the bar's title.
    pub request: Option<String>,
}

/// A person or role the draft mentions, as the field marks it: its bytes of
/// the draft, and the role's colour, or the theme's mention colour.
pub type Mark = (Range<usize>, Option<Color>);

/// The reply being written: `compose-label` and `reply-ping`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// Who is replied to, and what they wrote, as the bar says it:
    /// "Replying to Bob: ...".
    pub label: String,
    /// The reply mentions its author: the toggle reads "@ On".
    pub ping: bool,
}

/// A file waiting to go with the message.
#[derive(Clone, Debug, PartialEq)]
pub struct Staged {
    pub name: String,
    /// Its size as text, "7 KB".
    pub size: String,
    /// Its picture, where it is one; a file icon otherwise.
    pub picture: Option<image::Handle>,
    /// It goes as a spoiler: its picture is covered.
    pub spoiler: bool,
    /// What the button that takes it out is called ("Remove {}"), as the
    /// app translates it.
    pub remove_label: String,
    /// What the button that covers or uncovers it is called ("Mark {} as a
    /// spoiler", "Stop covering {}").
    pub spoil_label: String,
}

/// What the plus does (`can-attach`, `can-start-threads`,
/// `can-create-polls`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct More {
    pub attach: bool,
    pub threads: bool,
    pub polls: bool,
}

impl More {
    /// The plus opens Discord's menu rather than the file picker.
    pub fn menu(self) -> bool {
        self.threads || self.polls
    }

    /// The plus does anything at all.
    pub fn any(self) -> bool {
        self.attach || self.menu()
    }

    /// The plus menu's rows, in Discord's order, as `plus-menu` has them
    /// (`ui/yapfast.slint:2356`-`:2368`): Upload a File, disabled without
    /// `can-attach`; Create Thread only where threads can start; Create Poll
    /// only where polls can be created. The app and the fixtures share it.
    ///
    /// Translated here, so only with the `fixtures` feature until the crate
    /// takes `yapfast-lang` for good (F01's `Cargo.toml`).
    #[cfg(feature = "fixtures")]
    pub fn rows(self) -> Vec<crate::menu::Row> {
        use crate::icon::Icon;
        use crate::menu::Row;
        use yapfast_lang::tr;
        let mut rows = vec![
            Row::new(tr!("Upload a File"), Intent::Attach.into())
                .icon(Icon::File)
                .disabled(!self.attach),
        ];
        if self.threads {
            rows.push(
                Row::new(tr!("Create Thread"), Intent::StartThread.into()).icon(Icon::Thread),
            );
        }
        if self.polls {
            rows.push(Row::new(tr!("Create Poll"), Intent::StartPoll.into()).icon(Icon::ChartBar));
        }
        rows
    }
}

/// One of the pickers the buttons and Ctrl+G, Ctrl+E open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expression {
    Gifs,
    Emoji,
    Stickers,
}

/// The line under the frame (`composer-hint`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hint {
    pub text: String,
    /// A lock before it: the room is encrypted, or a preview.
    pub lock: bool,
}

/// What a key asks of the suggestions showing over the field: the moves and
/// choices C15's list takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suggest {
    /// Up (-1) or Down (1).
    Move(i32),
    /// Tab or Enter.
    Choose,
    /// Escape.
    Dismiss,
}

/// What a key asks of the command being written, which C16's pills show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Tab (1) or Shift+Tab (-1): the next or previous option.
    Tab(i32),
    /// Enter on the file option at this index, which has none: pick one.
    File(usize),
    /// Escape, or with `back` Backspace in the empty first option, which
    /// puts the command's name back in the text.
    Close { back: bool },
}

/// The composer's frame: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The text changed: whose draft, the new text and the caret's byte
    /// offset (`draft_edited`, `src/ui/callbacks.rs:2908`).
    Edited {
        room: u64,
        text: String,
        caret: usize,
    },
    /// The caret moved alone (`composer_caret`).
    Caret { room: u64, caret: usize },
    /// Enter, or Send.
    Send,
    /// The field took the keys or lost them.
    Focus(bool),
    /// Ctrl+V or Paste: the app looks at the clipboard for files first.
    Paste,
    /// A button, or Ctrl+G or Ctrl+E: open this picker, anchored to the
    /// composer.
    Expressions(Expression),
    /// Up in an empty field: edit the last message.
    EditLast,
    /// Alt+Up: step through the messages.
    BrowseMessages,
    /// The plus where it has no menu, and the menu's Upload a File: the
    /// file picker.
    Attach,
    /// The plus where it has a menu, from where it sits: the menu opens
    /// above it, listing Upload a File, Create Thread and Create Poll as
    /// the room allows ([`Composer::more`]).
    More(Spot),
    /// Create Thread in the plus menu.
    StartThread,
    /// Create Poll in the plus menu.
    StartPoll,
    /// Take the staged file at this index out.
    Unstage(usize),
    /// Cover or uncover the staged file at this index.
    ToggleSpoiler(usize),
    /// Close the reply bar.
    CancelReply,
    /// The reply bar's "@ On"/"@ Off".
    ToggleReplyPing,
    /// A message request's Accept.
    Accept,
    /// A message request's Ignore.
    Ignore,
    /// A key while suggestions show.
    Suggest(Suggest),
    /// A key while a command is written or the `/` picker shows.
    Key(Key),
}
