//! The composer's intents, with its suggestions' and slash commands':
//! `src/ui/callbacks.rs:940` (send), `:2537`-`:2570` (files),
//! `:2798`-`:2908` (the words being typed, choosing what they're offered,
//! an emoji from the picker, the draft) and `src/ui/app_commands.rs`.
//!
//! A keystroke hands the core the draft (`SetDraft`) and follows the words
//! at the caret: both reach the composer alone (`Reach::COMPOSER`), so
//! typing never builds the timeline again. Every report carries whose
//! draft it was, and one about a room no longer open is dropped, so one
//! room's text never lands in another's draft.

mod aside;
mod commands;
mod files;
mod words;

pub(crate) use aside::handle as aside;
pub(crate) use commands::use_command;
pub(crate) use files::{Done, hovered, previews};
pub(crate) use words::{choose, follow, put_caret, suggest, typed};

use iced::Task;
use yapfast_core::application::{Action, ThreadFrom};

use anomi_vm::chat;
use anomi_vm::chat::composer::{Intent, PLUS_MENU};
use anomi_vm::chat::suggestions::Intent as Offered;
use anomi_vm::icon::Icon;
use anomi_vm::menu::{Menu, Row, Spot};
use yapfast_lang::tr;

use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::project::chat::composer::Field;
use crate::project::chat::timeline::Ask;

/// `Composer`, `Suggestions` and `Commands`.
pub fn handle(app: &mut App, intent: chat::Intent) -> Task<Message> {
    match intent {
        chat::Intent::Composer(intent) => composer(app, intent),
        chat::Intent::Suggestions(Offered::Choose(index)) => choose(app, Field::Channel, index),
        chat::Intent::Commands(intent) => commands::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not the composer's");
            Task::none()
        }
    }
}

fn composer(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Edited { room, text, caret } => {
            if room != app.vm.chat.composer.room {
                return Task::none();
            }
            edited(app, text, caret)
        }
        Intent::Caret { room, caret } => {
            if room != app.vm.chat.composer.room {
                return Task::none();
            }
            follow(app, Field::Channel, caret)
        }
        Intent::Send => send(app),
        Intent::Focus(focused) => {
            app.vm.chat.composer.focused = focused;
            if focused {
                app.aside_last = false;
            }
            Task::none()
        }
        Intent::Paste => files::paste(app, false),
        Intent::EditLast => app.dispatch(Action::EditLast),
        Intent::BrowseMessages => super::timeline::browse(app),
        Intent::Attach => files::attach(app, false),
        Intent::More(spot) => {
            plus(app, spot);
            Task::none()
        }
        Intent::StartThread => app.dispatch(Action::NewThread {
            message: None,
            from: ThreadFrom::PlusButton,
        }),
        Intent::StartPoll => app.dispatch(Action::OpenPollDraft),
        Intent::Unstage(index) => app.dispatch(Action::Unstage(index)),
        Intent::ToggleSpoiler(index) => app.dispatch(Action::ToggleSpoiler(index)),
        Intent::CancelReply => app.dispatch(Action::CancelCompose),
        Intent::ToggleReplyPing => app.dispatch(Action::ToggleReplyPing { aside: false }),
        Intent::Accept => app.dispatch(Action::AnswerRequest(true)),
        Intent::Ignore => app.dispatch(Action::AnswerRequest(false)),
        Intent::Suggest(key) => suggest(app, Field::Channel, key),
        Intent::Key(key) => commands::key(app, key),
        Intent::Command(intent) => commands::handle(app, intent),
        // The pickers' buttons and Ctrl+G, Ctrl+E open the pickers over it.
        Intent::Expressions(which) => super::pickers::open::from_composer(app, which),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The text changed: the core takes it (unless the caret's report took it
/// already), and the words at the caret are followed; the composer shows
/// once, after both.
fn edited(app: &mut App, text: String, caret: usize) -> Task<Message> {
    let task = if app.core.state.draft() == text {
        Task::none()
    } else {
        app.act(Action::SetDraft(text))
    };
    Task::batch([task, typed(app, Field::Channel, caret)])
}

/// Enter, or Send: the list goes to the newest message, as Discord's does,
/// so the sent one shows; from a window of older messages, once the newest
/// replace it (`latest_due`).
fn send(app: &mut App) -> Task<Message> {
    if app.core.state.can_send() {
        if app.core.state.detached {
            app.shown.timeline.latest_due = true;
        } else {
            app.shown.timeline.asks.push(Ask::End);
        }
    }
    app.dispatch(Action::Send)
}

/// The plus where threads or polls can start: Discord's menu above it,
/// at least 224 px wide (`plus-menu`, `ui/yapfast.slint:2356`-`:2368`).
fn plus(app: &mut App, spot: Spot) {
    let more = app.vm.chat.composer.more;
    let mut rows = vec![
        Row::new(tr!("Upload a File"), Intent::Attach.into())
            .icon(Icon::File)
            .disabled(!more.attach),
    ];
    if more.threads {
        rows.push(Row::new(tr!("Create Thread"), Intent::StartThread.into()).icon(Icon::Thread));
    }
    if more.polls {
        rows.push(Row::new(tr!("Create Poll"), Intent::StartPoll.into()).icon(Icon::ChartBar));
    }
    let menu = Menu {
        narrowest: Some(PLUS_MENU),
        ..Menu::new(0, spot, rows)
    };
    super::super::menus::show(app, menu);
}

/// What the composer's async work brought back: a file chosen, the
/// clipboard read, a staged picture's preview, files dropped.
pub(crate) fn done(app: &mut App, done: Done) -> Task<Message> {
    files::done(app, done)
}

impl App {
    /// Hands the core an action from the composer and runs its effects,
    /// without showing anything yet: the caller shows the composer once
    /// for all it did (`App::show(Reach::COMPOSER)`).
    pub(crate) fn act(&mut self, action: Action) -> Task<Message> {
        let effects = self.core.dispatch(action);
        crate::effects::run(self, effects)
    }
}

/// Shows what a keystroke can have changed: the composer and the edit.
pub(crate) fn show(app: &mut App) {
    app.show(Reach::COMPOSER);
}
