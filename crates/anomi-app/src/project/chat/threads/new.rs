//! Starting a thread from the core's state (`show_new_thread`,
//! `src/ui/refresh.rs:1789`): Discord's "New Thread" form beside the open
//! channel, with the message it starts from as a found message's card, its
//! name, whether it's private, and its first message. The fields are the
//! window's while it's open: they're set only when the core's differ, so
//! the caret stays. Its first message takes the keys as it opens, as
//! Slint's `focus-composer` did.

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::AppState;
use yapfast_lang::tr;

use anomi_vm::chat::new_thread::NewThread;

use crate::layout::Layout;
use crate::pictures::{Requests, Watch};
use crate::project::chat::search::found;

/// What the form showed last: which form, and what its starter was built
/// from.
#[derive(Default)]
pub(crate) struct Shown {
    open: Option<u64>,
    starter: Option<u64>,
    pictures: Watch<()>,
}

pub(crate) fn show(
    state: &AppState,
    layout: &Layout,
    form: &mut NewThread,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let Some(open) = state.new_thread() else {
        if shown.open.is_some() {
            *form = NewThread::default();
            *shown = Shown::default();
        }
        return;
    };
    let key = {
        let mut hasher = DefaultHasher::new();
        (&open.room, &open.message).hash(&mut hasher);
        hasher.finish().max(1)
    };
    if shown.open != Some(key) {
        shown.open = Some(key);
        shown.starter = None;
        form.key = key;
        form.focused = false;
        *focus = Some(ASIDE_COMPOSER);
    }
    set(&mut form.name, &open.name);
    set(&mut form.body, &open.body);
    let placeholder = match open.default_name.as_str() {
        "" => tr!("New Thread"),
        name => name,
    };
    set(&mut form.placeholder, placeholder);
    form.can_private = open.can_private;
    form.only_private = open.only_private;
    form.private = open.private;
    form.starting = open.starting;
    set(&mut form.error, open.error.as_deref().unwrap_or_default());
    form.leaving = open.leaving;
    let starter = (open.message.as_deref()).and_then(|item| state.message(item));
    let Some(starter) = starter else {
        form.starter = None;
        return;
    };
    let print = {
        let mut hasher = DefaultHasher::new();
        found::print(starter, &mut hasher);
        layout.twelve_hour_clock.hash(&mut hasher);
        hasher.finish()
    };
    let moved = !shown.pictures.moved(state).is_empty();
    if shown.starter == Some(print) && !moved && form.starter.is_some() {
        return;
    }
    shown.starter = Some(print);
    let avatar = starter.avatar.as_deref().unwrap_or_default();
    shown.pictures.clear();
    shown.pictures.note(state, avatar, ());
    requests.want(state, [avatar]);
    let now = yapfast_core::clock::now_ms() / 1000;
    form.starter = Some(anomi_vm::chat::search::Found {
        room: open.room.clone(),
        ..found::found(state, starter, layout.twelve_hour_clock, now)
    });
}

/// The first message's field, by the name its focus reaches it by.
const ASIDE_COMPOSER: &str = "aside-composer";

/// Sets `field` to `value` unless it holds it already.
fn set(field: &mut String, value: &str) {
    if field != value {
        value.clone_into(field);
    }
}
