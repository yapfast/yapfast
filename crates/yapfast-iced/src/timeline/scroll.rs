//! Where the timeline is scrolled to, and keeping it there: rows measured
//! as they're built, the row someone reads kept in place, the end, a row
//! jumped to.

use iced::{
    Task,
    widget::{operation, scrollable::AbsoluteOffset},
};

use crate::app::{App, Message, key};

use super::{Event, JUMP_LIT, TIMELINE};

/// Heights the timeline's rows found at their last layout, which the
/// layout already made room for (`Rows::measured`): the rows keep them.
/// Taken in before each message, so the view built next places rows
/// where they were drawn.
pub fn take_found(app: &mut App) {
    let found = std::mem::take(&mut *app.timeline.found.borrow_mut());
    if found.is_empty() {
        return;
    }
    let (first, last) = app.timeline.in_view(crate::sync::MARGIN);
    let mut changed = false;
    for (row, height) in found {
        if let Some(index) =
            (first..last).find(|&index| key(&app.timeline.messages[index].item) == row)
            && (app.timeline.heights[index] - height).abs() > 0.5
        {
            app.timeline.heights[index] = height;
            changed = true;
        }
    }
    if changed {
        app.place_rows();
    }
}

/// The rows built in and below the view of someone reading earlier
/// ones report the heights they took: the rows keep them, and the view
/// stays on what's read ([`timeline::scroll::keep_reading`]).
pub fn measured(app: &mut App, heights: Vec<(u64, f32)>) -> Task<Message> {
    let (first, last) = app.timeline.in_view(crate::sync::MARGIN);
    let reading = reading(app);
    let mut changed = false;
    for (row, height) in heights {
        let Some(index) =
            (first..last).find(|&index| key(&app.timeline.messages[index].item) == row)
        else {
            continue;
        };
        if (app.timeline.heights[index] - height).abs() <= 0.5 {
            continue;
        }
        app.timeline.heights[index] = height;
        changed = true;
    }
    if !changed {
        return Task::none();
    }
    app.place_rows();
    Task::batch([keep_reading(app, reading), app.report()])
}

/// While someone reads earlier messages, the row at the view's top and
/// how far into it the view starts; at the end, none (the scrollable,
/// anchored to the bottom, follows the end).
pub fn reading(app: &App) -> Option<(String, f32)> {
    if app.timeline.at_end || app.timeline.messages.is_empty() {
        return None;
    }
    let top = app.timeline.view_top();
    let index = app.timeline.tops[1..]
        .partition_point(|end| *end <= top)
        .min(app.timeline.messages.len() - 1);
    Some((
        app.timeline.messages[index].item.clone(),
        top - app.timeline.tops[index],
    ))
}

/// Puts the row [`timeline::scroll::reading`] found back where it was in the view,
/// whatever came, went or changed height above or below it, as Discord
/// keeps what's read in place. The scrollable is moved by the difference
/// rather than to a place, so moves asked for before it reported where
/// it is add up.
pub fn keep_reading(app: &mut App, reading: Option<(String, f32)>) -> Task<Message> {
    let Some((item, into)) = reading else {
        return Task::none();
    };
    let Some(index) = app
        .timeline
        .messages
        .iter()
        .position(|message| message.item == item)
    else {
        return Task::none();
    };
    let top = app.timeline.tops[index] + into;
    let from_end = (app.timeline.total_height() - app.timeline.height - top).max(0.0);
    let by = from_end - app.timeline.from_end;
    if by.abs() <= 0.5 {
        return Task::none();
    }
    app.timeline.from_end = from_end;
    operation::scroll_by(TIMELINE, AbsoluteOffset { x: 0.0, y: by })
}

/// Goes to the newest message.
pub fn go_to_end(app: &mut App) -> Task<Message> {
    app.timeline.from_end = 0.0;
    app.timeline.at_end = true;
    operation::scroll_to(
        TIMELINE,
        AbsoluteOffset {
            x: None,
            y: Some(0.0),
        },
    )
}

/// Brings row `index` into view: its top at the view's top, or a third
/// of the way down.
pub fn scroll_to_row(app: &mut App, index: usize, third: bool) -> Task<Message> {
    let lead = if third {
        app.timeline.height / 3.0
    } else {
        0.0
    };
    let top = app.timeline.tops[index] - lead;
    app.timeline.from_end = (app.timeline.total_height() - app.timeline.height - top).max(0.0);
    app.timeline.at_end = app.timeline.from_end <= 2.0;
    operation::scroll_to(
        TIMELINE,
        AbsoluteOffset {
            x: None,
            y: Some(app.timeline.from_end),
        },
    )
}

/// Where the core asked the timeline to go, once it has the message:
/// a jump (tinted a moment) or the first unread message.
pub fn follow_jumps(app: &mut App) -> Task<Message> {
    let state = app.client.state();
    let jump = state.jump.clone();
    let unread_jump = state.unread_jump;
    let mut tasks = Vec::new();
    if jump != app.timeline.jumped
        && let Some((item, _)) = &jump
        && let Some(index) = app
            .timeline
            .messages
            .iter()
            .position(|message| &message.item == item)
    {
        app.timeline.jumped.clone_from(&jump);
        app.timeline.jump_lit = Some(item.clone());
        tasks.push(scroll_to_row(app, index, true));
        tasks.push(crate::after(
            JUMP_LIT,
            Message::Timeline(Event::JumpFaded(item.clone())),
        ));
    }
    if unread_jump != app.timeline.unread_jumped
        && let Some(index) = app.timeline.unread_index(app.client.state())
    {
        app.timeline.unread_jumped = unread_jump;
        tasks.push(scroll_to_row(app, index, false));
    }
    Task::batch(tasks)
}
