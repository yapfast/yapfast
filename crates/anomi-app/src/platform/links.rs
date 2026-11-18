//! Links the system hands the program: a Discord link this start was given,
//! or a later start brought (`queue_link` and `open_pending_link` in
//! `src/ui/shell.rs` and `src/ui.rs`). One waits until Discord is online,
//! the window shows and no dialog covers it, then opens where Discord's
//! client would: a server, a channel, a message, a profile, or an invite's,
//! a gift's, an event's or an authorization's dialog
//! (`discord_link_action`, `src/ui/callbacks.rs:4223`).
//!
//! And links clicked in the window that lead out of its own places, in a
//! message, an embed, a topic or a badge ([`follow`], the end of
//! `open-link`, `src/ui/callbacks.rs:2153`-`:2177`): one of Discord's opens
//! here when the core can show it; a masked one, whose words show another
//! address than where it goes, asks first, naming the real address; a web
//! page opens in the browser. Only web pages open: a message can't start
//! other programs.

use iced::Task;
use yapfast_core::application::Effect;
use yapfast_core::links;

use anomi_vm::places::leaving_link::LeavingLink;
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;

/// Follows a link clicked in the window: `masked` when its words show
/// another address than `link`, where it goes (`Link::Leave`), as a link
/// the core marked so (`links::LEAVE`) is too.
pub fn follow(app: &mut App, link: &str, masked: bool) -> Task<Message> {
    let (link, masked) = match links::masked(link) {
        Some(target) => (target, true),
        None => (link, masked),
    };
    #[cfg(feature = "discord")]
    if let Some(route) = yapfast_core::discord::discord_route(link)
        .filter(|route| app.core.state.opens_discord_route(route))
    {
        // An invite shows its server first and joins only when asked,
        // whatever the words say.
        return app.dispatch(action(route, true));
    }
    if !links::openable(link) {
        return Task::none();
    }
    if masked {
        ask(app, link);
        return Task::none();
    }
    open_page(app, link)
}

/// Asks before leaving for a masked link, naming the site it really goes
/// to; Visit opens it (`intent::places`).
fn ask(app: &mut App, link: &str) {
    app.vm.places.leaving_link = LeavingLink {
        link: link.to_owned(),
        site: links::host(link).to_owned(),
    };
    app.vm.shell.open(Layer::LeavingLink);
}

/// Opens a web page in the browser; anything else stays shut.
pub fn open_page(app: &mut App, link: &str) -> Task<Message> {
    if !links::openable(link) {
        return Task::none();
    }
    crate::effects::run(app, vec![Effect::OpenPage(link.to_owned())])
}

/// Keeps a link to open once it can; one this build can't open is let go.
pub fn queue(app: &mut App, link: String) {
    #[cfg(feature = "discord")]
    if yapfast_core::discord::discord_route(&link).is_some() {
        app.desktop.link = Some(link);
        return;
    }
    let _ = (app, link);
    tracing::warn!("Anomi was handed a link it can't open");
}

/// Opens the link waiting, once it can. Nothing while none waits.
pub fn open_waiting(app: &mut App) -> Task<Message> {
    if app.desktop.link.is_none() || app.window.is_none() {
        return Task::none();
    }
    #[cfg(feature = "discord")]
    {
        let state = &app.core.state;
        if !state.discord_online || state.dialog_open() {
            return Task::none();
        }
        let Some(link) = app.desktop.link.take() else {
            return Task::none();
        };
        let Some(route) = yapfast_core::discord::discord_route(&link)
            .filter(|route| app.core.state.opens_discord_route(route))
        else {
            tracing::info!("Anomi has no place for the link it was handed");
            return Task::none();
        };
        app.dispatch(action(route, false))
    }
    #[cfg(not(feature = "discord"))]
    Task::none()
}

/// What opening a Discord route does. A person's profile shows as a card
/// beside a link `clicked`, and opens whole for one handed over, as nothing
/// was clicked for its card to stand beside.
#[cfg(feature = "discord")]
fn action(
    route: yapfast_core::discord::DiscordRoute,
    clicked: bool,
) -> yapfast_core::application::Action {
    use yapfast_core::application::{Action, Navigation};
    use yapfast_core::discord::{self, DiscordRoute};
    match route {
        DiscordRoute::Invite { code } => Action::OpenInviteLink(code),
        DiscordRoute::Authorize { query } => Action::OpenOAuthLink(query),
        DiscordRoute::Gift { code } => Action::OpenGiftLink(code),
        DiscordRoute::Event { guild, event } => Action::OpenEventLink { guild, event },
        DiscordRoute::DirectMessages => Action::Navigate(Navigation::DirectMessages),
        DiscordRoute::Server { guild } => {
            Action::Navigate(Navigation::Server(discord::server_id(&guild)))
        }
        DiscordRoute::Channel { guild, channel } => {
            Action::Open(discord::room_id(&guild, &channel))
        }
        DiscordRoute::Message {
            guild,
            channel,
            message,
        } => Action::JumpInRoom {
            room: discord::room_id(&guild, &channel),
            item: format!("d:{message}"),
        },
        DiscordRoute::User { user } => Action::ShowProfile {
            user,
            large: !clicked,
        },
    }
}

#[cfg(all(test, feature = "discord"))]
mod tests {
    use yapfast_core::application::Action;

    #[test]
    fn a_message_link_jumps_to_its_message_in_its_channel() {
        let route = yapfast_core::discord::discord_route("https://discord.com/channels/10/11/12")
            .expect("a message link");
        let Action::JumpInRoom { room, item } = super::action(route, true) else {
            panic!("a jump to the message");
        };
        assert_eq!(room, yapfast_core::discord::room_id("10", "11"));
        assert_eq!(item, "d:12");
    }
}
