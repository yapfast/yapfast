//! The composer field: the text of a message being written, as `Composer`
//! (`ui/composer-field.slint:14`) is to the Slint window. The channel's
//! composer, the thread's beside it, a new thread's and a new post's first
//! message and the message being edited in place all write in one.
//!
//! - **Grows, then scrolls.** It is as tall as its text, from
//!   [`min_height`](Composer::min_height) up to ten lines of
//!   `1.5 × text size` and its padding, then scrolls to keep the caret in
//!   view; the text wraps at words, and inside a word too long for a line.
//! - **The draft is the view-model's.** [`composer`] takes the text to show;
//!   the field edits a copy of it and reports each change
//!   ([`on_edit`](Composer::on_edit)) with the caret's byte offset, which
//!   the app's `@`, `#` and `:` suggestions read, and each move of the caret
//!   alone ([`on_caret`](Composer::on_caret)), as `draft_edited` and
//!   `composer_caret` take them (`src/ui/callbacks.rs:2908`, `:4078`). A text
//!   the app sets (sending clears it, an emoji goes in) replaces the copy; a
//!   change the app hasn't answered yet isn't undone by the view before it.
//! - **One draft per room.** [`key`](Composer::key) names whose draft it is:
//!   a new key shows that draft afresh, its caret at the end, with nothing
//!   to undo, and what was typed for the last key is gone from the field.
//!   Every report comes from a view with the key it carries, so a caller
//!   that puts the room in its messages never sends one room's text to
//!   another.
//! - **Keys** (`capture-key-pressed`, `:108`): while suggestions show, Up,
//!   Down, Tab, Enter and Escape are theirs ([`Suggestion`]); while an app's
//!   command is written, Tab moves between its options, Escape stops it,
//!   Backspace in the empty first option puts its name back and Enter on a
//!   file option without one picks a file ([`CommandKey`]); Escape while the
//!   `/` picker shows closes it. Then Enter sends and Shift+Enter breaks the
//!   line, Up in an empty field edits the last message, Alt+Up goes to the
//!   messages, Ctrl+V asks the app to paste, Ctrl+G and Ctrl+E open the GIFs
//!   and the emoji. Escape nothing here wants is left for the window
//!   (`focus::Scope::on_escape`, which undoes one thing of what's being
//!   written). Tab is left for the window too, which moves the focus.
//! - **Paste** (`src/paste.rs`): the clipboard may hold files or a picture
//!   rather than text, so Ctrl+V and the menu's Paste only send
//!   [`on_paste`](Composer::on_paste); the app looks, stages files and
//!   pictures, and finding text runs [`paste_text`], which pastes it at the
//!   caret. Without `on_paste`, the field pastes text itself.
//! - **Marks** (`vendor/i-slint-core/yapfast_marks.rs`): the people and
//!   roles the draft mentions show as the message will, in their colour on
//!   a translucent pill of it. Marks hold for the view-model's text only:
//!   while the field shows another (a keystroke the app hasn't marked yet,
//!   a composition) it draws plain.
//! - **The edit menu**: a right click opens Discord's menu for the field,
//!   above the pointer: Cut and Copy with a selection, Paste and Select All,
//!   which Discord's lacks (`edit-menu`, `:194`).
//! - **Input methods**: the caret's rectangle goes to the input method, as
//!   iced's own editor gives it, so fcitx5 and IBus place their candidates
//!   at the caret and show the text being composed there; while one
//!   composes, Enter and the arrows are its own.
//! - **Typing anywhere** (`vendor/i-slint-core/yapfast_typing.rs`): the
//!   window's scope ([`crate::focus`]) gives the field the focus and the key
//!   when nothing else took it; the field takes the focus through
//!   [`id`](Composer::id), like any other.
//!
//! The caret blinks only while the window is in front and in use, and
//! shows steady otherwise, so a field with the keys behind a game asks for
//! nothing. The menu's way in plays only while [`Motion`] does.
//!
//! [`place_caret`] puts the caret at a byte offset (after the app put an
//! emoji or a mention in the draft); give the field the keys with
//! [`crate::focus::to`].
//!
//! The command's pills (`CommandPills`, `ui/app_commands.slint:117`) are
//! the slash commands' to draw; the field shows them over its text
//! ([`pills`](Composer::pills)) and grows by their height, and
//! [`command_placeholder`] says what the empty field says meanwhile.

mod draw;
mod events;
mod hint;
mod keys;
mod marks;
mod menu;
mod offset;
mod operation;
mod state;
#[cfg(test)]
mod tests;
mod widget;

use std::borrow::Cow;
use std::ops::Range;

use iced::Color;
use iced::advanced::widget::Id;

use anomi_theme::{Element, Motion, View, typography};

pub use hint::{Writing, command_placeholder};
pub use operation::{paste_text, place_caret};

