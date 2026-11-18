//! The cards about places: a server tag's (beside the tag, on a message, a
//! card or a profile), a mutual server's (beside its row on the full
//! profile) and a role's members, whose rows open their cards.

use iced::{Rectangle, Task};
use yapfast_core::application::{Action, Navigation};
use yapfast_lang::trn;

use anomi_vm::kind::Tag;
use anomi_vm::menu::Spot;
use anomi_vm::people::cards::{Intent, Server};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;
use crate::project::people::ShownTag;
use crate::project::rows;

/// A server tag's card beside it: the core asks Discord for the server's
/// profile, which the card shows as it comes.
pub(crate) fn tag_card(app: &mut App, tag: Tag, spot: Spot) -> Task<Message> {
    if tag.server.is_empty() {
        return Task::none();
    }
    let server = tag.server.clone();
    app.vm.people.cards.server = None;
    app.shown.people.tag = Some(ShownTag {
        server: tag.server,
        text: tag.text,
        badge: tag.badge,
    });
    app.vm.shell.open_at(Layer::Card, spot);
    app.dispatch(Action::OpenServerTag(server))
}

/// A mutual server's card, beside its row on the full profile.
pub(super) fn server_card(app: &mut App, id: &str, bounds: Rectangle) -> Task<Message> {
    let state = &app.core.state;
    let Some(view) = &state.profile else {
        return Task::none();
    };
    let Some(server) = view.profile.servers.iter().find(|server| server.id == id) else {
        return Task::none();
    };
    let card = Server {
        id: server.id.clone(),
        name: server.name.clone(),
        initial: rows::initial(&server.name),
        icon: rows::expected(state, server.icon.as_deref()),
        banner: rows::expected(state, server.banner.as_deref()),
        members: server
            .members
            .map(|count| trn!("{n} member", "{n} members", count))
            .unwrap_or_default(),
        channels: trn!("{n} channel", "{n} channels", server.channels),
        nick: server.nick.clone().unwrap_or_default(),
    };
    let wanted = [server.icon.as_deref(), server.banner.as_deref()];
    app.pictures.want(state, wanted.into_iter().flatten());
    app.shown.people.tag = None;
    app.vm.people.cards.tag = None;
    app.vm.people.cards.server = Some(card);
    app.vm.shell.open_at(Layer::Card, Spot::beside(bounds));
    Task::none()
}

/// The cards' intents.
pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        // The server a card shows, from its name: the person goes there,
        // and what was over the window closes.
        Intent::OpenServer(server) => {
            app.vm.shell.close(Layer::Card);
            let task = super::close(app, Layer::FullProfile);
            app.vm.shell.close(Layer::Profile);
            Task::batch([
                task,
                app.dispatch(Action::Navigate(Navigation::Server(server))),
            ])
        }
        Intent::Retry(server) => app.dispatch(Action::OpenServerTag(server)),
        Intent::OpenMember(user, spot) => super::open_card(app, user, spot),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
