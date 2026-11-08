//! Discord's Browse Channels for a Community server: every channel this
//! account sees, by category, each with a box ticked while its channel list
//! follows it, and each category with a switch that follows every channel in
//! it. It opens from the top of the server's channel list in place of a
//! conversation, as Discord's client opens it. Ticking a box follows the
//! channel and lists only the channels followed, turning Show All Channels
//! off; with none followed yet, every box shows ticked, and clearing one
//! follows all the others. The rows are kept only while it shows.

use super::*;

/// Browse Channels on screen, for the server navigated to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BrowseChannels {
    /// The server's id.
    pub server: String,
    /// Channels outside any category first, then each category's heading
    /// and its channels, in Discord's order.
    pub rows: Vec<BrowsedChannel>,
}

/// A channel or a category's heading in Browse Channels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BrowsedChannel {
    /// Its room id.
    pub id: String,
    pub name: String,
    /// What the channel is for, its first line as plain text, with whom and
    /// what it mentions by name.
    pub topic: String,
    pub kind: BrowsedKind,
    /// Its box is ticked, or a category's switch on.
    pub followed: bool,
    /// Followed through its category, which only unfollowing the category
    /// changes.
    pub by_category: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BrowsedKind {
    Category,
    #[default]
    Text,
    Announcement,
    Forum,
    Voice,
}

impl AppState {
    /// Whether a server's channel list opens Browse Channels: a Community
    /// server's, while Discord is connected.
    pub(super) fn offers_browse(&self, server: &str) -> bool {
        self.discord_online
            && self
                .servers
                .iter()
                .any(|known| known.id == server && known.community)
    }
}

impl Application {
    /// Opens Browse Channels for the server navigated to, a Community
    /// server, in place of its conversation.
    pub(super) fn open_browse(&mut self) -> Vec<Effect> {
        let Navigation::Server(server) = self.state.navigation.clone() else {
            return vec![];
        };
        if self.state.browse.is_some() || !self.state.offers_browse(&server) {
            return vec![];
        }
        let previous = self.state.place();
        let effects = self.deselect();
        self.state.browse = self
            .browse_rows(&server)
            .map(|rows| BrowseChannels { server, rows });
        if let Some(browse) = &self.state.browse
            && !self.state.nav_walking
        {
            self.state
                .record_visit(previous, &HistoryPlace::Browse(browse.server.clone()));
        }
        effects
    }

    /// Shows Discord's latest word on Browse Channels, or closes it when
    /// the server no longer offers it.
    #[cfg(feature = "discord")]
    pub(super) fn refresh_browse(&mut self) {
        let Some(server) = self
            .state
            .browse
            .as_ref()
            .map(|browse| browse.server.clone())
        else {
            return;
        };
        let rows = self
            .state
            .offers_browse(&server)
            .then(|| self.browse_rows(&server))
            .flatten();
        match rows {
            Some(rows) => {
                if let Some(browse) = self.state.browse.as_mut()
                    && browse.rows != rows
                {
                    browse.rows = rows;
                }
            }
            None => self.state.browse = None,
        }
    }

    /// A server's rows in Browse Channels: its channels this account sees,
    /// not threads, under their categories, in the order its channel list
    /// has them.
    fn browse_rows(&self, server: &str) -> Option<Vec<BrowsedChannel>> {
        #[cfg(feature = "discord")]
        {
            use yapfast_discord::ready::Access;
            let (known, None, _) = self.discord_place(server)? else {
                return None;
            };
            let settings = &known.marks.notifications;
            let names = self.discord.as_ref()?.names.read();
            // Show All Channels on with none followed: Discord's client
            // shows every box ticked, as turning it off would follow them.
            let all = !settings.follows_only() && !settings.follows_any();
            let mut channels: Vec<_> = known
                .text_channels()
                .map(|channel| (false, channel))
                .chain(known.voice_channels().map(|channel| (true, channel)))
                .filter(|(_, channel)| channel.marks.access != Access::Hidden)
                .map(|(voice, channel)| (voice, channel.channel))
                .collect();
            channels.sort_by_cached_key(|(voice, channel)| {
                (
                    known.category_order(channel),
                    *voice,
                    channel.position,
                    channel.id.clone(),
                )
            });
            let mut rows = Vec::with_capacity(channels.len());
            let mut heading: Option<&str> = None;
            for (voice, channel) in channels {
                let category = channel
                    .parent
                    .as_deref()
                    .filter(|parent| known.categories.contains_key(*parent));
                let by_category = category.is_some_and(|category| settings.of(category).followed());
                if let Some(category) = category
                    && heading != Some(category)
                {
                    heading = Some(category);
                    rows.push(BrowsedChannel {
                        id: discord::room_id(&known.id, category),
                        name: known.categories[category].name.clone(),
                        topic: String::new(),
                        kind: BrowsedKind::Category,
                        followed: all || by_category,
                        by_category: false,
                    });
                }
                rows.push(BrowsedChannel {
                    id: discord::room_id(&known.id, &channel.id),
                    name: channel.name.clone(),
                    topic: channel
                        .topic
                        .as_deref()
                        .map(|topic| chat::plain_snippet(&names.render(Some(known), topic)))
                        .unwrap_or_default(),
                    kind: if voice {
                        BrowsedKind::Voice
                    } else if channel.forum.is_some() {
                        BrowsedKind::Forum
                    } else if channel.announcement {
                        BrowsedKind::Announcement
                    } else {
                        BrowsedKind::Text
                    },
                    followed: all || by_category || settings.of(&channel.id).followed(),
                    by_category,
                });
            }
            Some(rows)
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = server;
            None
        }
    }

    /// Ticks a channel's box in Browse Channels, or a category's switch,
    /// or clears it, as Discord's client does: with Show All Channels on,
    /// it turns it off too, and with none followed yet, follows every
    /// channel this account sees but one cleared. The boxes change when
    /// Discord confirms it.
    pub(super) fn browse_follow(&mut self, room: String, follow: bool) -> Vec<Effect> {
        let Some(row) = self
            .state
            .browse
            .as_ref()
            .and_then(|browse| browse.rows.iter().find(|row| row.id == room))
        else {
            return vec![];
        };
        if row.followed == follow || row.by_category || !self.state.discord_online {
            return vec![];
        }
        #[cfg(feature = "discord")]
        if let Some((known, Some(channel), _)) = self.discord_place(&room) {
            use yapfast_discord::ready::Access;
            let settings = &known.marks.notifications;
            if settings.follows_only() {
                let flags = settings.of(channel).follow_flags(follow);
                return vec![Effect::Backend(Command::Discord(
                    DiscordCommand::FollowChannel { room, flags },
                ))];
            }
            let follow: Vec<(String, u64)> = if settings.follows_any() {
                vec![(
                    channel.to_owned(),
                    settings.of(channel).follow_flags(follow),
                )]
            } else {
                // Every channel but the one cleared; a category ticked
                // joins them.
                known
                    .text_channels()
                    .chain(known.voice_channels())
                    .filter(|known| known.marks.access != Access::Hidden && known.id != channel)
                    .map(|known| known.channel.id.as_str())
                    .chain(follow.then_some(channel))
                    .map(|id| (id.to_owned(), settings.of(id).follow_flags(true)))
                    .collect()
            };
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::ShowAllChannels {
                    server: discord::server_id(&known.id),
                    flags: settings.show_all_flags(false),
                    follow,
                },
            ))];
        }
        vec![]
    }
}
