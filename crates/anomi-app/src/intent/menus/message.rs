//! The message menu: opened at the pointer on a message, its picture or
//! video, or by the hover bar's More beside it (`open-message-menu`,
//! `ui/yapfast.slint:2460`), and each of its rows carried out on the
//! message it opened on, named by its key, then closed (`message-menu`,
//! `:2730`).

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat::attachments::Intent as Attachments;
use anomi_vm::chat::menu::{Intent, Media};
use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::text::Link;
use anomi_widgets::focus;

use crate::app::App;
use crate::message::Message;
use crate::project::menus::message::place;

/// Opens the menu of the message `key` at `spot`, about its attachment
/// `media` when a picture or video was right-clicked. Nothing for a
/// message deleted or gone.
pub(crate) fn open(app: &mut App, key: u64, spot: Spot, media: Option<Media>) -> Task<Message> {
    let link = (app.shown.menus.link.take())
        .filter(|(on, _)| *on == key && media.is_none())
        .map(|(_, link)| link);
    let timeline = &app.shown.timeline;
    let (Some(message), Some(look)) = (timeline.message(key), timeline.look()) else {
        return Task::none();
    };
    if message.deleted {
        return Task::none();
    }
    let (mut menu, due) =
        crate::project::menus::message::build(&app.core, look, message, key, media);
    app.shown.menus.message_masked = matches!(link, Some(Link::Leave(_)));
    if let Some(address) = link.as_ref().and_then(web_address) {
        menu.facts.address = address.to_owned();
        anomi_chat::menu::build(&mut menu);
    }
    let (at, flip) = place(spot, &menu, app.size);
    menu.flip = flip;
    app.vm.chat.menu = menu;
    let shown = &mut app.shown.menus;
    shown.message_apps_due = due;
    shown.message_spot = Some((spot, app.size));
    app.vm.shell.open_at(Layer::MessageMenu, at);
    // Its Apps follow the permissions as they are now.
    app.dispatch(Action::AskAppCommands)
}

/// The web address a link in a message's text leads to: a masked link's,
/// behind the question asked before leaving; none for a mention or one of
/// Yapfast's own links (`web_address`, `src/ui/callbacks.rs:4215`).
fn web_address(link: &Link) -> Option<&str> {
    match link {
        Link::Url(address) | Link::Leave(address) => {
            let web = yapfast_core::links::openable(address)
                && yapfast_core::mention::target(address).is_none();
            web.then_some(address.as_str())
        }
        _ => None,
    }
}

/// A row of the menu: the menu closes, and what it asks is done to its
/// message.
pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    if let Intent::Apps(open) = intent {
        app.vm.chat.menu.apps_open = open;
        return Task::none();
    }
    let key = app.vm.chat.menu.key;
    let spot = app
        .vm
        .shell
        .opened(Layer::MessageMenu)
        .and_then(|opened| opened.at)
        .unwrap_or_default();
    app.vm.shell.close(Layer::MessageMenu);
    let Some(item) = app
        .shown
        .timeline
        .message(key)
        .map(|message| message.item.clone())
    else {
        return Task::none();
    };
    let chat =
        |app: &mut App, intent: anomi_vm::chat::Intent| crate::intent::chat::handle(app, intent);
    let action = match intent {
        Intent::React(emoji) => Action::React { item, key: emoji },
        // The reaction picker, where the menu was.
        Intent::AddReaction => return chat(app, MessageIntent::Picker(key, spot).into()),
        Intent::ViewReactions => Action::ShowReactions { item, key: None },
        // The field takes the keys as the edit starts (`project::chat::edit`).
        Intent::Edit => Action::Edit(item),
        Intent::Reply => {
            let task = app.dispatch(Action::Reply(item));
            return Task::batch([task, focus::to(focus::COMPOSER)]);
        }
        Intent::Forward => Action::StartForward(item),
        Intent::StartThread => return chat(app, MessageIntent::NewThread(key).into()),
        Intent::ViewThread(room) => Action::OpenAside(room),
        Intent::Copy(text) => return iced::clipboard::write(text).discard(),
        Intent::EndPoll => Action::AskEndPoll(item),
        Intent::Pin => Action::PinMessage(item),
        Intent::MarkUnread => Action::MarkUnread(item),
        Intent::CopyImage(index) => Action::CopyAttachment { item, index },
        Intent::Save(index) => return chat(app, Attachments::Save { item: key, index }.into()),
        // Where a click on the link would go; a masked one asks first.
        Intent::OpenLink(link) => {
            let masked = std::mem::take(&mut app.shown.menus.message_masked);
            let link = if masked {
                Link::Leave(link)
            } else {
                Link::Url(link)
            };
            return chat(app, MessageIntent::Link(key, link).into());
        }
        // Shift and a click deletes at once, as Discord's menu does; a plain
        // click asks first.
        Intent::Delete if app.shift => Action::DeleteNow(item),
        Intent::Delete => Action::RequestDelete(item),
        Intent::RunApp(command) => Action::RunAppCommand {
            command,
            target: Some(item),
        },
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            return Task::none();
        }
    };
    app.dispatch(action)
}

/// The menu played its way out: its rows go.
pub(crate) fn gone(app: &mut App) {
    if !app.vm.shell.is_open(Layer::MessageMenu) {
        app.vm.chat.menu = Default::default();
        app.shown.menus.message_apps_due = false;
        app.shown.menus.message_spot = None;
        app.shown.menus.message_masked = false;
    }
}
