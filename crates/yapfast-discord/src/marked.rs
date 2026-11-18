//! What a snapshot marks on each server for this account: what it may do in
//! each channel, mutes, unread state and notification settings. The marks
//! sit beside the server rather than in a copy of it, so a server stays
//! shared with the service while only its marks change.

use std::{ops::Deref, sync::Arc};

use crate::{
    notifications::Notifications,
    ready::{Access, Channel, Server},
};

/// A server as a snapshot lists it, with this account's marks beside it.
/// The cache saves the marks in the server's own object.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MarkedServer {
    #[serde(flatten)]
    pub server: Arc<Server>,
    #[serde(default)]
    pub marks: Arc<ServerMarks>,
}

/// What's this account's in a server.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMarks {
    /// Muted in the account's notification settings.
    pub muted: bool,
    /// This account's roles there.
    pub roles: Vec<String>,
    /// This account's notification settings there, shared with the
    /// service's while unchanged.
    pub notifications: Arc<Notifications>,
    /// The notification settings keep @everyone and @here, and role
    /// mentions, from calling on this account.
    pub suppress_everyone: bool,
    pub suppress_roles: bool,
    /// Each text channel's, each voice channel's, then each thread's, in
    /// the server's order.
    pub channels: Vec<ChannelMarks>,
}

/// What's this account's in a channel or thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChannelMarks {
    /// What this account may do there.
    pub access: Access,
    /// This account may mention @everyone, @here and every role there.
    pub mention_everyone: bool,
    /// Muted, directly or through its category.
    pub muted: bool,
    /// Messages this account hasn't read.
    pub unread: bool,
    /// Unread messages that mention this account.
    pub mentions: u64,
    /// The snowflake of the last message this account read there, or 0.
    pub last_read: u64,
    /// A thread gone quiet; see [`crate::ready::Thread::quiet_at`].
    pub quiet: bool,
    /// A thread this account has a read state of its own in: it opened or
    /// joined it. A forum post it never opened shows as new, rather than
    /// unread.
    pub opened: bool,
}

/// A channel or thread of a [`MarkedServer`], with its marks.
#[derive(Clone, Copy, Debug)]
pub struct MarkedChannel<'a> {
    pub channel: &'a Channel,
    pub marks: ChannelMarks,
}

impl ServerMarks {
    /// The marks of `server`'s channel or thread `id`, filling in the
    /// defaults for any channel these marks don't have yet; for tests and
    /// tools.
    pub fn channel_mut(&mut self, server: &Server, id: &str) -> Option<&mut ChannelMarks> {
        let mut channels = server
            .text_channels
            .iter()
            .chain(&server.voice_channels)
            .chain(&server.threads);
        let at = channels.position(|channel| channel.id == id)?;
        let count = server.text_channels.len() + server.voice_channels.len() + server.threads.len();
        if self.channels.len() < count {
            self.channels.resize(count, ChannelMarks::default());
        }
        self.channels.get_mut(at)
    }
}

impl Deref for MarkedChannel<'_> {
    type Target = Channel;

    fn deref(&self) -> &Channel {
        self.channel
    }
}

impl Deref for MarkedServer {
    type Target = Server;

    fn deref(&self) -> &Server {
        &self.server
    }
}

/// A server with no marks: those a cache from before them reads as.
impl From<Arc<Server>> for MarkedServer {
    fn from(server: Arc<Server>) -> Self {
        Self {
            server,
            marks: Arc::default(),
        }
    }
}

impl From<Server> for MarkedServer {
    fn from(server: Server) -> Self {
        Arc::new(server).into()
    }
}

impl MarkedServer {
    /// `server` with the marks `channel` gives each of its channels and
    /// threads, and `marks`' own; for tests and tools.
    pub fn marked(
        server: Server,
        marks: ServerMarks,
        channel: impl FnMut(&Channel) -> ChannelMarks,
    ) -> Self {
        let channels = server
            .text_channels
            .iter()
            .chain(&server.voice_channels)
            .chain(&server.threads)
            .map(channel)
            .collect();
        Self {
            server: Arc::new(server),
            marks: Arc::new(ServerMarks { channels, ..marks }),
        }
    }

    /// The marks of the channel at `at` in [`ServerMarks::channels`]; the
    /// defaults where a cache from before marks were kept apart has none.
    fn at(&self, at: usize) -> ChannelMarks {
        self.marks.channels.get(at).copied().unwrap_or_default()
    }

