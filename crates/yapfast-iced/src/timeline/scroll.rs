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
    /// Where the "New" divider is, when its message is loaded.
    pub fn unread_index(&self) -> Option<usize> {
        let item = self.client.state().first_unread.as_deref()?;
        self.messages
            .iter()
            .position(|message| message.item == item)
    }

    /// Heights the timeline's rows found at their last layout, which the
    /// layout already made room for (`Rows::measured`): the rows keep them.
    /// Taken in before each message, so the view built next places rows
    /// where they were drawn.
    pub fn take_found(&mut self) {
        let found = std::mem::take(&mut *self.found.borrow_mut());
        if found.is_empty() {
            return;
        }
        let (first, last) = self.in_view(crate::sync::MARGIN);
        let mut changed = false;
        for (row, height) in found {
            if let Some(index) = (first..last).find(|&index| key(&self.messages[index].item) == row)
                && (self.heights[index] - height).abs() > 0.5
            {
                self.heights[index] = height;
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
        let (first, last) = self.in_view(crate::sync::MARGIN);
        let reading = self.reading();
        let mut changed = false;
        for (row, height) in heights {
            let Some(index) = (first..last).find(|&index| key(&self.messages[index].item) == row)
            else {
                continue;
            };
            if (self.heights[index] - height).abs() <= 0.5 {
                continue;
            }
            self.heights[index] = height;
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
        if self.at_end || self.messages.is_empty() {
            return None;
        }
        let top = self.view_top();
        let index = self.tops[1..]
            .partition_point(|end| *end <= top)
            .min(self.messages.len() - 1);
        Some((self.messages[index].item.clone(), top - self.tops[index]))
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
            .messages
            .iter()
            .position(|message| message.item == item)
        else {
            return Task::none();
        };
        let top = self.tops[index] + into;
        let from_end = (self.total_height() - self.timeline_height - top).max(0.0);
        let by = from_end - self.from_end;
        if by.abs() <= 0.5 {
            return Task::none();
        }
        self.from_end = from_end;
        operation::scroll_by(TIMELINE, AbsoluteOffset { x: 0.0, y: by })
    }

    /// Where the view's top is among the rows.
    pub fn view_top(&self) -> f32 {
        (self.total_height() - self.timeline_height - self.from_end).max(0.0)
    }

    /// Goes to the newest message.
    pub fn go_to_end(&mut self) -> Task<Message> {
        self.from_end = 0.0;
        self.at_end = true;
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
            self.timeline_height / 3.0
        } else {
            0.0
        };
        let top = self.tops[index] - lead;
        self.from_end = (self.total_height() - self.timeline_height - top).max(0.0);
        self.at_end = self.from_end <= 2.0;
        operation::scroll_to(
            TIMELINE,
            AbsoluteOffset {
                x: None,
                y: Some(self.from_end),
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
        if jump != self.jumped
            && let Some((item, _)) = &jump
            && let Some(index) = self
                .messages
                .iter()
                .position(|message| &message.item == item)
        {
            self.jumped = jump.clone();
            self.jump_lit = Some(item.clone());
            tasks.push(self.scroll_to_row(index, true));
            tasks.push(crate::after(JUMP_LIT, Message::JumpFaded(item.clone())));
        }
        if unread_jump != self.unread_jumped
            && let Some(index) = self.unread_index()
        {
            self.unread_jumped = unread_jump;
            tasks.push(self.scroll_to_row(index, false));
        }
        Task::batch(tasks)
    }
}
