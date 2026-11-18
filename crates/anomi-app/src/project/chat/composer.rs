//! The channel's composer from the core's state: `show_composer`
//! (`src/ui/refresh.rs:3951`), the emoji the `:name` being typed may mean
//! (`show_emoji_suggestions`, `:4127`), the mentions and the slash
//! commands over the field (`show_app_commands`, `src/ui/app_commands.rs`),
//! and the suggestions over the message edited in place
//! (`show_edit_suggesting`, `:4149`).
//!
//! What reaches what, as the Slint `Adapter` narrowed it: who's typing
//! ([`Reach::TYPING`]) reaches the typing line alone; a keystroke
//! ([`Reach::COMPOSER`]) the draft, its marks, the placeholder and what is
//! suggested; pictures the suggestions' avatars, emoji and app icons; and
//! anything else the frame too (its buttons, hint, reply and files).
//!
//! The words being typed (the `@` or `#` word and the `:name` at the caret,
//! which field was typed in last, what Escape closed) are the intents'
//! (`crate::intent::chat::composer`), kept here as [`Words`] for the
//! suggestions to be built from.

mod commands;
mod frame;
mod staged;
pub(crate) mod suggest;
#[cfg(test)]
mod tests;

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{AppState, Application};

use anomi_vm::chat::Chat;
use anomi_vm::chat::composer::Composer;

use super::edit;
use crate::layout::Layout;
use crate::pictures::Requests;
use crate::project::Reach;

pub(crate) use staged::Previews;
pub(crate) use suggest::{Field, Words};

/// What the composer showed last, and the words being typed.
#[derive(Default)]
pub(crate) struct Shown {
    pub(crate) words: Words,
    /// The draft and the sources of the people it may mention, as marked.
    marks: Option<(u64, Sources)>,
    /// What the suggestions were built from.
    offer: Option<suggest::Key>,
    /// The staged files' pictures, by path.
    pub(crate) previews: Previews,
    /// Files dropped on the window, while the rest dropped with them come.
    pub(crate) dropped: Vec<std::path::PathBuf>,
    /// The core's count of rooms opened, as last seen, and whether the
    /// composer takes the keys once the room opened is ready.
    selection: Option<u64>,
    focus_when_ready: bool,
}

/// What a draft's mentions are found among: who's listed, found and wrote
/// lately, and the roles and groups (`MentionSources`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sources {
    messages: usize,
    members: usize,
    found: usize,
    groups: usize,
}

impl Sources {
    pub(crate) fn of(state: &AppState) -> Self {
        Self {
            messages: std::sync::Arc::as_ptr(&state.messages) as usize,
            members: state
                .members
                .iter()
                .flatten()
                .map(|members| members.len())
                .sum(),
            found: state.found_members.len(),
            groups: state.mention_groups.len(),
        }
    }
}

/// Shows the composer as far as `reach` can have changed it.
pub(crate) fn show(
    core: &Application,
    layout: &Layout,
    reach: Reach,
    chat: &mut Chat,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    let composer = &mut chat.composer;
    if reach.touches(Reach::STATE) {
        take_keys(state, shown, focus);
        frame::show(core, layout, composer);
        staged::show(state, composer, &shown.previews);
        // A sent message, or another room's draft, ends the `:name` word;
        // an edit that ended, the words typed in it.
        shown.words.settle(state);
    }
    if reach.touches(Reach::STATE | Reach::TYPING) {
        let line = state.typing_line();
        if composer.typing != line {
            composer.typing = line;
        }
    }
    if reach.touches(Reach::STATE | Reach::COMPOSER) {
        draft(state, composer, &mut shown.marks);
    }
    if reach.touches(Reach::STATE | Reach::COMPOSER | Reach::PICTURES | Reach::EMOJI) {
        commands::show(core, &shown.words, composer, requests);
        suggest::show(core, chat, shown, requests);
    }
}

/// A room opened gives the composer the keys, as Discord does, or once it
/// is ready, if it shows text (`src/ui/refresh.rs:580`-`:588`).
fn take_keys(state: &AppState, shown: &mut Shown, focus: &mut Option<&'static str>) {
    let opened = shown.selection != Some(state.selection_version);
    if opened {
        shown.selection = Some(state.selection_version);
        shown.focus_when_ready = state.shows_text() && !state.room_ready;
    }
    if opened && state.shows_text() && state.selected.is_some() {
        *focus = Some(COMPOSER);
    } else if shown.focus_when_ready && state.room_ready {
        shown.focus_when_ready = false;
        *focus = Some(COMPOSER);
    }
}

/// The channel's composer field, by the name its focus reaches it by.
const COMPOSER: &str = "composer";

/// The staged files again, as a picture of one came.
pub(crate) fn show_staged(state: &AppState, composer: &mut Composer, shown: &Shown) {
    staged::show(state, composer, &shown.previews);
}

/// Whose draft the field shows, as the room's id hashed: a new one shows
/// its draft afresh.
pub(crate) fn room_key(room: Option<&str>) -> u64 {
    let Some(room) = room else {
        return 0;
    };
    let mut hasher = DefaultHasher::new();
    room.hash(&mut hasher);
    hasher.finish().max(1)
}

/// The draft, whose it is, what it mentions, and what the empty field says.
fn draft(state: &AppState, composer: &mut Composer, marked: &mut Option<(u64, Sources)>) {
    let key = room_key(state.selected.as_deref());
    if composer.room != key {
        composer.room = key;
    }
    let text = state.draft();
    if composer.draft != text {
        text.clone_into(&mut composer.draft);
    }
    let sources = Sources::of(state);
    let hashed = {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    };
    if *marked != Some((hashed, sources)) {
        *marked = Some((hashed, sources));
        composer.marks = edit::marks(state, text);
    }
    let placeholder = frame::placeholder(state);
    if composer.placeholder != placeholder {
        composer.placeholder = placeholder.into_owned();
    }
}