    fn list<'a>(
        &'a self,
        channels: &'a [Channel],
        from: usize,
    ) -> impl DoubleEndedIterator<Item = MarkedChannel<'a>> + Clone + 'a {
        channels
            .iter()
            .enumerate()
            .map(move |(at, channel)| MarkedChannel {
                channel,
                marks: self.at(from + at),
            })
    }

    /// Its text and announcement channels, in Discord's order.
    pub fn text_channels(&self) -> impl DoubleEndedIterator<Item = MarkedChannel<'_>> + Clone {
        self.list(&self.server.text_channels, 0)
    }

    pub fn voice_channels(&self) -> impl DoubleEndedIterator<Item = MarkedChannel<'_>> + Clone {
        self.list(&self.server.voice_channels, self.server.text_channels.len())
    }

    /// See [`Server::threads`].
    pub fn threads(&self) -> impl DoubleEndedIterator<Item = MarkedChannel<'_>> + Clone {
        self.list(
            &self.server.threads,
            self.server.text_channels.len() + self.server.voice_channels.len(),
        )
    }

    /// Its text channels, voice channels and threads.
    pub fn channels(&self) -> impl Iterator<Item = MarkedChannel<'_>> + Clone {
        self.text_channels()
            .chain(self.voice_channels())
            .chain(self.threads())
    }

    /// A text or voice channel, or a thread, by id.
    pub fn channel(&self, id: &str) -> Option<MarkedChannel<'_>> {
        self.channels().find(|channel| channel.id == id)
    }

    /// Whether the channel list lists only the channels this account
    /// follows, as Discord's client lists them: Show All Channels turned
    /// off, which it heeds only in a Community server.
    pub fn follows_only(&self) -> bool {
        self.server.community && self.marks.notifications.follows_only()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn each_channel_has_its_own_marks() {
        let channel = |id: &str| Channel {
            id: id.into(),
            ..Default::default()
        };
        let server = Server {
            text_channels: vec![channel("11"), channel("12")],
            voice_channels: vec![channel("21")],
            threads: vec![channel("31")],
            ..Default::default()
        };
        let marked = MarkedServer::marked(server, ServerMarks::default(), |channel| ChannelMarks {
            mentions: channel.id.parse().unwrap(),
            ..Default::default()
        });
        let mentions = |channels: Vec<MarkedChannel<'_>>| {
            channels
                .iter()
                .map(|channel| (channel.id.clone(), channel.marks.mentions.to_string()))
                .all(|(id, mentions)| id == mentions)
        };
        assert!(mentions(marked.text_channels().collect()));
        assert!(mentions(marked.voice_channels().collect()));
        assert!(mentions(marked.threads().collect()));
        assert_eq!(marked.channel("21").unwrap().marks.mentions, 21);
        assert_eq!(marked.channels().count(), 4);
    }

    #[test]
    fn a_cache_from_before_has_its_servers_unmarked() {
        let saved = json!({"id": "10", "name": "Server", "muted": true, "my_roles": ["20"],
            "text_channels": [{"id": "11", "name": "general", "position": 0, "parent": null,
                "overwrites": [], "access": "Locked", "mention_everyone": false,
                "unread": true, "mentions": 1, "muted": false}],
            "voice_channels": [], "categories": {}, "owner_id": null, "roles": {},
            "role_names": {}, "role_colors": {}, "role_positions": {},
            "mentionable_roles": [], "premium_tier": 0, "emojis": [], "joined_at": null,
            "banner": null, "icon": null, "member_count": null});
        let server: MarkedServer = serde_json::from_value(saved).unwrap();
        assert_eq!(server.name, "Server");
        assert_eq!(
            server.text_channels().next().unwrap().marks,
            ChannelMarks::default()
        );
        // Saved now, the marks come back.
        let marked = MarkedServer {
            marks: Arc::new(ServerMarks {
                muted: true,
                channels: vec![ChannelMarks {
                    unread: true,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..server
        };
        let saved = serde_json::to_value(&marked).unwrap();
        assert_eq!(saved["name"], "Server");
        assert_eq!(
            serde_json::from_value::<MarkedServer>(saved).unwrap(),
            marked
        );
    }
}
