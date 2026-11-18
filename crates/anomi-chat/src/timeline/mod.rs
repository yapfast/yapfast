//! The timeline: the open conversation's messages over the composer. The
//! Slint side: the timeline of `YapfastWindow` (`ui/yapfast.slint:3646`-
//! `:4105`: its `FocusScope`, `ListView` and rows, the bars over it, and
//! what stands in for it while there are no messages), with
//! `NoConversation`, `HistoryEdge`, `UnreadBar`, `PresentBar` and
//! `ConversationStart` (`ui/messages.slint:2373`-`:2556`).
//!
//! - **What shows.** No place chosen: what to do ([`start`]). A place
//!   whose history hasn't come: skeleton messages. One with no messages:
//!   its start. Otherwise the rows, each a message (C02's
//!   `message::view`) with what stands over it ([`row`]): the start of
//!   the history or the button to load more above the first ([`edge`]), a
//!   day's date and the "New" divider.
//! - **The list.** Only the rows near the view are built (`list`), laid
//!   out from the newest, which it follows while it shows; a window of
//!   older messages isn't followed. Older rows going in above leave the
//!   view where it was. The keyboard's message, or the one edited in place,
//!   is kept in view.
//! - **Telling.** The list reports the rows in view with six of margin at
//!   most every 50 ms ([`Intent::Shown`]), and whether it nears its start
//!   or its end, from which the app loads more where an edge has it
//!   ([`Timeline::wants_earlier`]).
//! - **The bars** ([`bars`]): over the messages, how much is new while the
//!   "New" divider is out of view, which jumps to it; under a window of
//!   older messages, the way back to the newest. Both arrive as popups do
//!   ([`grow`]).
//! - **Keys** ([`keys`]): while the timeline has the keyboard
//!   ([`focus::TIMELINE`](anomi_widgets::focus::TIMELINE)), Up and Down
//!   step through the messages and letters act on the one stepped to.
//!
//! Jumps are the app's: `anomi_widgets::list::operation::jump_to` on
//! [`list_id`], at a third of the height for a lit message, at the top for
//! the first unread one; [`Timeline::jump`] lights it.

mod bars;
mod chevron;
mod edge;
mod glow;
mod grow;
mod keys;
mod row;
#[cfg(feature = "shots")]
mod shots;
mod start;

use iced::Padding;
use iced::advanced::widget::Id;
use iced::widget::stack;

use anomi_theme::prelude::*;
use anomi_vm::chat::{
    self,
    timeline::{Intent, Row, Shown, Span, Timeline},
};
use anomi_widgets::list::{self, list};
use anomi_widgets::skeleton::messages_skeleton;

pub use row::ACTIVE;
#[cfg(feature = "shots")]
pub use shots::shots;

/// Room kept around the keyboard's message, so the next one is in view
/// (`keyboard-margin`, `ui/yapfast.slint:755`). Slint takes a third of a
/// view shorter than 480 px instead; the list takes its margin in pixels,
/// and the window's least height leaves the timeline about 430.
const KEYBOARD_MARGIN: f32 = 160.0;
/// Room under the newest message, and under a window of older ones, where
/// the bar back to the newest sits (`padding-bottom`, `:3857`).
const END: f32 = 20.0;
const DETACHED_END: f32 = 52.0;

/// The timeline's list in a room: another room's starts afresh, and the
/// app's jumps name it.
pub fn list_id(room: &str) -> Id {
    Id::from(format!("timeline-{room}"))
}

pub fn view(timeline: &Timeline, motion: Motion) -> impl View<chat::Intent> + '_ {
    let shown: Element<'_, chat::Intent> = match &timeline.room {
        None => start::nothing().boxed(),
        Some(_) if timeline.rows.is_empty() && !timeline.ready => messages_skeleton(motion).boxed(),
        Some(room) if timeline.rows.is_empty() => start::start(room).boxed(),
        Some(room) => keys::keys(
            stack![
                rows(timeline, &room.id, motion),
                bars::unread(timeline, motion),
                bars::present(timeline, motion),
            ],
            timeline,
        )
        .boxed(),
    };
    shown
}

/// The messages, built near the view.
fn rows<'a>(timeline: &'a Timeline, room: &str, motion: Motion) -> impl View<chat::Intent> + 'a {
    let detached = timeline.detached();
    let kept = timeline.editing.map(|key| (key, 0.0)).or(timeline
        .keys
        .active
        .filter(|_| timeline.keys.browsing)
        .map(|key| (key, KEYBOARD_MARGIN)));
    let list = list(
        timeline.rows.len(),
        |index| timeline.rows[index].key,
        move |index| row::view(timeline, index, motion),
        motion,
    )
    .id(list_id(room))
    .from_end();
    let list = if timeline.aside {
        list.short_at_start()
    } else {
        list
    };
    list.follow(!detached)
        .padding(Padding::ZERO.bottom(if detached { DETACHED_END } else { END }))
        .reveal(
            kept.map(|(key, _)| key),
            kept.map_or(0.0, |(_, margin)| margin),
        )
        .on_viewport(move |viewport| {
            chat::Intent::from(Intent::Shown(shown(&timeline.rows, viewport)))
        })
        .on_leave(|| chat::Intent::from(Intent::Left))
}

/// What the list told of `rows`, the rows it was built with: the span
/// names them by key, which stays true as rows come in before the next
/// report.
fn shown(rows: &[Row], viewport: list::Viewport) -> Shown {
    Shown {
        span: Span {
            near_start: viewport.near_start,
            near_end: viewport.near_end,
            ..Span::new(rows, viewport.first, viewport.last)
        },
        keys: viewport.keys,
        at_end: viewport.at_end,
        following: viewport.following,
    }
}
