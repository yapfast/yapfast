//! The thread beside's composer (`show_aside`'s, `src/ui/refresh.rs:3784`-
//! `:3820`): the channel's composer over the thread's draft, its reply and
//! its staged files. It takes files only (no menu, GIFs or stickers), and
//! what's suggested over it is the channel's suggestions, placed here while
//! it was typed in last (`suggest::show`). A keystroke in it reaches the
//! composer alone (`Reach::COMPOSER`), as one in the channel's does.

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{AppState, Application, Compose};
use yapfast_lang::tr;

use anomi_vm::chat::composer::{Composer, More, Reply};

use super::{Previews, Sources, fill_staged, room_key};
use crate::project::Reach;
use crate::project::chat::edit;

/// Shows the thread beside's composer as far as `reach` can have changed
/// it; empties it once the thread closes.
pub(super) fn show(
    core: &Application,
    reach: Reach,
    composer: &mut Composer,
    marked: &mut Option<(u64, Sources)>,
    previews: &Previews,
) {
    let state = &core.state;
    let Some(aside) = &state.aside else {
        if composer.room != 0 {
            *composer = Composer::default();
            *marked = None;
        }
        return;
    };
    let room = aside.room.as_str();
    if reach.touches(Reach::STATE) {
        set(&mut composer.enabled, state.can_write_in(room));
        let reply = state.aside_compose().map(
            |Compose::Reply {
                 author, body, ping, ..
             }| Reply {
                label: tr!("Replying to {}: {}", author, body),
                ping: *ping,
            },
        );
        set(&mut composer.reply, reply);
        let more = More {
            attach: state.aside_can_attach(),
            ..More::default()
        };
        set(&mut composer.more, more);
        set(&mut composer.can_spoil, state.aside_can_mark_spoilers());
        fill_staged(state.aside_staged(), &mut composer.staged, previews);
    }
    if reach.touches(Reach::STATE | Reach::TYPING) {
        set(&mut composer.typing, typing_line(state, room));
    }
    if !reach.touches(Reach::STATE | Reach::COMPOSER) {
        return;
    }
    set(&mut composer.room, room_key(Some(room)));
    let text = state.aside_draft();
    if composer.draft != text {
        text.clone_into(&mut composer.draft);
    }
    let hashed = {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    };
    let sources = Sources::of(state);
    if *marked != Some((hashed, sources)) {
        *marked = Some((hashed, sources));
        composer.marks = edit::marks(state, text);
    }
    let name = state.aside_room().map_or("", |listed| listed.name.as_str());
    let placeholder = if state.aside_compose().is_some() {
        tr!("Write a reply").to_owned()
    } else {
        tr!("ThreadPanel" => "Message {}", name)
    };
    if composer.placeholder != placeholder {
        composer.placeholder = placeholder;
    }
}

/// Sets `field` to `value` unless it holds it already.
fn set<T: PartialEq>(field: &mut T, value: T) {
    if *field != value {
        *field = value;
    }
}

/// Who's typing in the thread, worded as the channel's line
/// (`AppState::typing_line`, which reads the channel's room alone).
fn typing_line(state: &AppState, room: &str) -> String {
    let names = state
        .typing
        .get(room)
        .map(Vec::as_slice)
        .unwrap_or_default();
    match names {
        [] => String::new(),
        [one] => tr!("{} is typing...", one),
        [one, two] => tr!("{} and {} are typing...", one, two),
        [one, two, three] => tr!("{}, {} and {} are typing...", one, two, three),
        _ => tr!("Several people are typing...").into(),
    }
}
