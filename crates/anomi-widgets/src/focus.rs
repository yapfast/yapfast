//! Keyboard focus and the window's keys: the root `FocusScope`s and key
//! handlers of `ui/yapfast.slint`, the window's own filter of keys in
//! `src/ui.rs` (`watch_window`, `:660`-`:768`; the mouse's side buttons,
//! `side_button_action`, `:328`) and typing anywhere (`type_anywhere`,
//! `src/ui.rs:869`, over the vendored Slint's hook,
//! `vendor/i-slint-core/yapfast_typing.rs` and `window.rs:1553`-`:1598`).
//!
//! [`scope`] wraps the whole window, the layers over it included (the shell
//! places it around [`crate::layer::Layers`]), and has every key first and
//! last:
//!
//! 1. **Before anything else**, wherever the focus is: Ctrl+K
//!    ([`Scope::on_switcher`]), Ctrl+F ([`Scope::search`]), Alt+Left and
//!    Alt+Right and the mouse's back and forward buttons ([`Scope::on_back`],
//!    [`Scope::on_forward`]; the button's release is kept too, so it isn't
//!    taken for a click), and Escape while something waits to be let go
//!    ([`Scope::on_let_go`]: someone dragged from a voice channel). Ctrl
//!    and Alt count alone, without Shift or the other. Letters go by the
//!    layout's letter, or in a layout without Latin letters by the key in
//!    the letter's place, so Ctrl+K is Ctrl+Л on a Russian keyboard. A key
//!    held down acts once. Each is on only while the window gives it; one
//!    that's off goes on as any other key.
//! 2. **Then the window**: the focused widget, the top layer (which keeps
//!    Escape to close itself), the rest.
//! 3. **What nothing took**, with neither Ctrl, Alt nor Super held:
//!    - Tab and Shift+Tab move the focus to the next or previous widget
//!      that may have it, round from the last to the first, among those in
//!      reach: inside the top layer while one is open, as the layers keep
//!      focus operations inside it. A field reached so has its text
//!      selected, as Slint's do. Tab with nothing focused goes instead to
//!      where typing goes, and types nothing.
//!    - Escape sends [`Scope::on_escape`]: the app undoes one thing of what's
//!      being written, in Discord's order (`escape-composing`,
//!      `ui/yapfast.slint:1765`). A field that does something with Escape
//!      of its own captures it.
//!    - Text, Backspace and Delete focus [`Scope::type_into`] with its caret
//!      at the end, and the key goes on to it, as Discord sends typing
//!      anywhere to its composer. Enter doesn't: a stray one after a menu
//!      closes shouldn't send the draft. Nothing happens when the target is
//!      out of reach (not shown, or under a layer) or the window gives
//!      none; while [`Scope::browse`] has the focus (the timeline's keys)
//!      only letters and digits leave it; a key that talks while held
//!      ([`Scope::talk_keys`]) never writes.
//!
//! The scope keeps no clock and asks for no frame of its own: it acts only
//! on a key or a button, and asks for one redraw when it moved the focus.
//!
//! The app moves the focus with tasks: [`into`] (focus and caret at the
//! end), [`select`] (focus and select the text), [`to`] (focus alone),
//! [`next`] and [`previous`] (Tab's walk). The targets several areas share
//! carry the ids below; the widget gives itself the id, and the shell names
//! it to the scope.

mod reach;
mod shortcut;
#[cfg(test)]
mod tests;
mod unclaimed;
mod walk;
mod widget;

use iced::Task;
use iced::advanced::widget::{Id, operate};
use iced::keyboard::key::Physical;

use anomi_theme::{Element, View};

use reach::Caret;

/// The channel's composer, which typing goes to.
pub const COMPOSER: Id = Id::new("composer");
/// The composer of the thread beside the channel, or of the new thread's
/// form, which typing goes to once it was used last.
pub const ASIDE_COMPOSER: Id = Id::new("aside-composer");
/// The conversation's search in the header, which Ctrl+F focuses.
pub const SEARCH: Id = Id::new("search");
/// The timeline's keys, Up and Down stepping through the messages: while
/// it has the focus, only letters and digits type into the composer.
pub const TIMELINE: Id = Id::new("timeline");

/// The window's keys around `content`; see the module's documentation.
pub struct Scope<'a, M> {
    content: Element<'a, M>,
    talking: &'a [Physical],
    keys_taken: bool,
    switcher: Option<M>,
    back: Option<M>,
    forward: Option<M>,
    let_go: Option<M>,
    escape: Option<M>,
    search: Option<Id>,
    type_into: Option<Id>,
    browse: Option<Id>,
}

