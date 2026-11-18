//! The timeline's keys (`timeline-keys`, `ui/yapfast.slint:3654`): the
//! keyboard's message, what its keys may do to it, and what they do.

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::chat::{Delivery, Reacts};

use anomi_vm::chat::timeline::{Act, Intent, May};
use anomi_widgets::focus;

use crate::app::App;
use crate::message::Message;

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Browsing(browsing) => {
            let keys = &mut app.vm.chat.timeline.keys;
            keys.browsing = browsing;
            if !browsing {
                keys.confirming = false;
            }
            Task::none()
        }
        Intent::Step(key) => {
            step(app, key);
            Task::none()
        }
        // Escape while stepping: back to the composer (`focus-composer`).
        Intent::Leave => {
            app.vm.chat.timeline.keys.browsing = false;
            focus::into(focus::COMPOSER)
        }
        Intent::Act(key, act) => act_on(app, key, act),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The keyboard stepped to the message `key`: it's the keyboard's, and its
/// keys may do what the message allows.
fn step(app: &mut App, key: u64) {
    let shown = &app.shown.timeline;
    let state = &app.core.state;
    let may = match (shown.message(key), shown.look()) {
        (Some(message), Some(look)) if !message.deleted && message.delivery == Delivery::Sent => {
            let writes = look.writes;
            May {
                menu: true,
                react: message.can_react && look.reacts == Reacts::Any,
                reply: message.can_reply && writes,
                edit: message.own && message.can_edit && writes,
                delete: message.can_delete && writes,
                copy: !message.body.is_empty(),
                forward: message.can_forward && look.forwards,
                mark_unread: !state.preview,
            }
        }
        _ => May::default(),
    };
    let keys = &mut app.vm.chat.timeline.keys;
    keys.active = Some(key);
    keys.may = may;
    keys.confirming = false;
}

/// Alt+Up in the composer: the keyboard goes to the newest message, to
/// step through them from there (`browse-messages`, `ui/yapfast.slint:758`).
pub(super) fn browse(app: &mut App) -> Task<Message> {
    let shown = &app.shown.timeline;
    let newest = (app.vm.chat.timeline.rows.iter().rev())
        .map(|row| row.key)
        .find(|key| shown.message(*key).is_some());
    let Some(key) = newest else {
        return Task::none();
    };
    step(app, key);
    focus::to(focus::TIMELINE)
}

fn act_on(app: &mut App, key: u64, act: Act) -> Task<Message> {
    let Some(item) = app
        .shown
        .holding(key)
        .message(key)
        .map(|message| message.item.clone())
    else {
        return Task::none();
    };
    let action = match act {
        Act::Reply => Action::Reply(item),
        Act::Edit => Action::Edit(item),
        Act::Delete => {
            app.vm.chat.timeline.keys.confirming = true;
            Action::RequestDelete(item)
        }
        Act::Forward => Action::StartForward(item),
        Act::MarkUnread => Action::MarkUnread(item),
        Act::ConfirmDelete => {
            app.vm.chat.timeline.keys.confirming = false;
            Action::ConfirmDelete
        }
        Act::CancelDelete => {
            app.vm.chat.timeline.keys.confirming = false;
            Action::CancelDelete
        }
        // Its menu, beside it.
        Act::Menu(spot) => return crate::intent::menus::message::open(app, key, spot, None),
        // The reaction picker, beside it.
        Act::React(spot) => return crate::intent::chat::pickers::for_message(app, key, spot),
        // Its text, as the menu's Copy Text copies it.
        Act::Copy => {
            let body = app
                .shown
                .holding(key)
                .message(key)
                .map(|message| message.body.clone());
            return body.map_or_else(Task::none, |body| iced::clipboard::write(body).discard());
        }
    };
    let task = app.dispatch(action);
    // A reply is written in the composer; an edit in its own field, which
    // takes the keys as it shows (`project::chat::edit`).
    match act {
        Act::Reply => Task::batch([task, focus::into(focus::COMPOSER)]),
        _ => task,
    }
}
