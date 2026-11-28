//! The cards a click on an emoji in a message's text, or on a message's
//! sticker, opens, as Discord's client shows them: what it is, and, for one
//! of a server's, the server it is from with how many are online there. Both
//! clients draw these; what they say is worked out here once.
//!
//! An emoji in a text is a link ([`unicode_link`], [`custom_link`]) that
//! [`target`] reads back.

use crate::{
    application::{AppState, Application},
    chat::ChatServer,
};

/// The link an emoji's glyph opens, then the emoji's code points in lower-case
/// hexadecimal, such as `1f62d`, or `c/` and a custom emoji's id.
pub const LINK: &str = "yapfast:emoji/";

/// The link that opens the card of the emoji with these code points.
pub fn unicode_link(emoji: &str) -> String {
    let points: Vec<String> = emoji
        .chars()
        .map(|character| format!("{:x}", character as u32))
        .collect();
    format!("{LINK}{}", points.join("-"))
}

/// The link that opens the card of the custom emoji with this id.
pub fn custom_link(id: &str) -> String {
    format!("{LINK}c/{id}")
}

/// What a link in a text points at, for the card it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// An emoji of Unicode's own, by the emoji itself.
    Unicode(String),
    /// A Discord server's custom emoji, by its id.
    Custom(String),
}

impl Target {
    /// A custom emoji's id, or none for one of Unicode's own.
    pub fn custom(&self) -> Option<&str> {
        match self {
            Self::Custom(id) => Some(id),
            Self::Unicode(_) => None,
        }
    }
}

/// Reads `link` as [`unicode_link`] and [`custom_link`] write it.
pub fn target(link: &str) -> Option<Target> {
    let found = link.strip_prefix(LINK)?;
    if let Some(id) = found.strip_prefix("c/") {
        return (!id.is_empty()).then(|| Target::Custom(id.to_owned()));
    }
    let emoji: Option<String> = found
        .split('-')
        .map(|point| u32::from_str_radix(point, 16).ok().and_then(char::from_u32))
        .collect();
    emoji.filter(|emoji| !emoji.is_empty()).map(Target::Unicode)
}

/// A Unicode emoji's card says it can be used anywhere.
pub const DEFAULT_EMOJI: &str = "This is a default emoji. It can be used anywhere.";
/// A server's emoji, in the card of one whose server this account knows.
pub const SERVER_EMOJI: &str = "This emoji is from one of your servers.";
/// A server's emoji, in the card of one whose server this account can't
/// reach: one it never joined, or one that's gone. Its card says only this,
/// with no count of who's online and no separator, since Discord tells it
/// nothing.
pub const OTHER_EMOJI: &str = "Invite-Only Server";
/// A server's sticker's card, as Discord's says it: one from the server of
/// the room it shows in, with Nitro and without.
pub const HERE_STICKER: &str = "This sticker is from this server. You can use it everywhere.";
pub const HERE_STICKER_NO_NITRO: &str =
    "This sticker is from this server. Get Nitro to use it everywhere.";
/// One from another of this account's servers. Discord's adds "Type its
/// name in the chat bar to use it." with Nitro; Yapfast's composer doesn't
/// suggest stickers, so its card stops before it.
pub const SERVER_STICKER: &str = "This sticker is from one of your servers.";
pub const SERVER_STICKER_NO_NITRO: &str =
    "This sticker is from one of your servers. Get Nitro to use it everywhere.";
/// One neither in this account's servers nor in Discord's packs.
pub const OTHER_STICKER: &str =
    "This sticker is from a server that is either invite-only or unavailable.";
/// The heading over the server, on each card.
pub const EMOJI_FROM: &str = "This emoji is from";
pub const STICKER_FROM: &str = "This sticker is from";

/// What a card shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    /// Its name: an emoji's between colons (":sob:", ":pog:"), a sticker's
    /// as the message names it ("sob"); empty for a custom emoji no message
    /// on screen shows.
    pub name: String,
    pub description: String,
    /// A custom emoji's or a sticker's picture, by address.
    pub picture: Option<String>,
    /// A custom emoji whose picture moves.
    pub animated: bool,
    /// The server it is from, when it is one this account can reach.
    pub server: Option<CardServer>,
}

/// The server a card names, which opens on a click.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardServer {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    /// How many of its members are online, such as "3,999 Online", once
    /// Discord said; empty before.
    pub online: String,
    /// "Discoverable Server", or empty.
    pub note: &'static str,
}

impl CardServer {
    /// How many are online, then what the server is, on one line, as
    /// Discord's card has them.
    pub fn counts(&self) -> String {
        match (self.online.is_empty(), self.note.is_empty()) {
            (false, false) => format!("{} · {}", self.online, self.note),
            _ => format!("{}{}", self.online, self.note),
        }
    }
}

/// What Discover's note says.
const DISCOVERABLE: &str = "Discoverable Server";

