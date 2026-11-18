//! The rows from the core's messages, as `show_timeline` builds them
//! (`src/ui/messages.rs:95`): a row is built again only for a message that
//! changed since (the core hands a changed message as a new `Arc`, the rest
//! as the same), or whose pictures, spoilers or settings did; the others
//! move from the last rows as they were. Then every row takes what the rows
//! around it say: a group, a new day, the "New" divider, a reply to this
//! account's message while it's new, what lies over it. One message coming
//! builds that one row; marking walks the rows without copying them.

use std::collections::HashMap;
use std::sync::Arc;

use yapfast_core::chat::{self, ChatMessage};
use yapfast_core::clock;

use anomi_vm::chat::message::Edge;
use anomi_vm::chat::timeline::{Row, group};

use crate::pictures::Watch;
use crate::project::chat::message::{self, Context};

/// The rows as last built, with the messages they were built from.
#[derive(Default)]
pub(super) struct Built {
    pub messages: Arc<Vec<Arc<ChatMessage>>>,
    /// Each row's local date, worked out once, as grouping reads it.
    pub dates: Vec<Option<chrono::NaiveDate>>,
}

/// Builds `rows` from `messages`: again those `again` names, or every one
/// with `all`, else only what changed. Notes the pictures of each row
/// built in `pictures`.
pub(super) fn build(
    cx: &Context<'_>,
    messages: &Arc<Vec<Arc<ChatMessage>>>,
    rows: &mut Vec<Row>,
    built: &mut Built,
    again: &[u64],
    all: bool,
    pictures: &mut Watch<u64>,
) {
    let old = std::mem::replace(&mut built.messages, messages.clone());
    let same_list = Arc::ptr_eq(&old, messages);
    // Where each message was, by id, unless the list is the one built.
    let at: HashMap<&str, usize> = if same_list {
        HashMap::new()
    } else {
        old.iter()
            .enumerate()
            .map(|(index, message)| (message.id.as_str(), index))
            .collect()
    };
    let mut earlier: Vec<Option<Row>> = std::mem::take(rows).into_iter().map(Some).collect();
    let dates = std::mem::take(&mut built.dates);
    if all {
        pictures.clear();
    }
    rows.reserve_exact(messages.len());
    built.dates.reserve_exact(messages.len());
    for (index, message) in messages.iter().enumerate() {
        let key = group::key(&message.id);
        let was = if same_list {
            Some(index)
        } else {
            at.get(message.id.as_str()).copied()
        };
        // Each id shows once; a second finds its row taken. A row taken
        // and not kept is built again, and isn't gone.
        let taken = was.and_then(|was| Some((was, earlier.get_mut(was)?.take()?)));
        let kept = taken
            .filter(|&(was, _)| !all && Arc::ptr_eq(&old[was], message) && !again.contains(&key));
        let (row, date) = match kept {
            Some((was, row)) => (row, dates[was]),
            None => {
                message::addresses(message, cx.look, cx.revealed, &mut |address| {
                    pictures.note(cx.state, address, key);
                });
                let row = Row {
                    key,
                    message: message::build(cx, message, key),
                    ..Row::default()
                };
                (row, clock::date(message.timestamp))
            }
        };
        rows.push(row);
        built.dates.push(date);
    }
    // Rows whose messages went no longer show their pictures.
    let gone: Vec<u64> = earlier.into_iter().flatten().map(|row| row.key).collect();
    if !gone.is_empty() {
        pictures.retain(|key| !gone.contains(&key));
    }
}

/// Marks each row as the rows around it say: whether it starts a group, a
/// day's date over it, the "New" divider over `first_unread`, whether it
/// calls on this account (a reply to its message only while new), and what
/// lies over its message. Changes only what differs.
pub(super) fn mark(
    rows: &mut [Row],
    built: &Built,
    first_unread: Option<&str>,
    seen: Option<&str>,
    ready: bool,
) {
    let messages = &built.messages;
    let first_new = chat::first_new(messages, seen);
    let sent = messages
        .iter()
        .zip(&built.dates)
        .map(|(message, date)| group::Sent {
            sender: if message.sender.is_empty() {
                &message.author
            } else {
                &message.sender
            },
            timestamp: message.timestamp,
            notice: message.notice.is_some(),
            reply: message.reply.is_some(),
            command: message.used.is_some(),
            date: date.as_ref(),
            unread: first_unread == Some(message.item.as_str()),
        });
    let marks = group::marks(sent);
    for (index, ((row, mark), message)) in
        rows.iter_mut().zip(marks).zip(messages.iter()).enumerate()
    {
        let new = first_unread == Some(message.item.as_str());
        let day = match (mark.new_day, row.day.take()) {
            (false, _) => None,
            (true, Some(day)) => Some(day),
            (true, None) => Some(clock::day(message.timestamp)),
        };
        row.day = day;
        row.first = mark.first;
        row.new = new;
        let edge = if index == 0 && ready || mark.new_day || new {
            Edge::Divider
        } else if index == 0 {
            Edge::Top
        } else {
            Edge::Message
        };
        let highlighted = message.highlighted(index >= first_new);
        let shown = &mut row.message;
        shown.first = mark.first;
        shown.edge = edge;
        shown.highlighted = highlighted;
    }
}