/// `text-size: 16px`.
const TEXT_SIZE: f32 = typography::size::TITLE;
/// `pad: 12px`: the room above and below the text.
const PAD: f32 = 12.0;
/// `max(48px, …)`: the least height of the field and its padding.
const MIN_HEIGHT: f32 = 48.0;
/// `line: root.text-size * 1.5`: a line as the field's growth counts it.
const LINE_ROOM: f32 = 1.5;
/// `10 * root.line`: how many such lines it grows to before it scrolls.
const LINES: f32 = 10.0;
/// The text's own line height, DM Sans's, relative to its size; the
/// field's `TextInput` takes the font's.
const TEXT_LINE: f32 = 1.3;
/// `overhang: 4px`: room above and below the text inside the part that
/// clips it, as an emoji is taller than its line.
const OVERHANG: f32 = 4.0;
/// `x: 6px; width: root.width - 12px`: the text's inset at either side.
const INSET: f32 = 6.0;
/// The pills sit 4 px into the padding (`y: root.pad - 4px`) and the text
/// 6 px under them (`preferred-height + 6px`).
const PILLS_RAISE: f32 = 4.0;
const PILLS_GAP: f32 = 6.0;

/// A part of the text drawn as a mention: its byte range, and its colour,
/// or `None` for the theme's mention colour.
pub type Mark = (Range<usize>, Option<Color>);

/// What the keys ask of the suggestions showing over the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suggestion {
    /// Up (-1) or Down (1).
    Move(i32),
    /// Tab or Enter, without Shift.
    Choose,
    /// Escape.
    Dismiss,
}

/// What the keys ask of the app command being written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKey {
    /// Tab (1) or Shift+Tab (-1): the next or previous option.
    Tab(i32),
    /// Enter on the option at this index, a file without one: pick it.
    File(usize),
    /// Escape, or with `back` Backspace in the empty first option, which
    /// puts the command's name back in the text.
    Close { back: bool },
}

/// Which of the expressions Ctrl+G and Ctrl+E open, as Discord's do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expressions {
    Gifs,
    Emoji,
}

/// The app command being written, as far as the keys need it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Command {
    /// How many options it has.
    pub options: usize,
    /// The option the text writes.
    pub focus: usize,
    /// That option is a file and has none yet: Enter picks one.
    pub wants_file: bool,
}

/// The composer field; see the module's documentation.
pub struct Composer<'a, M> {
    text: &'a str,
    key: u64,
    id: Option<Id>,
    placeholder: Cow<'a, str>,
    size: f32,
    pad: f32,
    min_height: f32,
    max_height: Option<f32>,
    read_only: bool,
    muted_placeholder: bool,
    marks: &'a [Mark],
    suggesting: bool,
    picking: bool,
    command: Option<Command>,
    pills: Option<Element<'a, M>>,
    motion: Motion,
    on_edit: Option<Box<dyn Fn(String, usize) -> M + 'a>>,
    on_caret: Option<Box<dyn Fn(usize) -> M + 'a>>,
    on_send: Option<M>,
    on_focus: Option<Box<dyn Fn(bool) -> M + 'a>>,
    on_paste: Option<M>,
    on_expressions: Option<Box<dyn Fn(Expressions) -> M + 'a>>,
    on_up: Option<M>,
    on_browse: Option<M>,
    on_suggestion: Option<Box<dyn Fn(Suggestion) -> M + 'a>>,
    on_command: Option<Box<dyn Fn(CommandKey) -> M + 'a>>,
}

/// A composer showing `text`, the view-model's draft. Give it
/// [`on_edit`](Composer::on_edit) or it takes nothing, as a disabled one.
pub fn composer<'a, M>(text: &'a str, motion: Motion) -> Composer<'a, M> {
    Composer {
        text,
        key: 0,
        id: None,
        placeholder: Cow::Borrowed(""),
        size: TEXT_SIZE,
        pad: PAD,
        min_height: MIN_HEIGHT,
        max_height: None,
        read_only: false,
        muted_placeholder: false,
        marks: &[],
        suggesting: false,
        picking: false,
        command: None,
        pills: None,
        motion,
        on_edit: None,
        on_caret: None,
        on_send: None,
        on_focus: None,
        on_paste: None,
        on_expressions: None,
        on_up: None,
        on_browse: None,
        on_suggestion: None,
        on_command: None,
    }
}

impl<'a, M> Composer<'a, M> {
    /// Whose draft it shows, as a room's id hashed: a new key shows its
    /// draft afresh.
    pub fn key(mut self, key: u64) -> Self {
        self.key = key;
        self
    }

