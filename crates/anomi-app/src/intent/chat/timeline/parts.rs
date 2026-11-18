//! What a click on a message or its parts asks for: each names its message
//! by key, which the timeline turns into the core's item for its action,
//! so a click lands on the message it was made on however the rows moved
//! since. What another area shows (a person's card, a menu, a picker, the
//! viewer's video) is left to that area's task.

use std::time::Duration;

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::discord;

use anomi_vm::chat::attachments::Intent as AttachmentsIntent;
use anomi_vm::chat::embeds::Intent as EmbedsIntent;
use anomi_vm::chat::menu::Media;
use anomi_vm::chat::message::{Intent, Who};
use anomi_vm::chat::reactions::Intent as ReactionsIntent;
use anomi_vm::shell::Layer;
use anomi_vm::text::Link;
use anomi_widgets::focus;

use super::Tick;
use crate::app::{App, later};
use crate::intent::menus;
use crate::message::Message;
use crate::project::chat::message::{BLOCKED_LINK, blocked_key, fold_key};

/// How long the pointer stays on a reaction before who reacted is asked
/// for (`point_reaction`).
const HOVER: Duration = Duration::from_millis(500);

/// The core's item of the message `key` names.
fn item(app: &App, key: u64) -> Option<String> {
    app.shown
        .timeline
        .message(key)
        .map(|message| message.item.clone())
}

/// `action` made from the item `key` names; nothing once it's gone.
fn on(app: &mut App, key: u64, action: impl FnOnce(String) -> Action) -> Task<Message> {
    match item(app, key) {
        Some(item) => app.dispatch(action(item)),
        None => Task::none(),
    }
}

/// Reveals what `revealed` names on the message `key`, or hides it again,
/// and builds its row again.
fn reveal(app: &mut App, key: u64, revealed: String, shown: bool) -> Task<Message> {
    let timeline = &mut app.shown.timeline;
    let changed = if shown {
        timeline.revealed.insert(revealed)
    } else {
        timeline.revealed.remove(&revealed)
    };
    if changed {
        timeline.again.push(key);
        app.refresh();
    }
    Task::none()
}

