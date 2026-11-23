//! Where a link goes, wherever in the window it was clicked: a person's
//! card, a message jumped to, the pins, a Discord link opened in place,
//! or the browser.

use std::sync::Arc;

use iced::{Task, widget::operation};
use yapfast_core::application::Action;

use crate::app::{App, Message, Messages};

/// What a link in the timeline's text leads to.
pub const AUTHOR_LINK: &str = "yapfast:author/";
pub const JUMP_LINK: &str = "yapfast:jump/";
pub const PINS_LINK: &str = "yapfast:pins";

/// A link in the timeline's text was clicked: a spoiler shows, a
/// mention or author opens their card, a jump goes to its message, a
/// web address opens in the browser (a masked one once asked).
pub fn follow_link(app: &mut App, link: &str, at: iced::Point) -> Task<Message> {
    if let Some((item, index)) = yapfast_core::spoiler::target(link) {
        app.timeline.revealed.insert(format!("{item}/{index}"));
        let found = |messages: &Messages| messages.iter().position(|message| message.item == item);
        if let Some(row) = found(&app.timeline.messages)
            && let Some(body) = crate::timeline::body_for(app, row)
        {
            app.timeline.looks[row].body = Arc::new(body);
        } else if let Some(index) = app.aside.as_ref().and_then(|aside| found(&aside.messages))
            && let Some(body) = crate::timeline::body_for(app, crate::timeline::ASIDE + index)
            && let Some(aside) = app.aside.as_mut()
        {
            aside.looks[index].body = Arc::new(body);
        }
        return Task::none();
    }
    // An emoji opens its card under it.
    if let Some(target) = yapfast_core::emoji::card::target(link) {
        return app.show_expression(crate::info_cards::Expression::Emoji(target));
    }
    if let Some(item) = link.strip_prefix(JUMP_LINK) {
        app.client.dispatch(Action::Jump(item.to_owned()));
        return app.sync();
    }
    if link == PINS_LINK {
        app.client.dispatch(Action::ShowPins(true));
        return app.sync();
    }
    let user = link
        .strip_prefix(AUTHOR_LINK)
        .map(str::to_owned)
        .or_else(|| match yapfast_core::mention::target(link)? {
            yapfast_core::mention::Target::User(user) => Some(user),
            _ => None,
        });
    if let Some(user) = user {
        return app.open_card(user, iced::Point::new(at.x + 8.0, at.y));
    }
    if let Some(yapfast_core::mention::Target::Role(role)) = yapfast_core::mention::target(link) {
        return app.show_role(role, at);
    }
    if let Some(yapfast_core::mention::Target::Channel(room)) = yapfast_core::mention::target(link)
    {
        // Its server or thread opens, and a voice channel isn't joined.
        app.client.dispatch(Action::Open(room));
        return app.sync();
    }
    // Discord's own links open their place here, when this client can show
    // it, and in the browser otherwise: an invite shows its server first
    // and joins only when asked.
    if let Some(action) = discord_link(app, link, at) {
        return match action {
            Ok(action) => {
                app.client.dispatch(action);
                app.sync()
            }
            Err(task) => task,
        };
    }
    // A masked link, whose words aren't where it goes, asks first.
    if let Some(target) = yapfast_core::links::masked(link) {
        #[cfg(feature = "plugins")]
        ask_plugins(app, target);
        app.leaving = Some(target.to_owned());
        app.leaving_from = None;
        return operation::focus(crate::app::NOBODY);
    }
    if yapfast_core::links::openable(link) {
        // A plugin that offers to open links on this host makes the leaving
        // dialog show, with its offer beside Visit site, so the person
        // chooses between the app and the browser.
        #[cfg(feature = "plugins")]
        if ask_plugins(app, link) {
            app.leaving = Some(link.to_owned());
            app.leaving_from = None;
            return operation::focus(crate::app::NOBODY);
        }
        crate::browser::open(link);
    }
    Task::none()
}

/// What one of Discord's own links does here: an action for the core,
/// or a profile's card at the words clicked; none for a link the core
/// can't open now, which opens in the browser.
pub(crate) fn discord_link(
    app: &mut App,
    link: &str,
    at: iced::Point,
) -> Option<Result<Action, Task<Message>>> {
    use yapfast_core::discord::{self, DiscordRoute};
    let route = discord::discord_route(yapfast_core::links::masked(link).unwrap_or(link))
        .filter(|route| app.client.state().opens_discord_route(route))?;
    Some(Ok(match route {
        DiscordRoute::Invite { code } => Action::OpenInviteLink(code),
        DiscordRoute::DirectMessages => {
            Action::Navigate(yapfast_core::application::Navigation::DirectMessages)
        }
        DiscordRoute::Server { guild } => Action::Navigate(
            yapfast_core::application::Navigation::Server(discord::server_id(&guild)),
        ),
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
        DiscordRoute::User { user } => {
            return Some(Err(app.open_card(user, iced::Point::new(at.x + 8.0, at.y))));
        }
        DiscordRoute::Authorize { query } => Action::OpenOAuthLink(query),
        DiscordRoute::Gift { code } => Action::OpenGiftLink(code),
        #[cfg(feature = "plugins")]
        DiscordRoute::PluginSignIn { plugin, query } => {
            Action::Plugin(yapfast_core::application::plugins::Action::SignInLink { plugin, query })
        }
        #[cfg(not(feature = "plugins"))]
        DiscordRoute::PluginSignIn { .. } => return None,
        DiscordRoute::Event { guild, event } => Action::OpenEventLink { guild, event },
    }))
}

/// Asks the plugins that offer to open links on `link`'s host; whether any
/// was asked.
#[cfg(feature = "plugins")]
fn ask_plugins(app: &mut App, link: &str) -> bool {
    app.client.dispatch(Action::Plugin(
        yapfast_core::application::plugins::Action::AskLink(link.to_owned()),
    ));
    app.client.state().plugin_link.is_some()
}