/// The window's keys around `content`, all off until given.
pub fn scope<'a, M>(content: impl View<M> + 'a) -> Scope<'a, M> {
    Scope {
        content: content.boxed(),
        talking: &[],
        keys_taken: false,
        switcher: None,
        back: None,
        forward: None,
        let_go: None,
        escape: None,
        search: None,
        type_into: None,
        browse: None,
    }
}

impl<'a, M> Scope<'a, M> {
    /// What Ctrl+K sends: the quick switcher opens, or closes. Off over the
    /// settings and the question before a masked link opens.
    pub fn on_switcher(mut self, message: impl Into<Option<M>>) -> Self {
        self.switcher = message.into();
        self
    }

    /// What Alt+Left and the mouse's back button send. Off over the
    /// settings.
    pub fn on_back(mut self, message: impl Into<Option<M>>) -> Self {
        self.back = message.into();
        self
    }

    /// What Alt+Right and the mouse's forward button send.
    pub fn on_forward(mut self, message: impl Into<Option<M>>) -> Self {
        self.forward = message.into();
        self
    }

    /// What Escape sends before anything else has it, wherever the focus
    /// is: while someone dragged from a voice channel waits to be let go,
    /// Escape does that and nothing else.
    pub fn on_let_go(mut self, message: impl Into<Option<M>>) -> Self {
        self.let_go = message.into();
        self
    }

    /// What Escape nothing took sends.
    pub fn on_escape(mut self, message: impl Into<Option<M>>) -> Self {
        self.escape = message.into();
        self
    }

    /// The field Ctrl+F focuses, selecting what it holds, when it's in
    /// reach; else Ctrl+F goes on to whatever has the keyboard.
    pub fn search(mut self, field: impl Into<Option<Id>>) -> Self {
        self.search = field.into();
        self
    }

    /// Where typing goes: the composer used last.
    pub fn type_into(mut self, field: impl Into<Option<Id>>) -> Self {
        self.type_into = field.into();
        self
    }

    /// The widget whose own keys only letters and digits leave for the
    /// composer: the timeline, while it steps through the messages.
    pub fn browse(mut self, widget: impl Into<Option<Id>>) -> Self {
        self.browse = widget.into();
        self
    }

    /// The keys that talk while held, in their places: pressed, they don't
    /// also type into the composer (`type_anywhere`, `src/ui.rs:883`). The
    /// window gives them while a call hears the microphone.
    pub fn talk_keys(mut self, keys: &'a [Physical]) -> Self {
        self.talking = keys;
        self
    }

    /// While a keybind records, the keys go to the recorder, which hears
    /// them beside the window, and none to the window (`record`,
    /// `src/ui/keybinds.rs`): a key held to record doesn't also type, press
    /// a button or close the settings.
    pub fn take_keys(mut self, taken: bool) -> Self {
        self.keys_taken = taken;
        self
    }
}

/// Focuses `field`, if it's in reach, with its caret at the end: what
/// typing anywhere and leaving the header's search do
/// (`type-into-composer`, `place-caret(1000000)`,
/// `ui/yapfast.slint:1750`-`:1759`). A reply started from a message's menu
/// and the quick switcher closing leave the caret where it was: [`to`].
pub fn into<M: Send + 'static>(field: Id) -> Task<M> {
    operate(reach::Reach::new(field, Caret::End)).discard()
}

/// Focuses `field`, if it's in reach, selecting what it holds, as Ctrl+F
/// does to the search.
pub fn select<M: Send + 'static>(field: Id) -> Task<M> {
    operate(reach::Reach::new(field, Caret::All)).discard()
}

/// Focuses `widget`, if it's in reach, leaving its caret where it was.
pub fn to<M: Send + 'static>(widget: Id) -> Task<M> {
    operate(reach::Reach::new(widget, Caret::Kept)).discard()
}

/// Moves the focus to the next widget in reach, round from the last to
/// the first. With nothing focused it goes to the first, not to where
/// typing goes as Tab does then.
pub fn next<M: Send + 'static>() -> Task<M> {
    operate(walk::Walk::new(false)).discard()
}

/// Moves the focus as Shift+Tab does.
pub fn previous<M: Send + 'static>() -> Task<M> {
    operate(walk::Walk::new(true)).discard()
}
