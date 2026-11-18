//! The open conversation's timeline from the core's state: `show_messages`
//! (`src/ui/refresh.rs:4161`) with what it hands the window around the rows
//! (the room, the edges of the history, the bar to the "New" divider,
//! jumps). The rows themselves are [`rows`]': built once per message and
//! marked by their neighbours.
//!
//! A row holds its message built (`anomi_vm::chat::message::Message`): its
//! text parsed into spans, its times written, its pictures as handles over
//! the core's pixels; nothing of the core's message is copied besides what
//! it draws. One message coming costs that one row built, a walk over the
//! rows to mark them (comparing, not copying), and a map of the ids to find
//! where each row was; a reaction or an edit the same for its row. Who's
//! typing never reaches here (`Reach::TYPING`); a picture coming builds
//! again only the rows that show it ([`Watch`]).

mod frame;
mod moved;
mod rows;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::Arc;

use yapfast_core::application::Application;
use yapfast_core::chat::ChatMessage;

use anomi_vm::chat::message::Bar;
use anomi_vm::chat::timeline::{Edge, Jump, Timeline, Unread, group};
use anomi_widgets::list::Place;

use super::message::{self, Context, Look};
use crate::layout::Layout;
use crate::pictures::{Requests, Watch};

/// What the timeline showed last, and what the app keeps of it: what the
/// person revealed, the message the pointer is on, what the list is asked.
#[derive(Default)]
pub(crate) struct Shown {
    room: Option<String>,
    built: rows::Built,
    look: Option<Look>,
    marks: (Option<String>, Option<String>),
    /// The rooms' list as the room's name and intro were made from.
    lists: u64,
    pictures: Watch<u64>,
    /// The reaction whose people were asked for, and whether they came.
    tip: Option<(String, String, bool)>,
    /// The message asked to be confirmed deleted.
    confirming: Option<String>,
    jumped: Option<(String, u64)>,
    unread_jumped: u64,
    /// Spoilers shown, long messages opened and blocked ones shown; see
    /// `message::Context::revealed`.
    pub(crate) revealed: HashSet<String>,
    /// Rows to build again on the next show, by key.
    pub(crate) again: Vec<u64>,
    /// What this device keeps of polls and bots' controls (I08).
    pub(crate) kept: super::kept::Kept,
    /// The message the pointer is on, lit, with its bar.
    pub(crate) lit: Option<u64>,
    /// The pointer left the timeline while a menu, picker or question of
    /// a message's held its bar: it goes dark as that closes.
    pub(crate) left_held: bool,
    /// Shift is held: the lit message's bar shows Shift's actions.
    pub(crate) shift: bool,
    /// What the list is asked to do next: the app runs it after `update`.
    pub(crate) asks: Vec<Ask>,
    /// A jump marked: its light goes this many jumps on.
    pub(crate) jumps: u64,
    /// When a row's words change next by themselves, in Unix seconds:
    /// midnight, or an app's answer giving up (the day's clock,
    /// `crate::clocks`).
    pub(crate) wake: Option<u64>,
    /// The newest show after a window of older messages, once they replace
    /// it (`latest_due`).
    pub(crate) latest_due: bool,
    /// The list's last report of the rows in view.
    pub(crate) report: Option<anomi_vm::chat::timeline::Shown>,
    /// The reaction chip the pointer is on, and how many it came onto.
    pub(crate) hover: Option<(u64, String)>,
    pub(crate) hovers: u64,
}

/// The key the open room's own picture is watched by, beside the rows'.
const ROOM: u64 = 0;

/// What the timeline's list is asked to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ask {
    Jump(u64, Place),
    End,
}

impl Shown {
    /// The open room, as its rows were made for.
    pub(crate) fn room(&self) -> Option<&str> {
        self.room.as_deref()
    }

    /// The core's message the row `key` was made from.
    pub(crate) fn message(&self, key: u64) -> Option<&Arc<ChatMessage>> {
        self.built
            .messages
            .iter()
            .find(|message| group::key(&message.id) == key)
    }

    /// The core's message of the row at `index`.
    pub(crate) fn message_at(&self, index: usize) -> Option<&Arc<ChatMessage>> {
        self.built.messages.get(index)
    }