    /// The id the window's focus reaches it by (`focus::COMPOSER`...).
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// What shows while it's empty: the room's "Message #general", or while
    /// an app's command is written its [`command_placeholder`].
    pub fn placeholder(mut self, placeholder: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// The text's size: 16 px unless set.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The room above and below the text: 12 px unless set; the channel's
    /// composer takes `metrics::COMPOSER_PADDING`.
    pub fn pad(mut self, pad: f32) -> Self {
        self.pad = pad;
        self
    }

    /// Its least height with the padding: 48 px unless set; the channel's
    /// composer takes `metrics::COMPOSER_HEIGHT`.
    pub fn min_height(mut self, height: f32) -> Self {
        self.min_height = height;
        self
    }

    /// Its greatest height with the padding, under the ten lines it grows
    /// to; past it the text scrolls with the caret (a bot form's paragraph).
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    /// Taking nothing without [`on_edit`](Self::on_edit), it draws as one
    /// that does, not muted (Slint's `read-only`, a form saving).
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// The placeholder in the muted ink, as a settings area's, not dimmer.
    pub fn muted_placeholder(mut self) -> Self {
        self.muted_placeholder = true;
        self
    }

    /// The mentions in `text` to mark; see [`Mark`].
    pub fn marks(mut self, marks: &'a [Mark]) -> Self {
        self.marks = marks;
        self
    }

    /// Mention, channel, emoji or command suggestions show over it: the
    /// keys go to them ([`on_suggestion`](Self::on_suggestion)).
    pub fn suggesting(mut self, suggesting: bool) -> Self {
        self.suggesting = suggesting;
        self
    }

    /// The `/` picker shows over it, perhaps only to say its commands are
    /// coming or that none match: Escape closes it
    /// ([`CommandKey::Close`]).
    pub fn picking(mut self, picking: bool) -> Self {
        self.picking = picking;
        self
    }

    /// An app's command is being written; see [`Command`].
    pub fn command(mut self, command: Option<Command>) -> Self {
        self.command = command;
        self
    }

    /// The command's pills, drawn over the text while one is written.
    pub fn pills(mut self, pills: impl View<M> + 'a) -> Self {
        self.pills = Some(pills.boxed());
        self
    }

    /// The text changed (typed, pasted, cut, undone), with the caret's byte
    /// offset after it.
    pub fn on_edit(mut self, on_edit: impl Fn(String, usize) -> M + 'a) -> Self {
        self.on_edit = Some(Box::new(on_edit));
        self
    }

    /// The caret moved to a byte offset, the text unchanged.
    pub fn on_caret(mut self, on_caret: impl Fn(usize) -> M + 'a) -> Self {
        self.on_caret = Some(Box::new(on_caret));
        self
    }

    /// Enter: send.
    pub fn on_send(mut self, message: M) -> Self {
        self.on_send = Some(message);
        self
    }

    /// The field took the keys (`true`) or lost them, which the frame
    /// around it shows; the window types into the composer used last.
    pub fn on_focus(mut self, on_focus: impl Fn(bool) -> M + 'a) -> Self {
        self.on_focus = Some(Box::new(on_focus));
        self
    }

    /// Ctrl+V or the menu's Paste: the app looks at the clipboard; see the
    /// module's documentation.
    pub fn on_paste(mut self, message: M) -> Self {
        self.on_paste = Some(message);
        self
    }

    /// Ctrl+G and Ctrl+E.
    pub fn on_expressions(mut self, on_expressions: impl Fn(Expressions) -> M + 'a) -> Self {
        self.on_expressions = Some(Box::new(on_expressions));
        self
    }

    /// Up in an empty field: edit the last message.
    pub fn on_up(mut self, message: M) -> Self {
        self.on_up = Some(message);
        self
    }

    /// Alt+Up: go to the messages, to step through them.
    pub fn on_browse(mut self, message: M) -> Self {
        self.on_browse = Some(message);
        self
    }

    /// The keys suggestions take while [`suggesting`](Self::suggesting).
    pub fn on_suggestion(mut self, on_suggestion: impl Fn(Suggestion) -> M + 'a) -> Self {
        self.on_suggestion = Some(Box::new(on_suggestion));
        self
    }

    /// The keys an app's command takes while one is written or the `/`
    /// picker shows.
    pub fn on_command(mut self, on_command: impl Fn(CommandKey) -> M + 'a) -> Self {
        self.on_command = Some(Box::new(on_command));
        self
    }

    /// The caret blinks: the window is in front and in use. Otherwise it
    /// shows steady and asks for no frames.
    fn blinks(&self) -> bool {
        self.motion.front && !self.motion.idle
    }

    /// It takes text: it has [`on_edit`](Self::on_edit).
    fn enabled(&self) -> bool {
        self.on_edit.is_some()
    }

    /// The ten lines' height it grows to, or less under its greatest.
    fn tallest(&self) -> f32 {
        let lines = LINES * self.size * LINE_ROOM;
        (self.max_height).map_or(lines, |most| lines.min(most - 2.0 * self.pad))
    }
}
