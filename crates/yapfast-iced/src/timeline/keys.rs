//! The keyboard's way through the messages (docs/chat-ui.md): Alt+Up in the
//! composer moves into the messages at the newest, lit with its hover bar
//! as the pointer would; the arrows, Page Up and Down, Home and End step;
//! Enter (or the Menu key, or Shift+F10) opens its menu, + reacts, R
//! replies, E edits one's own, Delete deletes, C copies its text, F
//! forwards, Alt+Enter marks a Discord message unread, and Escape goes back
//! to the composer. While it steps the window's focus is on nobody, so the
//! keys come here.

use iced::{
    Task,
    keyboard::{self, Key, key::Named},
    widget::operation,
};
use yapfast_core::{application::Action, chat::Delivery};

use crate::{
    app::{App, Message, NOBODY},
    menus::MenuAct,
};

/// Alt+Up in the composer: the newest message is the one stepped on.
pub fn step_into_messages(app: &mut App) -> Task<Message> {
    let Some(last) = app.timeline.messages.len().checked_sub(1) else {
        return Task::none();
    };
    Task::batch([operation::focus(NOBODY), step_to(app, last)])
}

/// The message stepped on, by its place, while there is one.
fn keyed_index(app: &App) -> Option<usize> {
    let item = app.keyed.as_ref()?;
    app.timeline
        .messages
        .iter()
        .position(|message| message.item == *item)
}

fn step_to(app: &mut App, index: usize) -> Task<Message> {
    let index = index.min(app.timeline.messages.len().saturating_sub(1));
    let Some(message) = app.timeline.messages.get(index) else {
        return Task::none();
    };
    app.keyed = Some(message.item.clone());
    app.timeline.keyed_press = app.pressed.get();
    app.pointed.message = Some(crate::timeline::row_item(app, index));
    crate::edit::reveal_row(app, index)
}

/// Stops stepping, as Escape or the composer taking the keys does.
pub fn leave_messages(app: &mut App) {
    if app.keyed.take().is_some() {
        app.pointed.message = None;
    }
}

/// A press anywhere but on the messages takes the keys from them: what was
/// pressed has them now, and the edge would mark a message they've left.
pub fn follow_press(app: &mut App) {
    if app.keyed.is_none() {
        return;
    }
    let at = app.pressed.get();
    if at != app.timeline.keyed_press && !app.timeline.place.get().contains(at) {
        leave_messages(app);
    }
}

/// A key while stepping through the messages; none for one it leaves.
pub fn browse_key(
    app: &mut App,
    key: &Key,
    modifiers: keyboard::Modifiers,
) -> Option<Task<Message>> {
    let index = keyed_index(app)?;
    let named = match key {
        Key::Named(named) => Some(*named),
        _ => None,
    };
    let last = app.timeline.messages.len().saturating_sub(1);
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
        return Some(step_to(app, to));
    }
    if named == Some(Named::Escape) {
        leave_messages(app);
        return Some(crate::composer::refocus(app));
    }
    let message = app.timeline.messages.get(index)?.clone();
    if message.deleted || message.delivery != Delivery::default() {
        return None;
    }
    let item = message.item.clone();
    let at = crate::message::hover_bar::bar_menu_at(app.ctx(), index);
    let writes = crate::composer::composer_look(app.ctx()).enabled;
    let letter = match key.as_ref() {
        Key::Character(text) => text.to_lowercase(),
        _ => String::new(),
    };
    let act =
        |app: &mut App, act: MenuAct| Some(crate::menus::act(app, act, item.clone(), at, None));
    match (named, letter.as_str()) {
        (Some(Named::Enter), _) if modifiers.alt() => {
            if app.client.app.marks_unread(&item) {
                app.client.dispatch(Action::MarkUnread(item.clone()));
            }
            Some(app.sync())
        }
        (Some(Named::Enter | Named::ContextMenu), _) => {
            Some(crate::menus::open_menu(app, index, at))
        }
        (Some(Named::F10), _) if modifiers.shift() => Some(crate::menus::open_menu(app, index, at)),
        (_, "+" | "=") if message.can_react && crate::menus::adds_reactions(app, index) => {
            act(app, MenuAct::AddReaction)
        }
        (_, "r") if message.can_reply && writes => {
            leave_messages(app);
            act(app, MenuAct::Reply)
        }
        (_, "e") if message.own && message.can_edit && writes => {
            leave_messages(app);
            act(app, MenuAct::Edit)
        }
        (Some(Named::Delete | Named::Backspace), _) if message.can_delete && writes => {
            act(app, MenuAct::Delete)
        }
        // Copying leaves the keys where they are.
        (_, "c") if !message.body.is_empty() => Some(iced::clipboard::write(message.body.clone())),
        // Ctrl+F is the search's.
        (_, "f")
            if message.can_forward
                && !modifiers.command()
                && crate::forward::allowed(app.client.state()) =>
        {
            act(app, MenuAct::Forward)
        }
        _ => None,
    }
}
