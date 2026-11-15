//! The keyboard's way through the messages, as the Slint client's
//! `timeline-keys` (docs/chat-ui.md): Alt+Up in the composer moves into the
//! messages at the newest, lit with its hover bar as the pointer would; the
//! arrows, Page Up and Down, Home and End step; Enter (or the Menu key, or
//! Shift+F10) opens its menu, + reacts, R replies, E edits one's own,
//! Delete deletes, C copies its text, F forwards, Alt+Enter marks a Discord
//! message unread, and Escape goes back to the composer. While it steps the
//! window's focus is on nobody, so the keys come here.

use iced::{
    Point, Task,
    keyboard::{self, Key, key::Named},
    widget::operation,
};
use yapfast_core::{application::Action, chat::Delivery};

use crate::{
    app::{App, Message, NOBODY},
    header,
    menus::MenuAct,
    rail, sidebar,
};

impl App {
    /// Alt+Up in the composer: the newest message is the one stepped on.
    pub fn step_into_messages(&mut self) -> Task<Message> {
        let Some(last) = self.messages.len().checked_sub(1) else {
            return Task::none();
        };
        Task::batch([operation::focus(NOBODY), self.step_to(last)])
    }

    /// The message stepped on, by its place, while there is one.
    fn keyed_index(&self) -> Option<usize> {
        let item = self.keyed.as_ref()?;
        self.messages
            .iter()
            .position(|message| message.item == *item)
    }

    fn step_to(&mut self, index: usize) -> Task<Message> {
        let index = index.min(self.messages.len().saturating_sub(1));
        let Some(message) = self.messages.get(index) else {
            return Task::none();
        };
        self.keyed = Some(message.item.clone());
        self.pointed.message = Some(message.item.clone());
        self.reveal_row(index)
    }

    /// Where a keyed message's menu or picker opens: under its hover bar's
    /// right end, as a click on More would.
    fn keyed_at(&self, index: usize) -> Point {
        Point::new(
            rail::WIDTH + sidebar::WIDTH + self.width - 16.0 - 200.0,
            header::HEIGHT + 1.0 + self.tops[index] - self.view_top() + 24.0,
        )
    }

    /// Stops stepping, as Escape or the composer taking the keys does.
    pub fn leave_messages(&mut self) {
        if self.keyed.take().is_some() {
            self.pointed.message = None;
        }
    }

    /// A key while stepping through the messages; none for one it leaves.
    pub fn browse_key(
        &mut self,
        key: &Key,
        modifiers: keyboard::Modifiers,
    ) -> Option<Task<Message>> {
        let index = self.keyed_index()?;
        let named = match key {
            Key::Named(named) => Some(*named),
            _ => None,
        };
        let last = self.messages.len().saturating_sub(1);
        let step = match named {
            Some(Named::ArrowUp) => Some(index.saturating_sub(1)),
            Some(Named::ArrowDown) => Some(index + 1),
            Some(Named::PageUp) => Some(index.saturating_sub(5)),
            Some(Named::PageDown) => Some(index + 5),
            Some(Named::Home) => Some(0),
            Some(Named::End) => Some(last),
            _ => None,
        };
        if let Some(to) = step {
            return Some(self.step_to(to));
        }
        if named == Some(Named::Escape) {
            self.leave_messages();
            return Some(self.refocus());
        }
        let message = self.messages.get(index)?.clone();
        if message.deleted || message.delivery != Delivery::default() {
            return None;
        }
        let item = message.item.clone();
        let at = self.keyed_at(index);
        let writes = self.composer_look().enabled;
        let letter = match key.as_ref() {
            Key::Character(text) => text.to_lowercase(),
            _ => String::new(),
        };
        let act = |app: &mut Self, act: MenuAct| Some(app.act(act, item.clone(), at, None));
        match (named, letter.as_str()) {
            (Some(Named::Enter), _) if modifiers.alt() => {
                if self.client.app.marks_unread(&item) {
                    self.client.dispatch(Action::MarkUnread(item.clone()));
                }
                Some(self.sync())
            }
            (Some(Named::Enter | Named::ContextMenu), _) => Some(self.open_menu(index, at)),
            (Some(Named::F10), _) if modifiers.shift() => Some(self.open_menu(index, at)),
            (_, "+" | "=") if message.can_react => act(self, MenuAct::AddReaction),
            (_, "r") if message.can_reply && writes => {
                self.leave_messages();
                act(self, MenuAct::Reply)
            }
            (_, "e") if message.own && message.can_edit && writes => {
                self.leave_messages();
                act(self, MenuAct::Edit)
            }
            (Some(Named::Delete | Named::Backspace), _) if message.can_delete && writes => {
                act(self, MenuAct::Delete)
            }
            // Copying leaves the keys where they are.
            (_, "c") if !message.body.is_empty() => {
                Some(iced::clipboard::write(message.body.clone()))
            }
            (_, "f") if message.can_forward && self.forward_allowed() => {
                act(self, MenuAct::Forward)
            }
            _ => None,
        }
    }
}
