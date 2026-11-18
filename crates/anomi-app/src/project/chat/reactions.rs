//! A message's reactions under it, and the Reactions window: the chips of
//! `message_row` (`src/ui/messages.rs:821`) with the tooltip of whoever
//! reacted (`reaction_tip`, `:1225`), and `show_reactions`
//! (`src/ui/refresh.rs:1838`).

use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatMessage, Reaction, Reacts};
use yapfast_core::emoji;
use yapfast_lang::{tr, trn};

use anomi_vm::chat::reactions::{Chip, Dialog, Face, Pill, Reactions, Reactor};
use anomi_vm::shell::{Layer, Shell};

use super::message::Context;
use crate::pictures::{Requests, Watch};
use crate::project::rows;

/// A message's reactions, each with its emoji, its count and whether this
/// account reacted; the one the pointer stayed on says who reacted.
pub(super) fn chips(cx: &Context<'_>, message: &ChatMessage, key: u64, acts: bool) -> Reactions {
    let state = cx.state;
    let tip = state.reaction_tip();
    Reactions {
        item: key,
        chips: message
            .reactions
            .iter()
            .map(|reaction| Chip {
                key: reaction.key.clone(),
                face: face(state, message, &reaction.key),
                name: emoji::shortcode(&reaction.key),
                count: reaction.count as u64,
                mine: reaction.mine,
                burst: None,
                who: match tip {
                    Some((tip, counted)) if tip.item == message.item && tip.key == reaction.key => {
                        tip.names
                            .as_deref()
                            .map(|names| who(&emoji::shortcode(&tip.key), names, counted.count))
                            .unwrap_or_default()
                    }
                    _ => String::new(),
                },
            })
            .collect(),
        enabled: acts && cx.look.reacts != Reacts::None,
    }
}

/// What draws a reaction's emoji: a server's by its picture, among the
/// message's custom emoji, keeping its room while it comes; Unicode's as its
/// glyphs (`custom_emoji::text` of the key).
fn face(state: &AppState, message: &ChatMessage, key: &str) -> Face {
    // A server's emoji reacts as `:name:`, as the message's emoji name it.
    let custom = message.emoji.iter().find(|emoji| emoji.text == key);
    match custom {
        Some(emoji) => Face::Picture(rows::expected(state, Some(&emoji.picture))),
        None => Face::Glyphs(key.to_owned()),
    }
}

/// What a reaction's tooltip says, as Discord's client words it: the
/// emoji's name, then up to three of the `count` who reacted by name and how
/// many more (`reaction_tip`).
pub(crate) fn who(emoji: &str, names: &[String], count: usize) -> String {
    let others = count.saturating_sub(names.len());
    let more = trn!("{n} other", "{n} others", others);
    let people = match (names, others) {
        ([one], 0) => one.clone(),
        ([one], _) => tr!("{} and {}", one, more),
        ([one, two], 0) => tr!("{} and {}", one, two),
        ([one, two], _) => tr!("{}, {}, and {}", one, two, more),
        ([one, two, three], 0) => tr!("{}, {}, and {}", one, two, three),
        ([one, two, three, ..], _) => tr!("{}, {}, {}, and {}", one, two, three, more),
        _ => trn!("{n} person", "{n} people", count),
    };
    tr!("{} reacted by {}", emoji, people)
}

/// What the Reactions window showed last.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<Key>,
    pictures: Watch<()>,
}

#[derive(PartialEq)]
struct Key {
    item: String,
    chosen: String,
    poll: bool,
    people: usize,
    loading: bool,
    error: Option<String>,
    more: bool,
    messages: usize,
}

/// The Reactions window, while the core holds it open; its tabs and people
/// are made again as they change, or their pictures come.
pub(crate) fn dialog(
    state: &AppState,
    shell: &mut Shell,
    dialog: &mut Dialog,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let Some(window) = state.reactions.as_ref() else {
        if shell.is_open(Layer::Reactions) {
            shell.close(Layer::Reactions);
        }
        shown.key = None;
        return;
    };
    if !shell.is_open(Layer::Reactions) {
        shell.open(Layer::Reactions);
    }
    let key = Key {
        item: window.item.clone(),
        chosen: window.key.clone(),
        poll: window.poll,
        people: window.people.len(),
        loading: window.loading,
        error: window.error.clone(),
        more: state.more_reactors(),
        messages: Arc::as_ptr(&state.messages).addr(),
    };
    let moved = !shown.pictures.moved(state).is_empty();
    if !moved && shown.key.as_ref() == Some(&key) {
        return;
    }
    let message = state.message(&window.item);
    let face_of = |key: &str| match message {
        Some(message) => face(state, message, key),
        None => Face::Glyphs(key.to_owned()),
    };
    dialog.pills = if window.poll {
        state
            .window_answers()
            .iter()
            .map(|answer| Pill {
                key: answer.id.to_string(),
                face: answer.emoji.as_deref().map(face_of),
                name: answer.text.clone(),
                count: answer.votes as u64,
            })
            .collect()
    } else {
        state
            .window_reactions()
            .into_iter()
            .map(|reaction: &Reaction| Pill {
                key: reaction.key.clone(),
                face: Some(face_of(&reaction.key)),
                name: String::new(),
                count: reaction.count as u64,
            })
            .collect()
    };
    dialog.voters = window.poll;
    dialog.chosen.clone_from(&window.key);
    shown.pictures.clear();
    dialog.people = window
        .people
        .iter()
        .map(|person| {
            shown.pictures.note(state, person.avatar.as_str(), ());
            Reactor {
                id: person.id.clone(),
                name: person.name.clone(),
                username: person.username.clone(),
                avatar: rows::avatar(state, &person.name, &person.name, Some(&person.avatar)),
                removable: !window.poll && (person.me || window.can_remove),
            }
        })
        .collect();
    requests.want(
        state,
        window.people.iter().map(|person| person.avatar.as_str()),
    );
    if let Some(message) = message {
        for emoji in &message.emoji {
            shown.pictures.note(state, &emoji.picture, ());
        }
        requests.want(
            state,
            message.emoji.iter().map(|emoji| emoji.picture.as_str()),
        );
    }
    dialog.loading = window.loading;
    dialog.more = key.more;
    dialog.error.clone_from(&window.error);
    shown.key = Some(key);
}

/// The window played its way out: its rows go.
pub(crate) fn gone(dialog: &mut Dialog, shown: &mut Shown) {
    *dialog = Dialog::default();
    *shown = Shown::default();
}
