//! Places to go, searched as Discord's clients search them: where a
//! message may be forwarded, in the order Discord's Forward To dialog lists
//! them, and where the quick switcher goes. With nothing searched for,
//! Forward To lists those chosen, and both then list the conversations
//! opened lately, then those this account uses most as Discord's clients
//! count it, fifteen at most, without the message's own conversation or the
//! one open. A search ranks every place by how well its name matches,
//! raised by how much it's used, and ties by name. A Matrix message's
//! Forward To lists its rooms as the sidebar does, those whose name holds
//! the search.

use std::{cell::OnceCell, collections::HashMap, sync::Arc};

use super::{AppState, Application, Effect, PlacesUsed};
use crate::chat::{ChatRoom, ChatServer, Command, DiscordCommand, PlaceUses};
use crate::discord;
use crate::network::Network;

/// The most places listed before a search.
const LISTED: usize = 15;
/// The conversations opened lately that are listed, and that are kept.
const VISITED: usize = 8;
const VISITED_KEPT: usize = 16;
/// The most places the quick switcher lists for a search.
const FOUND: usize = 50;
/// The most used places that count, and the uses of each kept.
const FREQUENT: usize = 100;
const SAMPLES: usize = 10;
const DAY_MS: i64 = 86_400_000;

impl PlacesUsed {
    /// Asks for Discord's counts once, the first time a Discord message is
    /// forwarded or the quick switcher opens.
    pub(super) fn ask(&mut self) -> bool {
        let ask = !self.fresh && !self.asked;
        self.asked |= ask;
        ask
    }

    /// Counts a conversation opened at `now`: any as opened lately, and a
    /// Discord one for Discord's counts too.
    pub(super) fn visit(&mut self, room: &str, now: u64) {
        self.visited.retain(|id| id != room);
        self.visited.insert(0, room.to_owned());
        self.visited.truncate(VISITED_KEPT);
        self.changes += 1;
        let Some((_, channel)) = discord::parse_room(room) else {
            return;
        };
        self.pending.push((channel.to_owned(), now));
        track(&mut self.uses, channel, now);
    }

    /// Takes Discord's counts in place of the old, with this session's uses
    /// added again, as Discord's client does until it saves its own.
    pub(super) fn came(&mut self, result: Result<Vec<(String, PlaceUses)>, String>) {
        self.asked = false;
        match result {
            Ok(places) => {
                self.fresh = true;
                self.changes += 1;
                self.uses = places.into_iter().collect();
                for (id, at) in &self.pending {
                    track(&mut self.uses, id, *at);
                }
            }
            Err(error) => tracing::debug!(%error, "could not fetch the places used"),
        }
    }

    /// Counts the changes to what ranks places, so a client can tell when
    /// its list of them is out of date.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// The most used places' frecency at `now`, as Discord's client works it
    /// out, by channel id; and the highest.
    fn frequent(&self, now: u64) -> (HashMap<&str, u32>, u32) {
        let mut ranked: Vec<(&str, u32)> = self
            .uses
            .iter()
            .map(|(id, uses)| (id.as_str(), frecency(uses, now)))
            .filter(|(_, frecency)| *frecency > 0)
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ranked.truncate(FREQUENT);
        let highest = ranked.first().map_or(0, |(_, frecency)| *frecency);
        (ranked.into_iter().collect(), highest)
    }
}

/// Adds a use of `id` at `at`, keeping the latest [`SAMPLES`].
fn track(uses: &mut HashMap<String, PlaceUses>, id: &str, at: u64) {
    let place = uses.entry(id.to_owned()).or_default();
    place.total += 1;
    let after = place.recent.partition_point(|used| *used <= at);
    place.recent.insert(after, at);
    let over = place.recent.len().saturating_sub(SAMPLES);
    place.recent.drain(..over);
}

/// What a use counts for by its age, as Discord's client weighs it.
fn weight(days: i64) -> u32 {
    match days {
        0 => 100,
        1 => 70,
        2..=3 => 50,
        4..=6 => 30,
        7.. => 10,
        _ => 1,
    }
}

/// A place's frecency: its uses in all, scaled by how recent the latest
/// were; none for a place not used lately.
fn frecency(uses: &PlaceUses, now: u64) -> u32 {
    let score: u32 = uses
        .recent
        .iter()
        .take(SAMPLES)
        .map(|at| weight((now as i64 - *at as i64) / DAY_MS))
        .sum();
    if score == 0 {
        return 0;
    }
    (u64::from(uses.total) * u64::from(score)).div_ceil(uses.recent.len() as u64) as u32
}