    /// The settings the rows were made with.
    pub(crate) fn look(&self) -> Option<&Look> {
        self.look.as_ref()
    }

    /// The rows whose app's answer gave up by `now`, in Unix seconds, are
    /// made again on the next show. Past midnight every row is, as the
    /// look's day changed.
    pub(crate) fn gave_up(&mut self, now: u64) {
        let Some(wake) = self.wake.filter(|&wake| wake <= now) else {
            return;
        };
        let ended = |end: u64| (wake..=now).contains(&end);
        let messages = self.built.messages.iter();
        let gave_up = messages.filter(|message| message::thinking_ends(message).is_some_and(ended));
        self.again
            .extend(gave_up.map(|message| group::key(&message.id)));
    }
}

/// Shows the selected room's messages in `timeline` while `open`; lets go
/// of every row otherwise.
pub(crate) fn show(
    core: &Application,
    layout: &Layout,
    open: bool,
    timeline: &mut Timeline,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let room = state.selected_room().filter(|_| open);
    if shown.room.as_deref() != room.map(|room| room.id.as_str()) {
        let keep = (
            std::mem::take(&mut shown.revealed),
            shown.jumped.take(),
            shown.unread_jumped,
            std::mem::take(&mut shown.kept.polls),
        );
        *timeline = Timeline::default();
        *shown = Shown::default();
        (
            shown.revealed,
            shown.jumped,
            shown.unread_jumped,
            shown.kept.polls,
        ) = keep;
        shown.room = room.map(|room| room.id.clone());
        shown.lists = state.list_changes.wrapping_sub(1);
    }
    let Some(room) = room else {
        return;
    };
    let mut again = std::mem::take(&mut shown.again);
    again.extend(shown.pictures.moved(state));
    if shown.lists != state.list_changes || timeline.room.is_none() || again.contains(&ROOM) {
        shown.lists = state.list_changes;
        let made = frame::room(core, room);
        note_room(state, room, &mut shown.pictures);
        if timeline.room.as_ref() != Some(&made) {
            timeline.room = Some(made);
        }
    }
    let now = yapfast_core::clock::now_ms() / 1000;
    let look = frame::look(core, layout, now);
    let all = shown.look.as_ref() != Some(&look);
    let marks_moved = shown.marks.0 != state.first_unread || shown.marks.1 != state.seen;
    let listed = !Arc::ptr_eq(&shown.built.messages, &state.messages);
    again.extend(moved::tip(state, shown));
    again.extend(moved::confirming(state, shown));
    again.extend(super::kept::moved(state, &mut shown.kept, now));
    if all || listed || marks_moved || !again.is_empty() {
        let cx = Context {
            state,
            look: &look,
            revealed: &shown.revealed,
            now,
            kept: &shown.kept,
        };
        rows::build(
            &cx,
            &state.messages,
            &mut timeline.rows,
            &mut shown.built,
            &again,
            all,
            &mut shown.pictures,
        );
        if all {
            note_room(state, room, &mut shown.pictures);
        }
        rows::mark(
            &mut timeline.rows,
            &shown.built,
            state.first_unread.as_deref(),
            state.seen.as_deref(),
            state.room_ready,
        );
        shown.wake = frame::wake(&state.messages, now);
        if listed || marks_moved {
            timeline.unread = unread(state, now);
        }
        if marks_moved {
            shown.marks = (state.first_unread.clone(), state.seen.clone());
        }
        shown.look = Some(look);
        light(core, timeline, shown);
        // The pictures of the rows in view, as rows come in or change.
        want(core, timeline, shown, requests);
        if listed && !state.detached && std::mem::take(&mut shown.latest_due) {
            shown.asks.push(Ask::End);
        }
    }
    if let Some(unread) = &mut timeline.unread {
        unread.caught_up = state.caught_up;
    }
    timeline.ready = state.room_ready;
    timeline.earlier = if state.loading_history {
        Edge::Loading
    } else if state.history_failed {
        Edge::Failed
    } else if state.history_start {
        Edge::Start
    } else {
        Edge::More
    };
    timeline.later = state.detached.then_some(if state.loading_later {
        Edge::Loading
    } else if state.later_failed {
        Edge::Failed
    } else {
        Edge::More
    });
    timeline.compact = layout.compact_messages;
    timeline.locked = state.busy || state.preview;
    jumps(state, timeline, shown);
}

