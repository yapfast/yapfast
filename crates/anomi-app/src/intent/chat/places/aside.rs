//! The thread beside the channel (`ThreadPanel`'s `close`, `expand`,
//! `more` and the `aside-*` callbacks, `src/ui/callbacks.rs:1123`-`:1137`):
//! its header's buttons, and what its timeline and composer ask, which ride
//! in `Intent::Within`. Its timeline is its own: the rows in view are told
//! the core as the thread's (`VisibleMessages` with its room), its earlier
//! messages load at its top (`AsideEarlier`), and the message the pointer
//! is on is its own, apart from the channel's. What its rows ask otherwise
//! (a reaction, a menu, a link, a picture) goes where the channel's does:
//! each names its message by key, which the thread's rows hold
//! (`Shown::holding`). Its composer is the channel's over the thread's
//! draft (`composer::aside`).

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::chat::suggestions::Intent as Offered;
use anomi_vm::chat::thread_panel::Intent;
use anomi_vm::chat::timeline::{self, Intent as TimelineIntent};
use anomi_vm::chat::{self as chat};
use anomi_vm::shell::Layer;
use anomi_widgets::focus;

use crate::app::App;
use crate::intent::chat::composer;
use crate::message::Message;
use crate::project::chat::composer::Field;
use crate::project::menus::subs::SIDEBAR;

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let room = app.shown.aside.room().map(str::to_owned);
    match intent {
        Intent::Close => app.dispatch(Action::CloseAside),
        Intent::Expand => app.dispatch(Action::ExpandAside),
        // The thread's menu under its "...": Leave thread, Copy link (I10).
        Intent::More(spot) => match room {
            Some(room) => crate::intent::menus::open(app, format!("{SIDEBAR}{room}"), false, spot),
            None => Task::none(),
        },
        Intent::Leave | Intent::CopyLink => {
            let Some(room) = room else {
                return Task::none();
            };
            let id = if intent == Intent::Leave {
                "leave-thread"
            } else {
                "copy-link"
            };
            crate::intent::menus::choose_on(app, format!("{SIDEBAR}{room}"), id)
        }
        Intent::Within(intent) => within(app, *intent),
        intent => {
            tracing::debug!(?intent, "not the thread beside's");
            Task::none()
        }
    }
}

/// What the thread's timeline or composer asked.
fn within(app: &mut App, intent: chat::Intent) -> Task<Message> {
    match intent {
        chat::Intent::Timeline(intent) => self::timeline(app, intent),
        chat::Intent::Message(MessageIntent::Point(key)) => {
            app.shown.aside.left_held = false;
            light(app, Some(key));
            Task::none()
        }
        // A reply is written in the thread's composer.
        chat::Intent::Message(MessageIntent::Reply(key)) => {
            let item = app
                .shown
                .aside
                .message(key)
                .map(|message| message.item.clone());
            match item {
                Some(item) => Task::batch([
                    app.dispatch(Action::Reply(item)),
                    focus::into(focus::ASIDE_COMPOSER),
                ]),
                None => Task::none(),
            }
        }
        chat::Intent::Composer(intent) => composer::aside(app, intent),
        chat::Intent::Suggestions(Offered::Choose(index)) => {
            composer::choose(app, Field::Aside, index)
        }
        intent => crate::intent::chat::handle(app, intent),
    }
}

fn timeline(app: &mut App, intent: TimelineIntent) -> Task<Message> {
    match intent {
        TimelineIntent::Shown(report) => shown(app, report),
        TimelineIntent::Earlier => app.dispatch(Action::AsideEarlier),
        TimelineIntent::Left => {
            left(app);
            Task::none()
        }
        // No unread bar, no window of older messages, no keys of its own.
        intent => {
            tracing::debug!(?intent, "not the thread beside's timeline's");
            Task::none()
        }
    }
}

/// The list told which of the thread's rows are in view: the core hears
/// them as the thread's, and their pictures are asked for.
fn shown(app: &mut App, report: timeline::Shown) -> Task<Message> {
    let Some(room) = app.shown.aside.room().map(str::to_owned) else {
        return Task::none();
    };
    let timeline = &mut app.vm.chat.thread_panel.timeline;
    timeline.shown = Some(report.span);
    let items: Vec<String> = (report.keys.iter())
        .filter_map(|&key| app.shown.aside.message_at(timeline.index_of(key)?))
        .map(|message| message.item.clone())
        .collect();
    let at_end = report.at_end;
    app.shown.aside.report = Some(report);
    let visible = app.dispatch(Action::VisibleMessages {
        room,
        items,
        at_end,
    });
    let timeline = &app.vm.chat.thread_panel.timeline;
    crate::project::chat::timeline::want(&app.core, timeline, &app.shown.aside, &mut app.pictures);
    crate::media::seen(app);
    visible
}

/// The pointer left the thread's timeline: its lit message goes dark,
/// unless a menu, the reaction picker or the question of deleting holds
/// its bar, which then goes as that closes ([`let_go`]).
fn left(app: &mut App) {
    let held = app.core.state.confirm_delete.is_some()
        || app.vm.shell.is_open(Layer::MessageMenu)
        || app.vm.shell.is_open(Layer::EmojiPicker);
    if held {
        app.shown.aside.left_held = true;
    } else {
        light(app, None);
    }
}

/// What held the thread's lit message's bar closed.
pub(crate) fn let_go(app: &mut App) {
    if std::mem::take(&mut app.shown.aside.left_held) {
        light(app, None);
    }
}

/// The message the pointer is on in the thread lies lit, with its bar.
fn light(app: &mut App, key: Option<u64>) {
    if app.shown.aside.lit == key {
        return;
    }
    app.shown.aside.lit = key;
    let timeline = &mut app.vm.chat.thread_panel.timeline;
    crate::project::chat::timeline::light(&app.core, timeline, &app.shown.aside);
}

/// After every update: the thread's earlier messages load as its view
/// nears its top.
pub(crate) fn after(app: &mut App) -> Task<Message> {
    if app.vm.chat.thread_panel.timeline.wants_earlier() {
        return app.dispatch(Action::AsideEarlier);
    }
    Task::none()
}