/// How well `name` matches `query`, both in lower case, as Discord's search
/// scores a channel or group: the whole name, its start, within it, each
/// word within it, its letters in order. A channel's dashes match spaces.
fn name_score(name: &str, query: &str) -> u32 {
    let name = name.replace('-', " ");
    if name == query {
        10
    } else if name.starts_with(query) {
        7
    } else if name.contains(query) {
        5
    } else if query
        .split([',', ' '])
        .filter(|word| !word.is_empty())
        .all(|word| name.contains(word))
    {
        3
    } else if in_order(&name, query) {
        1
    } else {
        0
    }
}

/// How well a person's `name` matches `query`, as Discord's search scores
/// people: by its start, or its letters in order.
fn person_score(name: &str, query: &str) -> u32 {
    if name.starts_with(query) {
        10
    } else if in_order(name, query) {
        1
    } else {
        0
    }
}

/// The Discord channel a room is.
fn channel(room: &ChatRoom) -> Option<&str> {
    discord::parse_room(&room.id).map(|(_, channel)| channel)
}

/// `query`'s letters all come in `name`, in order.
fn in_order(name: &str, query: &str) -> bool {
    let mut letters = name.chars();
    query
        .chars()
        .filter(|letter| !letter.is_whitespace())
        .all(|wanted| letters.any(|letter| letter == wanted))
}

/// A place the quick switcher goes to: a conversation, or a server.
#[derive(Clone, Copy)]
pub enum Place<'a> {
    Room(&'a ChatRoom),
    Server(&'a ChatServer),
}

impl Place<'_> {
    pub fn name(&self) -> &str {
        match self {
            Place::Room(room) => &room.name,
            Place::Server(server) => &server.name,
        }
    }
}

/// What a search looks for, by the character it starts with.
#[derive(Clone, Copy)]
enum Only {
    Everything,
    /// `@`: people.
    People,
    /// `#` in Forward To: channels.
    Channels,
    /// `#` in the quick switcher: text channels.
    Text,
    /// `!`: voice channels.
    Voice,
    /// `*`: servers.
    Servers,
}

impl Only {
    /// What `query` looks for, and the rest of it. Forward To knows `@` and
    /// `#`, and the quick switcher `!` and `*` too, as Discord's do.
    fn of(query: &str, switcher: bool) -> (Self, &str) {
        let only = match query.chars().next() {
            Some('@') => Only::People,
            Some('#') if switcher => Only::Text,
            Some('#') => Only::Channels,
            Some('!') if switcher => Only::Voice,
            Some('*') if switcher => Only::Servers,
            _ => return (Only::Everything, query),
        };
        (only, query[1..].trim_start())
    }
}

/// Ranks places by name and by how much this account uses them.
struct Ranking<'a> {
    state: &'a AppState,
    /// The most used places' frecency, by Discord id, and the highest.
    frequent: HashMap<&'a str, u32>,
    highest: u32,
    /// The servers' names in lower case, by their place in
    /// [`AppState::servers`], made the first time a search needs them.
    server_names: OnceCell<Vec<String>>,
}

impl<'a> Ranking<'a> {
    fn new(state: &'a AppState) -> Self {
        let (frequent, highest) = state.places_used.frequent(crate::clock::now_ms());
        Self {
            state,
            frequent,
            highest,
            server_names: OnceCell::new(),
        }
    }

    /// What using the Discord server or channel `id` raises a score by: up
    /// to twice for the most used.
    fn boost(&self, id: Option<&str>) -> f64 {
        let used = id.and_then(|id| self.frequent.get(id));
        1.0 + used.map_or(0.0, |used| {
            f64::from(*used) / f64::from(self.highest.max(1))
        })
    }

