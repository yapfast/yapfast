//! The messages in the timeline, as the golden screenshots show them: the
//! timeline's samples (C01's) with each row's message filled in.

use yapfast_lang::tr;

use crate::chat::timeline::group::key;
use crate::chat::timeline::{Row, Timeline};
use crate::fixtures::chat::timeline::{self as rows, FIRST, day, message_key};

use super::{
    Message, alice, article, blocked, bob_in_dm, bobs, call, command, compact, failed, forwarded,
    lit, long, markdown, mention, pinned, reply, thinking, with_thread,
};

/// #general scrolled up into Bob's history (`chan/history_plain_a`).
pub fn history() -> Timeline {
    let mut timeline = rows::history();
    for (index, row) in timeline.rows.iter_mut().enumerate() {
        row.message = bobs(FIRST + index, row.first);
    }
    timeline
}

/// [`history`] with message 46 under the pointer: its time in the gutter,
/// its bar over it.
pub fn history_lit() -> Timeline {
    let mut timeline = history();
    let lit_key = message_key(46);
    if let Some(row) = timeline.rows.iter_mut().find(|row| row.key == lit_key) {
        row.message = lit(row.message.clone());
    }
    timeline
}

/// The showcase further down #general: a spoiler, Bob's reply to the
/// welcome under the pointer, a blocked person's message and Bob's link
/// (`chan/spoiler_reply_blocked_linkembed`).
pub fn blocked_showcase() -> Timeline {
    let look = Message {
        key: key("dont-look"),
        body: markdown("Don't look"),
        time: "01:10".into(),
        short_time: "01:10".into(),
        ..bobs(9, true)
    };
    let messages = [look, lit(reply()), blocked(), article()];
    Timeline {
        rows: messages.into_iter().map(row).collect(),
        shown: None,
        ..rows::general()
    }
}

/// Bob's DM in a call: the call's notice under the "New" divider
/// (`dmcall/connected_bob_dm`).
pub fn call_in_dm() -> Timeline {
    let mut timeline = rows::bob();
    let said = [
        ("trip-notes", "Did you see the trip notes?", "16:02", true),
        ("snacks", "Yes, bringing snacks", "16:03", true),
        ("great", "Great", "16:04", true),
        ("nine", "See you at nine", "16:04", false),
    ];
    for (row, (id, body, time, first)) in timeline.rows.iter_mut().zip(said) {
        row.new = false;
        row.message = Message {
            key: key(id),
            first,
            author: if id == "snacks" { alice() } else { bob_in_dm() },
            time: format!("02.10.2026 {time}"),
            short_time: time.into(),
            body: markdown(body),
            ..Message::default()
        };
    }
    let mut notice = row(call());
    notice.day = Some(day(17, tr!("November"), 2026));
    notice.new = true;
    timeline.rows.push(notice);
    timeline.unread = None;
    timeline
}

/// [`history_lit`] with Shift held over Alice's own message 46: Pin,
/// Create Thread and Reply before the picker, Edit, Forward, and Delete in
/// More's place.
pub fn shift_held() -> Timeline {
    with_bar(|message| {
        message.author = alice();
        if let Some(bar) = &mut message.bar {
            bar.shift = true;
            bar.may.edit = true;
            bar.may.delete = true;
        }
    })
}

/// [`history_lit`] asking before message 46 goes: "Delete for everyone?".
pub fn confirming() -> Timeline {
    with_bar(|message| {
        if let Some(bar) = &mut message.bar {
            bar.confirming = true;
        }
    })
}

/// Bob's photos under the pointer, with their gallery (C04's), as
/// `chan/gallery_hoverbar` scrolled up to them.
pub fn gallery() -> Timeline {
    let photos = Message {
        key: key("gallery"),
        body: markdown("Three from today"),
        ..bobs(30, false)
    };
    let messages = [bobs(28, true), bobs(29, false), lit(photos), bobs(31, true)];
    Timeline {
        rows: messages.into_iter().map(row).collect(),
        shown: None,
        ..rows::general()
    }
}

/// One of each kind: a notice, a forward, an app's answer only Alice
/// sees, an app thinking and one given up on, a long body folded, a
/// mention, a message that didn't go, one with its thread.
pub fn kinds() -> Timeline {
    let messages = [
        pinned(),
        forwarded(),
        command(),
        thinking(false),
        thinking(true),
        long(false),
        mention(),
        failed(),
        with_thread(),
    ];
    Timeline {
        rows: messages.into_iter().map(row).collect(),
        shown: None,
        ..rows::general()
    }
}

/// [`kinds`] in the compact display.
pub fn kinds_compact() -> Timeline {
    let mut timeline = kinds();
    for row in &mut timeline.rows {
        row.message = compact(row.message.clone());
    }
    timeline
}

fn with_bar(change: impl FnOnce(&mut Message)) -> Timeline {
    let mut timeline = history_lit();
    let lit_key = message_key(46);
    if let Some(row) = timeline.rows.iter_mut().find(|row| row.key == lit_key) {
        change(&mut row.message);
    }
    timeline
}

fn row(message: Message) -> Row {
    Row {
        key: message.key,
        first: message.first,
        message,
        ..Row::default()
    }
}