/// `count` with its thousands marked, as Discord writes "3,999".
pub fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.char_indices() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// The card of an emoji, `server` being the one its id is listed in.
pub fn emoji(target: &Target, server: Option<&ChatServer>, state: &AppState) -> Card {
    let id = match target {
        Target::Unicode(emoji) => {
            return Card {
                name: super::shortcode(emoji),
                description: DEFAULT_EMOJI.into(),
                ..Card::default()
            };
        }
        Target::Custom(id) => id,
    };
    // A custom emoji's name and picture come from the message that showed it:
    // its server's list of emoji may not reach here, being another server's.
    let listed = state
        .messages
        .iter()
        .chain(state.aside.iter().flat_map(|aside| aside.messages.iter()))
        .flat_map(|message| &message.emoji)
        .find(|emoji| &emoji.id == id);
    // The server its card shows: one this account is in, and can reach. One
    // it left, or that's out of reach, shows no server at all, and says why.
    let server = server.filter(|server| !server.unavailable);
    // Discover lists a server whose preview says so, and Yapfast keeps its
    // own word for a Community server either way: a preview that answers
    // without the feature would otherwise take the note away from a server
    // that has it.
    let server = server.map(|server| {
        let preview = state.server_previews.get(&server.id);
        let discoverable = server.community || preview.is_some_and(|preview| preview.discoverable);
        card_server(server, preview, discoverable)
    });
    Card {
        name: listed.map_or_else(String::new, |emoji| emoji.text.clone()),
        description: if server.is_some() {
            SERVER_EMOJI
        } else {
            OTHER_EMOJI
        }
        .into(),
        picture: listed.map(|emoji| emoji.picture.clone()),
        animated: listed.is_some_and(|emoji| emoji.animated),
        server,
    }
}

/// The card an emoji opens, with the server the app knows it from: a custom
/// emoji's is the one that lists it.
pub fn emoji_for(app: &Application, target: &Target) -> Card {
    let server = target
        .custom()
        .and_then(|id| app.emoji_server(id))
        .and_then(|server| app.state.servers.iter().find(|known| known.id == server));
    emoji(target, server.map(|server| &**server), &app.state)
}

/// The id of the sticker a message's attachment is, from its picture's
/// address: `/stickers/{id}.png?size=160`, `.gif` or `.json`.
fn sticker_id(url: &str) -> Option<&str> {
    let name = url.rsplit('/').next()?;
    name.split(['.', '?']).next().filter(|id| !id.is_empty())
}

/// What a default sticker's card says, naming its pack as Discord's does.
pub fn pack_sticker(pack: &str) -> String {
    format!("This is a {pack} sticker.")
}

/// The card of message `item`'s attachment `index`, a sticker: its name as
/// the message names it, and what it is as Discord's card says. A message's
/// sticker names no server, so it's looked for among this account's
/// servers, then in Discord's packs; one in neither is from a server out of
/// reach. None when the message or attachment is gone.
pub fn sticker(app: &Application, item: &str, index: usize) -> Option<Card> {
    let state = &app.state;
    let attachment = state.message(item)?.attachments.get(index)?;
    let url = attachment.url.as_deref().unwrap_or_default();
    let id = sticker_id(url);
    let server = id
        .and_then(|id| app.sticker_server(id))
        .and_then(|server| state.servers.iter().find(|known| known.id == server))
        .filter(|server| !server.unavailable);
    let description = match server {
        Some(server) => {
            let here = state
                .selected
                .as_deref()
                .and_then(crate::discord::parse_room)
                .is_some_and(|(guild, _)| crate::discord::server_id(guild) == server.id);
            match (here, app.discord_nitro()) {
                (true, true) => HERE_STICKER,
                (true, false) => HERE_STICKER_NO_NITRO,
                (false, true) => SERVER_STICKER,
                (false, false) => SERVER_STICKER_NO_NITRO,
            }
            .to_owned()
        }
        None => id
            .and_then(|id| {
                state
                    .stickers
                    .packs
                    .iter()
                    .find(|pack| pack.stickers.iter().any(|sticker| sticker.id == id))
            })
            .map_or_else(|| OTHER_STICKER.to_owned(), |pack| pack_sticker(&pack.name)),
    };
    let server = server.map(|server| {
        let preview = state.server_previews.get(&server.id);
        let discoverable = preview.map_or(server.community, |preview| preview.discoverable);
        card_server(server, preview, discoverable)
    });
    Some(Card {
        name: attachment.name.clone(),
        description,
        picture: (!url.is_empty()).then(|| url.to_owned()),
        animated: false,
        server,
    })
}

fn card_server(
    server: &ChatServer,
    preview: Option<&crate::chat::ServerPreview>,
    discoverable: bool,
) -> CardServer {
    CardServer {
        id: server.id.clone(),
        name: server.name.clone(),
        icon: server.icon.clone(),
        online: preview
            .and_then(|preview| preview.online)
            .map_or_else(String::new, |online| format!("{} Online", grouped(online))),
        note: if discoverable { DISCOVERABLE } else { "" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_read_back_as_they_were_written() {
        assert_eq!(unicode_link("😭"), "yapfast:emoji/1f62d");
        assert_eq!(
            target(&unicode_link("👍🏽")),
            Some(Target::Unicode("👍🏽".into()))
        );
        assert_eq!(
            target(&custom_link("42")),
            Some(Target::Custom("42".into()))
        );
        assert_eq!(target("yapfast:emoji/c/"), None);
        assert_eq!(target("https://example.com"), None);
    }

    #[test]
    fn counts_join_with_a_dot_only_when_both_are_there() {
        let mut server = CardServer {
            online: "12 Online".into(),
            note: DISCOVERABLE,
            ..CardServer::default()
        };
        assert_eq!(server.counts(), "12 Online · Discoverable Server");
        server.note = "";
        assert_eq!(server.counts(), "12 Online");
        server.online.clear();
        assert_eq!(server.counts(), "");
        assert_eq!(grouped(3999), "3,999");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }
}