/// A DM's picture, which the start of its history shows, is watched as the
/// rows' are.
fn note_room(
    state: &yapfast_core::application::AppState,
    room: &yapfast_core::chat::ChatRoom,
    pictures: &mut Watch<u64>,
) {
    if let Some(avatar) = room.avatar.as_deref().filter(|_| room.direct) {
        pictures.note(state, avatar, ROOM);
    }
}

/// The bar to the "New" divider, while there is one.
fn unread(state: &yapfast_core::application::AppState, now: u64) -> Option<Unread> {
    let item = state.first_unread.as_deref()?;
    let index = state
        .messages
        .iter()
        .position(|message| message.item == item)?;
    Some(Unread {
        label: frame::unread_label(&state.messages[index..], state.detached, now),
        caught_up: state.caught_up,
    })
}

/// A jump the core asked for: to a message, lit for a moment a third of the
/// way down; to the first unread one, at the top, unlit (`show_message`).
/// One to a message not loaded yet waits for it.
fn jumps(state: &yapfast_core::application::AppState, timeline: &mut Timeline, shown: &mut Shown) {
    let key_of = |item: &str| {
        state
            .messages
            .iter()
            .find(|message| message.item == item)
            .map(|message| group::key(&message.id))
    };
    if state.jump != shown.jumped {
        match &state.jump {
            None => shown.jumped = None,
            Some((item, _)) => {
                if let Some(key) = key_of(item) {
                    shown.jumped.clone_from(&state.jump);
                    timeline.jump = Some(Jump { key, lit: true });
                    shown.jumps += 1;
                    shown.asks.push(Ask::Jump(key, Place::Third));
                }
            }
        }
    }
    if state.unread_jump != shown.unread_jumped {
        shown.unread_jumped = state.unread_jump;
        if let Some(key) = state.first_unread.as_deref().and_then(key_of) {
            timeline.jump = Some(Jump { key, lit: false });
            shown.jumps += 1;
            shown.asks.push(Ask::Jump(key, Place::Top));
        }
    }
}

/// The message the pointer is on lies lit with its bar, and the one asked
/// to be confirmed deleted keeps its bar with the question; any other lies
/// as it is.
pub(crate) fn light(core: &Application, timeline: &mut Timeline, shown: &Shown) {
    let state = &core.state;
    let Some(look) = &shown.look else {
        return;
    };
    let confirming = state
        .confirm_delete
        .as_deref()
        .and_then(|item| moved::key_of_item(state, item));
    for (row, message) in timeline.rows.iter_mut().zip(shown.built.messages.iter()) {
        let lit = shown.lit == Some(row.key);
        let barred = (lit || confirming == Some(row.key)) && row.message.acts();
        let bar = barred.then(|| Bar {
            shift: shown.shift,
            ..message::bar(core, look, message)
        });
        let shown = &mut row.message;
        if shown.lit != lit {
            shown.lit = lit;
            shown.attachments.lit = lit;
        }
        if shown.bar != bar {
            shown.bar = bar;
        }
    }
}

/// Asks for the pictures of the rows the list last told were in view, with
/// its margin.
pub(crate) fn want(
    core: &Application,
    timeline: &Timeline,
    shown: &Shown,
    requests: &mut Requests,
) {
    let (Some(span), Some(look)) = (timeline.shown, &shown.look) else {
        return;
    };
    let state = &core.state;
    let (Some(top), Some(bottom)) = (timeline.index_of(span.top), timeline.index_of(span.bottom))
    else {
        return;
    };
    let margin = anomi_widgets::list::MARGIN;
    let from = top.saturating_sub(margin);
    let to = (bottom + margin).min(shown.built.messages.len().saturating_sub(1));
    let mut wanted: Vec<&str> = Vec::new();
    for message in shown.built.messages.get(from..=to).into_iter().flatten() {
        message::addresses(message, look, &shown.revealed, &mut |address| {
            wanted.push(address);
        });
    }
    requests.want(state, wanted);
    // The bar's and the menu's reactions, a server's kept as `name:id`.
    message::want_frequent(core, requests);
}