pub(super) fn message(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Card(key, who, _) => {
            let sender = app
                .shown
                .timeline
                .message(key)
                .and_then(|message| match who {
                    Who::Author => Some(message.sender.clone()),
                    Who::Commander => message.used.as_ref().map(|used| used.sender.clone()),
                });
            match sender.as_deref().and_then(discord::sender_user) {
                Some(user) if !user.is_empty() => app.dispatch(Action::ShowProfile {
                    user: user.to_owned(),
                    large: false,
                }),
                _ => Task::none(),
            }
        }
        Intent::Quoted(key) => {
            let reply = app
                .shown
                .timeline
                .message(key)
                .and_then(|message| message.reply.as_ref()?.item.clone());
            match reply {
                Some(item) => app.dispatch(Action::Jump(item)),
                None => Task::none(),
            }
        }
        Intent::Link(key, link) => self::link(app, Some(key), link),
        Intent::Fold(key, open) => match item(app, key) {
            Some(item) => reveal(app, key, fold_key(&item), open),
            None => Task::none(),
        },
        Intent::Thread(key) => {
            let room = app
                .shown
                .timeline
                .message(key)
                .and_then(|message| Some(message.thread.as_ref()?.room.clone()));
            match room {
                Some(room) => app.dispatch(Action::OpenAside(room)),
                None => Task::none(),
            }
        }
        Intent::Retry(key) => on(app, key, Action::Retry),
        Intent::CancelSend(key) => on(app, key, Action::CancelSend),
        Intent::Dismiss(key) => on(app, key, Action::DismissEphemeral),
        Intent::React(key, emoji) => on(app, key, |item| Action::React { item, key: emoji }),
        Intent::Edit(key) => on(app, key, Action::Edit),
        Intent::Reply(key) => {
            Task::batch([on(app, key, Action::Reply), focus::into(focus::COMPOSER)])
        }
        Intent::Forward(key) => on(app, key, Action::StartForward),
        Intent::Pin(key) => on(app, key, Action::PinMessage),
        Intent::DeleteNow(key) => on(app, key, Action::DeleteNow),
        Intent::ConfirmDelete(_) => app.dispatch(Action::ConfirmDelete),
        Intent::CancelDelete(_) => app.dispatch(Action::CancelDelete),
        Intent::Menu(key, spot) => menus::message::open(app, key, spot, None),
        // The link under a right press, which the menu opening there next
        // offers to copy.
        Intent::LinkMenu(key, link) => {
            app.shown.menus.link = Some((key, link));
            Task::none()
        }
        // The author's menu is their person's, by user id (`author-menu`).
        Intent::AuthorMenu(key, spot) => {
            let user = app.shown.timeline.message(key).map(|message| {
                let sender = &message.sender;
                discord::sender_user(sender).unwrap_or(sender).to_owned()
            });
            match user.filter(|user| !user.is_empty()) {
                Some(user) => menus::open(app, user, true, spot),
                None => Task::none(),
            }
        }
        // The tag's card (I16),
        // a command written in the composer (I11), the reaction picker
        // (I12), a thread from it (I13).
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// A link in a message's text, or an embed's (no `key`): a spoiler shows,
/// a blocked message shows, a pin's notice jumps or shows the pins, a
/// mention opens its card, a channel its chat (`open-link`,
/// `src/ui/callbacks.rs:2065`).
fn link(app: &mut App, key: Option<u64>, link: Link) -> Task<Message> {
    let action = match link {
        Link::Spoiler(index) => {
            return match key.and_then(|key| Some((key, item(app, key)?))) {
                Some((key, item)) => reveal(app, key, format!("{item}/{index}"), true),
                None => Task::none(),
            };
        }
        Link::Url(url) if url.starts_with(BLOCKED_LINK) => {
            return match key.and_then(|key| Some((key, item(app, key)?))) {
                Some((key, item)) => reveal(app, key, blocked_key(&item), true),
                None => Task::none(),
            };
        }
        Link::Jump(item) => Action::Jump(item),
        Link::Pins => {
            let task = app.dispatch(Action::ShowPins(true));
            if app.core.state.pins.is_some() {
                app.vm.shell.open(Layer::Pins);
            }
            return task;
        }
        Link::Channel(room) => Action::Open(room),
        Link::User(user) | Link::Author(user) => Action::ShowProfile {
            user: discord::sender_user(&user).unwrap_or(&user).to_owned(),
            large: false,
        },
        Link::Role(role) => Action::ShowRole(role),
        Link::Everyone => return Task::none(),
        // A web address, a masked link (I15) and an emoji's card (I08).
        link => {
            tracing::debug!(?link, "not handled by the iced interface yet");
            return Task::none();
        }
    };
    app.dispatch(action)
}

pub(super) fn reactions(app: &mut App, intent: ReactionsIntent) -> Task<Message> {
    match intent {
        ReactionsIntent::Toggle(key, emoji) => {
            on(app, key, |item| Action::React { item, key: emoji })
        }
        // Who reacted is asked for once the pointer stays (`hover-reaction`).
        ReactionsIntent::Hover(key, emoji) => {
            let shown = &mut app.shown.timeline;
            shown.hovers += 1;
            shown.hover = Some((key, emoji));
            later(HOVER, Message::Timeline(Tick::Hovered(shown.hovers)))
        }
        ReactionsIntent::Left(key, emoji) => {
            let shown = &mut app.shown.timeline;
            if shown.hover.as_ref() != Some(&(key, emoji)) {
                return Task::none();
            }
            shown.hover = None;
            if app.core.state.reaction_tip().is_some() {
                return app.dispatch(Action::HoverReaction(None));
            }
            Task::none()
        }
        ReactionsIntent::Open(key, emoji) => on(app, key, |item| Action::ShowReactions {
            item,
            key: Some(emoji),
        }),
        ReactionsIntent::Select(key) => app.dispatch(Action::ShowReactors(key)),
        ReactionsIntent::Scrolled { near_end } => {
            let state = &app.core.state;
            let loading = state
                .reactions
                .as_ref()
                .is_some_and(|window| window.loading);
            if near_end && state.more_reactors() && !loading {
                return app.dispatch(Action::MoreReactors);
            }
            Task::none()
        }
        ReactionsIntent::Remove(user) => app.dispatch(Action::RemoveReactor(user)),
        ReactionsIntent::Close => app.dispatch(Action::CloseReactions),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The pointer stayed on a reaction: who reacted is asked for.
pub(super) fn hovered(app: &mut App, hover: u64) -> Task<Message> {
    let shown = &app.shown.timeline;
    if shown.hovers != hover {
        return Task::none();
    }
    let Some((key, emoji)) = shown.hover.clone() else {
        return Task::none();
    };
    on(app, key, |item| Action::HoverReaction(Some((item, emoji))))
}

pub(super) fn attachments(app: &mut App, intent: AttachmentsIntent) -> Task<Message> {
    match intent {
        AttachmentsIntent::Reveal { item: key, index } => match item(app, key) {
            Some(item) => reveal(app, key, format!("{item}/a{index}"), true),
            None => Task::none(),
        },
        AttachmentsIntent::Open { item: key, index } => {
            on(app, key, |item| Action::ViewImage { item, index })
        }
        AttachmentsIntent::Favorite { item: key, index } => {
            on(app, key, |item| Action::FavoriteMessageGif { item, index })
        }
        AttachmentsIntent::Video(intent) => {
            crate::media::handle(app, anomi_vm::chat::Intent::Video(intent))
        }
        AttachmentsIntent::Voice(intent) => {
            crate::media::handle(app, anomi_vm::chat::Intent::VoiceMessage(intent))
        }
        AttachmentsIntent::Menu {
            item: key,
            index,
            video,
            spot,
        } => menus::message::open(app, key, spot, Some(Media { index, video })),
        // Saving to a file (I15), a sticker's card (I08).
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

pub(super) fn embeds(app: &mut App, intent: EmbedsIntent) -> Task<Message> {
    match intent {
        EmbedsIntent::Link(link) => self::link(app, None, link),
        EmbedsIntent::View { item: key, index } => {
            on(app, key, |item| Action::ViewImage { item, index })
        }
        EmbedsIntent::Reveal { item: key, index } => match item(app, key) {
            Some(item) => reveal(app, key, format!("{item}/a{index}"), true),
            None => Task::none(),
        },
        EmbedsIntent::Video(intent) => {
            crate::media::handle(app, anomi_vm::chat::Intent::Video(intent))
        }
        EmbedsIntent::Menu {
            item: key,
            index,
            video,
            at,
        } => menus::message::open(app, key, at, Some(Media { index, video })),
        // A video's page in the browser (I15).
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