    /// Those listed with nothing searched for: `first`, then the
    /// conversations opened lately, then those used most.
    fn unsearched<'b>(
        &self,
        places: impl Iterator<Item = &'b ChatRoom>,
        first: &[&str],
    ) -> Vec<&'b ChatRoom> {
        let places: Vec<&ChatRoom> = places.collect();
        let visited: Vec<&str> = self
            .state
            .places_used
            .visited
            .iter()
            .map(String::as_str)
            .filter(|id| !first.contains(id) && places.iter().any(|room| room.id == *id))
            .take(VISITED)
            .collect();
        let mut listed: Vec<(usize, &ChatRoom)> = places
            .into_iter()
            .filter_map(|room| {
                let at = first
                    .iter()
                    .position(|id| *id == room.id)
                    .or_else(|| {
                        let at = visited.iter().position(|id| *id == room.id);
                        at.map(|at| first.len() + at)
                    })
                    .or_else(|| {
                        let used = *self.frequent.get(channel(room)?)?;
                        Some(usize::MAX - used as usize)
                    })?;
                Some((at, room))
            })
            .collect();
        listed.sort_by_key(|(at, _)| *at);
        listed
            .into_iter()
            .take(LISTED)
            .map(|(_, room)| room)
            .collect()
    }

    /// How well a conversation matches `query`, in lower case; not at all
    /// if it isn't what the search looks for.
    fn room(&self, room: &ChatRoom, only: Only, query: &str) -> f64 {
        let person = room.person.as_deref().filter(|_| room.direct);
        let wanted = match only {
            Only::Everything => true,
            Only::People => person.is_some(),
            Only::Channels => !room.direct,
            Only::Text => !room.direct && !room.voice,
            Only::Voice => !room.direct && room.voice,
            Only::Servers => false,
        };
        if !wanted {
            return 0.0;
        }
        let name = room.name.to_lowercase();
        let boost = self.boost(channel(room));
        match person {
            Some(person) => {
                // People get more for being friends, and for a DM.
                let friend = self
                    .state
                    .discord_friends
                    .iter()
                    .any(|friend| friend == person);
                let boost = boost + if friend { 0.2 } else { 0.0 } + 0.1;
                f64::from(person_score(&name, query)) * boost
            }
            None if room.direct => f64::from(name_score(&name, query)) * boost,
            None => {
                // A channel's server's name may hold words of it too.
                let in_server = || {
                    let servers = &self.state.servers;
                    let Some(at) = servers
                        .iter()
                        .position(|server| server.channels.contains(&room.id))
                    else {
                        return false;
                    };
                    let names = self.server_names.get_or_init(|| {
                        servers
                            .iter()
                            .map(|server| server.name.to_lowercase())
                            .collect()
                    });
                    let server = &names[at];
                    query
                        .split(' ')
                        .filter(|word| !word.is_empty())
                        .all(|word| name.contains(word) || server.contains(word))
                };
                let score = match name_score(&name, query) {
                    0 if query.contains(' ') && in_server() => 3,
                    score => score,
                };
                f64::from(score) * boost
            }
        }
    }

    /// How well a server matches `query`, in lower case.
    fn server(&self, server: &ChatServer, query: &str) -> f64 {
        let score = name_score(&server.name.to_lowercase(), query);
        f64::from(score) * self.boost(server.id.strip_prefix(discord::PREFIX))
    }
}

/// `name`'s letters in lower case, for ordering without allocating.
fn lower(name: &str) -> impl Iterator<Item = char> + '_ {
    name.chars().flat_map(char::to_lowercase)
}

/// The places that matched, the best first, and those as good by `name`
/// regardless of case.
fn found<T>(scored: impl Iterator<Item = (f64, T)>, name: impl Fn(&T) -> &str) -> Vec<T> {
    let mut found: Vec<(f64, T)> = scored.filter(|(score, _)| *score > 0.0).collect();
    found.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| lower(name(&a.1)).cmp(lower(name(&b.1))))
    });
    found.into_iter().map(|(_, place)| place).collect()
}

impl Application {
    /// Asks for Discord's counts of the places this account uses, the first
    /// time they rank places.
    pub(super) fn ask_places_used(&mut self) -> Vec<Effect> {
        if self.state.preview || !self.state.places_used.ask() {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::PlacesUsed,
        ))]
    }
}

