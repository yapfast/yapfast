//! Where the timeline is scrolled to, and keeping it there: rows measured
//! as they're built, the row someone reads kept in place, the end, a row
//! jumped to.

use iced::{
    Task,
    widget::{operation, scrollable::AbsoluteOffset},
};

use crate::app::{App, Message, key};

use super::{JUMP_LIT, TIMELINE};

impl App {
    /// Heights the timeline's rows found at their last layout, which the
    /// layout already made room for (`Rows::measured`): the rows keep them.
    /// Taken in before each message, so the view built next places rows
    /// where they were drawn.
    pub fn take_found(&mut self) {
        let found = std::mem::take(&mut *self.timeline.found.borrow_mut());
        if found.is_empty() {
            return;
        }
        let (first, last) = self.timeline.in_view(crate::sync::MARGIN);
        let mut changed = false;
        for (row, height) in found {
            if let Some(index) =
                (first..last).find(|&index| key(&self.timeline.messages[index].item) == row)
                && (self.timeline.heights[index] - height).abs() > 0.5
            {
                self.timeline.heights[index] = height;
                changed = true;
            }
        }
        if changed {
            self.place_rows();
        }
    }

    /// The rows built in and below the view of someone reading earlier
    /// ones report the heights they took: the rows keep them, and the view
    /// stays on what's read ([`App::keep_reading`]).
    pub fn measured(&mut self, heights: Vec<(u64, f32)>) -> Task<Message> {
        let (first, last) = self.timeline.in_view(crate::sync::MARGIN);
        let reading = self.reading();
        let mut changed = false;
        for (row, height) in heights {
            let Some(index) =
                (first..last).find(|&index| key(&self.timeline.messages[index].item) == row)
            else {
                continue;
            };
            if (self.timeline.heights[index] - height).abs() <= 0.5 {
                continue;
            }
            self.timeline.heights[index] = height;
            changed = true;
        }
        if !changed {
            return Task::none();
        }
        self.place_rows();
        Task::batch([self.keep_reading(reading), self.report()])
    }

    /// While someone reads earlier messages, the row at the view's top and
    /// how far into it the view starts; at the end, none (the scrollable,
    /// anchored to the bottom, follows the end).
    pub fn reading(&self) -> Option<(String, f32)> {
        if self.timeline.at_end || self.timeline.messages.is_empty() {
            return None;
        }
        let top = self.timeline.view_top();
        let index = self.timeline.tops[1..]
            .partition_point(|end| *end <= top)
            .min(self.timeline.messages.len() - 1);
        Some((
            self.timeline.messages[index].item.clone(),
            top - self.timeline.tops[index],
        ))
    }

    /// Puts the row [`App::reading`] found back where it was in the view,
    /// whatever came, went or changed height above or below it, as the
    /// Slint client and Discord keep what's read in place. The scrollable
    /// is moved by the difference rather than to a place, so moves asked
    /// for before it reported where it is add up.
    pub fn keep_reading(&mut self, reading: Option<(String, f32)>) -> Task<Message> {
        let Some((item, into)) = reading else {
            return Task::none();
        };
        let Some(index) = self
            .timeline
            .messages
            .iter()
            .position(|message| message.item == item)
        else {
            return Task::none();
        };
        let top = self.timeline.tops[index] + into;
        let from_end = (self.timeline.total_height() - self.timeline.height - top).max(0.0);
        let by = from_end - self.timeline.from_end;
        if by.abs() <= 0.5 {
            return Task::none();
        }
        self.timeline.from_end = from_end;
        operation::scroll_by(TIMELINE, AbsoluteOffset { x: 0.0, y: by })
    }

    /// Goes to the newest message.
    pub fn go_to_end(&mut self) -> Task<Message> {
        self.timeline.from_end = 0.0;
        self.timeline.at_end = true;
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
    pub fn scroll_to_row(&mut self, index: usize, third: bool) -> Task<Message> {
        let lead = if third {
            self.timeline.height / 3.0
        } else {
            0.0
        };
        let top = self.timeline.tops[index] - lead;
        self.timeline.from_end =
            (self.timeline.total_height() - self.timeline.height - top).max(0.0);
        self.timeline.at_end = self.timeline.from_end <= 2.0;
        operation::scroll_to(
            TIMELINE,
            AbsoluteOffset {
                x: None,
                y: Some(self.timeline.from_end),
            },
        )
    }

    /// Where the core asked the timeline to go, once it has the message:
    /// a jump (tinted a moment) or the first unread message.
    pub fn follow_jumps(&mut self) -> Task<Message> {
        let state = self.client.state();
        let jump = state.jump.clone();
        let unread_jump = state.unread_jump;
        let mut tasks = Vec::new();
        if jump != self.timeline.jumped
            && let Some((item, _)) = &jump
            && let Some(index) = self
                .timeline
                .messages
                .iter()
                .position(|message| &message.item == item)
        {
            self.timeline.jumped = jump.clone();
            self.timeline.jump_lit = Some(item.clone());
            tasks.push(self.scroll_to_row(index, true));
            tasks.push(crate::after(JUMP_LIT, Message::JumpFaded(item.clone())));
        }
        if unread_jump != self.timeline.unread_jumped
            && let Some(index) = self.timeline.unread_index(self.client.state())
        {
            self.timeline.unread_jumped = unread_jump;
            tasks.push(self.scroll_to_row(index, false));
        }
        Task::batch(tasks)
    }
}
