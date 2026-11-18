//! The thread beside the open channel, a timeline of its own (`show_aside`,
//! `src/ui/refresh.rs:3769`): its rows built by the channel's [`rows`] over
//! the thread's messages, with a [`Shown`] of its own (its look, the
//! pictures its rows show, what was revealed in it, the message the pointer
//! is on), so a message coming to one side builds nothing on the other. It
//! has no "New" divider, no bar to the unread and no window of older
//! messages; its earlier messages load at its top (`AsideEarlier`). The
//! hover bar offers two frequent reactions, the panel being narrower
//! (`light`). Its composer is the channel's over the thread's draft
//! (`crate::project::chat::composer::aside`).

use std::sync::Arc;

use yapfast_core::application::Application;

use anomi_vm::chat::thread_panel::ThreadPanel;
use anomi_vm::chat::timeline::{Edge, Kind, Room, Timeline};
use anomi_vm::icon::Icon;

use super::{Shown, frame, light, moved, rows, want};
use crate::layout::Layout;
use crate::pictures::Requests;
use crate::project::chat::message::{Context, Look};

/// Shows the thread beside the channel in `panel`; lets go of its rows
/// once it closes.
pub(crate) fn show(
    core: &Application,
    layout: &Layout,
    panel: &mut ThreadPanel,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let Some(aside) = &state.aside else {
        if shown.room.is_some() {
            *panel = ThreadPanel::default();
            *shown = Shown::default();
        }
        return;
    };
    if shown.room.as_deref() != Some(aside.room.as_str()) {
        let kept = std::mem::take(&mut shown.kept.polls);
        panel.timeline = Timeline::default();
        *shown = Shown::default();
        shown.kept.polls = kept;
        shown.room = Some(aside.room.clone());
        shown.lists = state.list_changes.wrapping_sub(1);
    }
    let timeline = &mut panel.timeline;
    timeline.aside = true;
    if shown.lists != state.list_changes || timeline.room.is_none() {
        shown.lists = state.list_changes;
        let name = state.aside_room().map_or("", |room| room.name.as_str());
        if panel.name != name {
            name.clone_into(&mut panel.name);
        }
        let room = Room {
            id: aside.room.clone(),
            name: name.to_owned(),
            kind: Kind::Thread(
                state
                    .aside_room()
                    .map_or(Icon::Thread, crate::project::rows::channel_icon),
            ),
            intro: None,
        };
        if timeline.room.as_ref() != Some(&room) {
            timeline.room = Some(room);
        }
    }
    let now = yapfast_core::clock::now_ms() / 1000;
    let look = look(core, layout, &aside.room, now);
    let all = shown.look.as_ref() != Some(&look);
    let marks_moved = shown.marks.1 != aside.seen;
    let listed = !Arc::ptr_eq(&shown.built.messages, &aside.messages);
    let mut again = std::mem::take(&mut shown.again);
    again.extend(shown.pictures.moved(state));
    again.extend(moved::tip(state, &aside.messages, shown));
    again.extend(moved::confirming(state, &aside.messages, shown));
    again.extend(crate::project::chat::kept::moved(
        state,
        &aside.messages,
        &mut shown.kept,
        now,
    ));
    if all || listed || marks_moved || !again.is_empty() {
        let cx = Context {
            state,
            look: &look,
            revealed: &shown.revealed,
            now,
            kept: &shown.kept,
        };
        let (rows, built) = (&mut timeline.rows, &mut shown.built);
        rows::build(
            &cx,
            &aside.messages,
            rows,
            built,
            &again,
            all,
            &mut shown.pictures,
        );
        rows::mark(rows, built, None, aside.seen.as_deref(), aside.ready);
        shown.wake = frame::wake(&aside.messages, now);
        shown.marks.1.clone_from(&aside.seen);
        shown.look = Some(look);
        light(core, timeline, shown);
        want(core, timeline, shown, requests);
    }
    timeline.ready = aside.ready;
    timeline.earlier = if aside.loading_history {
        Edge::Loading
    } else if aside.history_failed {
        Edge::Failed
    } else if aside.history_start {
        Edge::Start
    } else {
        Edge::More
    };
    timeline.later = None;
    timeline.compact = layout.compact_messages;
    timeline.locked = state.busy || state.preview;
}

/// The channel's look, with what this account may do in the thread.
fn look(core: &Application, layout: &Layout, room: &str, now: u64) -> Look {
    let state = &core.state;
    Look {
        writes: state.can_write_in(room),
        reacts: state.reacts_in(room),
        // No thread starts inside a thread.
        threads: false,
        ..frame::look(core, layout, now)
    }
}