impl AppState {
    /// Where the message being forwarded may go, in the order its dialog
    /// lists them.
    pub fn forward_places(&self) -> Vec<&ChatRoom> {
        let Some(forwarding) = &self.forwarding else {
            return vec![];
        };
        let places = self
            .rooms
            .iter()
            .map(Arc::as_ref)
            .filter(|room| self.can_forward_to(&forwarding.room, room));
        let query = forwarding.query.trim().to_lowercase();
        if Network::of(&forwarding.room) != Network::Discord {
            return places
                .filter(|room| room.name.to_lowercase().contains(&query))
                .collect();
        }
        let ranking = Ranking::new(self);
        if query.is_empty() {
            // Those chosen, the latest first, as when the search last
            // changed; and not the message's own conversation, unless it's
            // chosen.
            let first: Vec<&str> = forwarding.pinned.iter().rev().map(String::as_str).collect();
            let own = |room: &ChatRoom| {
                room.id == forwarding.room && !forwarding.chosen.contains(&room.id)
            };
            return ranking.unsearched(places.filter(|room| !own(room)), &first);
        }
        let (only, query) = Only::of(&query, false);
        found(
            places.map(|room| (ranking.room(room, only, query), room)),
            |room| &room.name,
        )
    }

    /// Where the quick switcher goes, in the order it lists them: with
    /// nothing searched for, the conversations opened lately but the one
    /// open, then those used most; otherwise the conversations and servers
    /// that match, the best first, up to [`FOUND`]. Conversations this
    /// account may not read stay out, and so do message requests, as the DM
    /// list leaves them out.
    pub fn switcher_places(&self) -> Vec<Place<'_>> {
        let Some(query) = &self.switcher else {
            return vec![];
        };
        let places = self.rooms.iter().map(Arc::as_ref).filter(|room| {
            !room.locked && room.category != discord::REQUESTS && room.category != discord::SPAM
        });
        let ranking = Ranking::new(self);
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            let open = self.selected.as_deref();
            let places = places.filter(|room| Some(room.id.as_str()) != open);
            return ranking
                .unsearched(places, &[])
                .into_iter()
                .map(Place::Room)
                .collect();
        }
        let (only, query) = Only::of(&query, true);
        let rooms = places.map(|room| (ranking.room(room, only, query), Place::Room(room)));
        let servers = self
            .servers
            .iter()
            .filter(|_| matches!(only, Only::Everything | Only::Servers))
            .map(|server| (ranking.server(server, query), Place::Server(server)));
        let mut found = found(rooms.chain(servers), Place::name);
        found.truncate(FOUND);
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn frecency_weighs_uses_by_age_as_discord_does() {
        let uses = |total, days: &[u64]| PlaceUses {
            total,
            recent: days
                .iter()
                .rev()
                .map(|days| NOW - days * DAY_MS as u64)
                .collect(),
        };
        assert_eq!(frecency(&uses(1, &[0]), NOW), 100);
        // Ten uses, five today and five a week or more ago, of twenty.
        let mixed = uses(20, &[0, 0, 0, 0, 0, 7, 8, 9, 30, 90]);
        assert_eq!(frecency(&mixed, NOW), (20 * 550u32).div_ceil(10));
        assert_eq!(
            frecency(&uses(4, &[1, 3, 5]), NOW),
            (4 * 150u32).div_ceil(3)
        );
        assert_eq!(frecency(&uses(3, &[]), NOW), 0);
    }

    #[test]
    fn tracking_keeps_the_latest_uses() {
        let mut places = PlacesUsed::default();
        for at in 0..12 {
            places.visit("discord:1:2", NOW + at);
        }
        let uses = &places.uses["2"];
        assert_eq!(uses.total, 12);
        assert_eq!(uses.recent, (2..12).map(|at| NOW + at).collect::<Vec<_>>());
        // Counts that come replace the old, with this session's added again.
        places.came(Ok(vec![(
            "2".into(),
            PlaceUses {
                total: 40,
                recent: vec![NOW - 1],
            },
        )]));
        assert_eq!(places.uses["2"].total, 52);
        assert_eq!(places.uses["2"].recent.len(), SAMPLES);
        assert_eq!(places.visited, ["discord:1:2"]);
    }

    #[test]
    fn names_score_as_discords_search_does() {
        assert_eq!(name_score("general", "general"), 10);
        assert_eq!(name_score("general-chat", "general chat"), 10);
        assert_eq!(name_score("general", "gen"), 7);
        assert_eq!(name_score("off-topic", "topic"), 5);
        assert_eq!(name_score("weekend plans", "plans week"), 3);
        assert_eq!(name_score("general", "gnl"), 1);
        assert_eq!(name_score("general", "xyz"), 0);
        assert_eq!(person_score("bob", "bo"), 10);
        assert_eq!(person_score("bob", "bb"), 1);
        assert_eq!(person_score("bob", "x"), 0);
    }
}
