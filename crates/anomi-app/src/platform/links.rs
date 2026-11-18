//! Links the system hands the program: a Discord link this start was given,
//! or a later start brought (`queue_link` and `open_pending_link` in
//! `src/ui/shell.rs` and `src/ui.rs`). One waits until Discord is online,
//! the window shows and no dialog covers it, then opens where Discord's
//! client would: a server, a channel, a message, a profile, or an invite's,
//! a gift's, an event's or an authorization's dialog
//! (`discord_link_action`, `src/ui/callbacks.rs:4223`). Registering as the
//! system's handler of `discord:` links, and links clicked in messages, are
//! task I15's (docs/iced/tasks/integration.md).

use iced::Task;

use crate::app::App;
use crate::message::Message;

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
        app.dispatch(action(route))
    }
    #[cfg(not(feature = "discord"))]
    Task::none()
}

/// What opening a Discord route does. A person's profile opens whole, as
/// nothing was clicked for its card to stand beside.
#[cfg(feature = "discord")]
fn action(route: yapfast_core::discord::DiscordRoute) -> yapfast_core::application::Action {
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
        DiscordRoute::User { user } => Action::ShowProfile { user, large: true },
    }
}

#[cfg(all(test, feature = "discord"))]
mod tests {
    use yapfast_core::application::Action;

    #[test]
    fn a_message_link_jumps_to_its_message_in_its_channel() {
        let route = yapfast_core::discord::discord_route("https://discord.com/channels/10/11/12")
            .expect("a message link");
        let Action::JumpInRoom { room, item } = super::action(route) else {
            panic!("a jump to the message");
        };
        assert_eq!(room, yapfast_core::discord::room_id("10", "11"));
        assert_eq!(item, "d:12");
    }
}
