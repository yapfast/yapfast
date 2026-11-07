//! When the member list's Activity panel asks for its feed, and for who of it
//! this channel may show. Discord's client fetches the feed whenever it's due;
//! this asks only while a server's channel shows, and otherwise as it does:
//! not out of focus, disconnected or long idle, nor with the cards hidden and
//! a feed held.

use std::sync::Arc;

use super::*;

/// How long without input counts as idle for the feed, in milliseconds.
#[cfg(feature = "discord")]
const FEED_IDLE_MS: u64 = 15 * 60 * 1000;

/// Where the feed's asking stands.
pub(super) struct FeedAsks {
    /// The feed may have changed since it was last asked for.
    due: bool,
    fetching: bool,
    /// A feed came, even an empty one.
    held: bool,
    focused: bool,
    /// When the last input was, once there's been none for a while.
    quiet_since: Option<u64>,
    /// The room whose people were asked for from this feed.
    people_asked: Option<String>,
}

impl Default for FeedAsks {
    fn default() -> Self {
        Self {
            due: true,
            fetching: false,
            held: false,
            focused: true,
            quiet_since: None,
            people_asked: None,
        }
    }
}

impl Application {
    /// Asks for what the panel lacks, if it may ask now.
    #[cfg(feature = "discord")]
    pub(super) fn ask_activity(&mut self) -> Vec<Effect> {
        let Some(room) = self
            .state
            .selected_room()
            .filter(|room| !room.direct && !room.forum && discord::parse_room(&room.id).is_some())
        else {
            return vec![];
        };
        if self.state.preview || !self.state.discord_online {
            return vec![];
        }
        let asks = &mut self.feed_asks;
        let mut effects = vec![];
        if asks.people_asked.as_deref() != Some(&room.id) && !self.state.activity_feed.is_empty() {
            let mut users: Vec<Arc<str>> = self
                .state
                .activity_feed
                .iter()
                .map(|entry| entry.author.clone())
                .collect();
            users.sort_unstable();
            users.dedup();
            asks.people_asked = Some(room.id.clone());
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ActivityPeople {
                    room: room.id.clone(),
                    users,
                },
            )));
        }
        let idle = asks
            .quiet_since
            .is_some_and(|since| crate::clock::now_ms().saturating_sub(since) > FEED_IDLE_MS);
        let hidden_and_held = self.state.activity_cards_hidden && asks.held;
        if asks.due && !asks.fetching && asks.focused && !idle && !hidden_and_held {
            asks.due = false;
            asks.fetching = true;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ActivityFeed,
            )));
        }
        effects
    }

    #[cfg(not(feature = "discord"))]
    pub(super) fn ask_activity(&mut self) -> Vec<Effect> {
        vec![]
    }

    pub(super) fn show_activity_feed(
        &mut self,
        entries: Arc<[crate::chat::FeedEntry]>,
    ) -> Vec<Effect> {
        // One asked for before signing out isn't this account's.
        if !self.feed_asks.fetching {
            return vec![];
        }
        self.feed_asks.fetching = false;
        self.feed_asks.held = true;
        self.feed_asks.people_asked = None;
        self.state.activity_feed = entries;
        self.ask_activity()
    }

    pub(super) fn activity_feed_due(&mut self) -> Vec<Effect> {
        self.feed_asks.due = true;
        self.ask_activity()
    }

    pub(super) fn show_activity_people(
        &mut self,
        room: String,
        people: Arc<[crate::chat::FeedPerson]>,
    ) {
        if self.state.selected.as_deref() == Some(&room) {
            self.state.activity_people = Some((room, people));
        }
    }

    pub(super) fn window_focused(&mut self, focused: bool) -> Vec<Effect> {
        self.feed_asks.focused = focused;
        if focused { self.ask_activity() } else { vec![] }
    }

    /// Input stopped, or came again after stopping.
    pub(super) fn feed_quiet_since(&mut self, since: Option<u64>) -> Vec<Effect> {
        self.feed_asks.quiet_since = since;
        if since.is_none() {
            self.ask_activity()
        } else {
            vec![]
        }
    }

    pub(super) fn set_activity_cards_hidden(&mut self, hidden: bool) -> Vec<Effect> {
        self.state.activity_cards_hidden = hidden;
        if hidden { vec![] } else { self.ask_activity() }
    }

    /// Lets the feed go with the account it was of.
    pub(super) fn forget_activity_feed(&mut self) {
        self.feed_asks = FeedAsks::default();
        self.state.activity_feed = Arc::default();
        self.state.activity_people = None;
    }
}

#[cfg(all(test, feature = "discord"))]
mod tests;
