//! What a profile shows beyond the core's `Profile`, worked out once each
//! time the core's view of it changes: the friendship it offers, whether it
//! messages, the mutual lines and tabs, the bio laid out, the activities
//! written out, and the colours its theme gives.

use std::sync::Arc;

use iced::Color;
use yapfast_core::{
    application::{AppState, MenuChoice, ProfileView, Relation},
    chat::{Activities, Activity, Presence, Profile, Wishlist},
    i18n::{self, tr},
};

use super::Event;
use crate::{app::Message, looks::Status, markdown::Body, pictures::Icon, style::hex, theme::mix};

/// The full profile's tabs, in Discord's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Board,
    Activity,
    Wishlist,
    Friends,
    Servers,
}

/// What a profile offers for a Discord friendship, as its buttons show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    None,
    Add,
    Sent,
    Accept,
    Friends,
}

impl Offer {
    /// Its button's icon, words and what a press does, for person `id`:
    /// Friends opens their menu, a request sent waits, Accept and Add
    /// befriend; none with nothing to offer.
    pub fn button(self, id: &str) -> Option<(Icon, &'static str, Option<Message>)> {
        let befriend = || Some(Message::Profile(Event::Befriend(id.to_owned())));
        Some(match self {
            Offer::None => return None,
            Offer::Friends => (
                Icon::Check,
                tr("Friends"),
                Some(Message::Profile(Event::More(id.to_owned()))),
            ),
            Offer::Sent => (Icon::UserPlus, tr("Friend Request Sent"), None),
            Offer::Accept => (Icon::UserPlus, tr("Accept Friend Request"), befriend()),
            Offer::Add => (Icon::UserPlus, tr("Add Friend"), befriend()),
        })
    }
}

/// A profile as its card, the DM's panel and the full profile show it.
#[derive(Debug)]
pub struct Look {
    /// The view it was made of, shared with the core, and how this account
    /// stood with them, which a new one is compared with.
    pub view: Arc<ProfileView>,
    pub relation: Relation,
    /// What the DM list said of them, over the profile's, for the panel
    /// beside their DM.
    pub live: Option<Live>,
    /// Whether they're around.
    pub presence: Option<Presence>,
    /// Their custom status or what they're doing, beside the picture.
    pub thought: Status,
    pub activities: Vec<ActivityLook>,
    pub bio: Option<Arc<Body>>,
    pub offer: Offer,
    pub own: bool,
    pub bot: bool,
    pub can_message: bool,
    pub note_editable: bool,
    /// The colour behind the banner without a picture: their accent.
    pub accent: Option<u32>,
    /// "Message @name", the card's box; empty when there's none.
    pub message_hint: String,
    pub mutual_friends: String,
    pub mutual_servers: String,
    /// The full profile's tab names.
    pub friends_label: String,
    pub servers_label: String,
    pub mutuals_known: bool,
    pub tabs: Vec<Tab>,
    pub recent_known: bool,
    pub friends_since: String,
    /// Loading, or why there's no more.
    pub note: String,
    /// The mutual friends' presences, in their order.
    pub presences: Vec<Presence>,
    /// The wishlist's line: why it doesn't show, or that it's private.
    pub wishlist_note: &'static str,
}

/// One of someone's activities, written out for its card as Discord's
/// client lays it out: a heading, a title, up to two lines, then small
/// lines with an icon (the party, an episode, how long it's run) or a
/// song's bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityLook {
    /// "Playing", "Listening to Spotify" and the like, as Discord writes it.
    pub heading: String,
    /// The bold title: a game's name, else the details, else the name.
    pub name: String,
    pub name_emoji: Option<String>,
    /// The lines under the title, in order.
    pub lines: Vec<String>,
    /// The party's line, "In a match (2 of 4)", beside a group icon.
    pub party: Option<String>,
    /// A show's episode, "S1E2", beside a topics icon.
    pub episode: Option<String>,
    /// How long it has run: counted from `since` (Unix seconds), never
    /// past `until`, beside the kind's `icon` (1 controller, 2 music,
    /// 3 screen) in the online green.
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub icon: u8,
    /// A song's or show's start and end, in Unix seconds, for its bar.
    pub bar: Option<(u64, u64)>,
    pub large: Option<String>,
    pub small: Option<String>,
    /// A stream's address, and what the title opens.
    pub url: Option<String>,
    pub link: Option<String>,
    /// A game played, whose menu on one's own card offers its privacy.
    pub game: bool,
}

impl Look {
    pub fn new(
        state: &AppState,
        view: &Arc<ProfileView>,
        live: Option<Live>,
        choices: &[MenuChoice],
        link: u32,
    ) -> Self {
        let profile = &view.profile;
        // Matrix IDs start with `@`; Discord user ids are numbers.
        let discord = !profile.id.is_empty() && !profile.id.starts_with('@');
        let relation = state.relation(&profile.id);
        // One's own shows no mutual friends or servers, as Discord's client
        // doesn't: it shares them with nobody.
        let own = discord && relation == Relation::Me;
        let bot = relation == Relation::Bot;
        // Discord has said who's mutual, so an empty list means none: not
        // while it loads, nor when it failed.
        let mutuals_known = view.kept || (!view.loading && view.note.is_none());
        let can_message = choices.contains(&MenuChoice::Message);
        let mut tabs = Vec::new();
        if discord && (own || !profile.widgets.is_empty()) {
            tabs.push(Tab::Board);
        }
        tabs.push(Tab::Activity);
        let wishlist = discord
            && (own
                || (profile
                    .wishlist_place
                    .as_ref()
                    .is_some_and(|place| !place.private)
                    && (relation == Relation::Friend || !profile.servers.is_empty())));
        if wishlist {
            tabs.push(Tab::Wishlist);
        }
        if discord && !own && !bot {
            tabs.push(Tab::Friends);
        }
        if discord && !own {
            tabs.push(Tab::Servers);
        }
        let bio = (!profile.bio.trim().is_empty()).then(|| {
            let mut body = crate::markdown::parse(&profile.bio, &[], link, "bio", |_| true);
            crate::markdown::with_emoji(&mut body, &profile.bio_emoji);
            Arc::new(body)
        });
        Self {
            thought: match &live {
                Some(Live {
                    activity: Some(line),
                    activities,
                    ..
                }) => status_of(line, activities),
                _ => thought(state, profile),
            },
            activities: activity_looks(match &live {
                Some(live) => &live.activities,
                None if profile.activities.is_empty()
                    && state.discord_user.as_deref() == Some(profile.id.as_str()) =>
                {
                    &state.discord_activities
                }
                None => &profile.activities,
            }),
            presence: live
                .as_ref()
                .and_then(|live| live.presence)
                .or(profile.presence),
            bio,
            offer: if !discord {
                Offer::None
            } else {
                match relation {
                    Relation::Friend => Offer::Friends,
                    Relation::Stranger if state.is_blocked(&profile.id) => Offer::None,
                    Relation::Stranger => Offer::Add,
                    Relation::Outgoing => Offer::Sent,
                    Relation::Incoming => Offer::Accept,
                    Relation::Me | Relation::Bot => Offer::None,
                }
            },
            own,
            bot,
            can_message,
            note_editable: discord && !own,
            accent: profile.accent,
            message_hint: if can_message {
                i18n::fill(tr("Message @{}"), &[&profile.name])
            } else {
                String::new()
            },
            mutual_friends: mutual_count(profile.friends.len(), true),
            mutual_servers: mutual_count(profile.servers.len(), false),
            friends_label: mutual_label(mutuals_known, profile.friends.len(), true),
            servers_label: mutual_label(mutuals_known, profile.servers.len(), false),
            mutuals_known,
            tabs,
            recent_known: !discord || state.preview || profile.recent_came,
            friends_since: if own {
                String::new()
            } else {
                state
                    .friends
                    .iter()
                    .find(|friend| friend.id == profile.id)
                    .and_then(|friend| friend.since)
                    .map(|ms| yapfast_core::clock::profile_day(ms / 1000))
                    .unwrap_or_default()
            },
            note: if view.loading && !view.kept {
                tr("Loading the profile...").to_owned()
            } else {
                view.note.clone().unwrap_or_default()
            },
            presences: profile
                .friends
                .iter()
                .map(|friend| {
                    state
                        .friends
                        .iter()
                        .find(|known| known.id == friend.id)
                        .and_then(|known| known.presence)
                        .unwrap_or(Presence::Offline)
                })
                .collect(),
            wishlist_note: wishlist_note(profile),
            view: view.clone(),
            relation,
            live,
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.view.profile
    }
}

/// What the DM list knows of someone now: the panel beside their DM shows
/// it over what their profile said as it opened.
#[derive(Debug, PartialEq)]
pub struct Live {
    pub presence: Option<Presence>,
    pub activity: Option<String>,
    pub activities: Activities,
}

/// The custom status a profile shows beside the picture, with its own line
/// breaks; a line that only names an activity goes, as its card says more.
/// One's own says the custom status this account has set, or none.
fn thought(state: &AppState, profile: &Profile) -> Status {
    if state.discord_user.as_deref() == Some(profile.id.as_str()) {
        return state
            .custom_status
            .as_ref()
            .map_or_else(Status::default, |status| {
                let text = status.text.trim();
                match (&status.emoji, &status.emoji_id) {
                    (_, Some(id)) => Status {
                        words: text.to_owned(),
                        emoji: yapfast_core::discord::emoji_picture(id),
                        ..Status::default()
                    },
                    (Some(emoji), None) if text.is_empty() => Status::words(emoji.clone()),
                    (Some(emoji), None) => Status::words(format!("{emoji} {text}")),
                    (None, None) => Status::words(text.to_owned()),
                }
            });
    }
    match profile.activity.as_deref() {
        Some(line) => status_of(line, &profile.activities),
        None => Status::default(),
    }
}

/// A status line as the bubble shows it: none when it only names one of
/// the activities, whose card says more.
fn status_of(line: &str, activities: &Activities) -> Status {
    if activities
        .iter()
        .any(|activity| activity.line().as_deref() == Some(line))
    {
        return Status::default();
    }
    let (emoji, words) = yapfast_core::discord::status_parts(line);
    Status {
        words: words.to_owned(),
        emoji,
        ..Status::default()
    }
}

fn activity_looks(activities: &Activities) -> Vec<ActivityLook> {
    yapfast_core::yapfast_discord::activity::shown(activities)
        .into_iter()
        .filter_map(activity_look)
        .collect()
}

/// An activity as its card shows it, as Discord's web client's activity
/// card (and Spotify's own card) lays it out; none for a Hang status,
/// which Discord gives no card.
pub fn activity_look(activity: &Activity) -> Option<ActivityLook> {
    use yapfast_core::chat::ActivityKind as Kind;
    let heading = activity.heading()?;
    let text = |text: &Option<Box<str>>| text.as_deref().map(str::to_owned);
    let (name_emoji, name) = yapfast_core::discord::status_parts(&activity.name);
    let name = name.to_owned();
    let details = text(&activity.details);
    let state = text(&activity.state);
    let spotify = activity.spotify();
    // The title, then the lines under it.
    let (title, lines) = if spotify {
        (
            details.unwrap_or(name),
            state
                .map(|state| yapfast_core::yapfast_discord::activity::artists(&state))
                .into_iter()
                .collect(),
        )
    } else if activity.kind == Kind::Playing {
        // A game's state goes on the party's line once it has a party.
        let state = state.filter(|_| activity.party.is_none());
        (name, [details, state].into_iter().flatten().collect())
    } else {
        // A Watching card's picture's words show only as an episode.
        let large = text(&activity.large_text).filter(|_| activity.kind != Kind::Watching);
        (
            details.unwrap_or(name),
            [state, large].into_iter().flatten().collect(),
        )
    };
    let party = (activity.kind == Kind::Playing)
        .then(|| party_line(activity.state.as_deref(), activity.party))
        .flatten();
    let episode = (activity.kind == Kind::Watching)
        .then(|| episode(activity.large_text.as_deref()?))
        .flatten();
    let seconds = |ms: u64| ms / 1000;
    let bar = match (activity.start, activity.end) {
        (Some(start), Some(end))
            if matches!(activity.kind, Kind::Listening | Kind::Watching) && end > start =>
        {
            Some((seconds(start), seconds(end)))
        }
        _ => None,
    };
    // Streams have no timer; the rest count from their start, or from when
    // Discord heard of them, unless a bar shows.
    let since = (bar.is_none() && activity.kind != Kind::Streaming)
        .then(|| activity.start.or(activity.created).map(seconds))
        .flatten();
    // The picture: its own, else the small one in its place, as Discord's
    // shows it; Spotify's card has no small one.
    let (large, small) = match (text(&activity.large_image), text(&activity.small_image)) {
        (Some(large), small) => (Some(large), small.filter(|_| !spotify)),
        (None, small) => (small, None),
    };
    Some(ActivityLook {
        heading,
        name: title,
        name_emoji,
        lines,
        party,
        episode,
        since,
        until: activity.end.map(seconds),
        icon: match activity.kind {
            Kind::Listening => 2,
            Kind::Watching => 3,
            _ => 1,
        },
        bar,
        game: activity.kind == Kind::Playing,
        large,
        small,
        url: text(&activity.url).filter(|_| activity.kind == Kind::Streaming),
        link: text(&activity.link),
    })
}

/// A game's party line as Discord writes it: its state, then "(2 of 4)",
/// or "(2)" with no most; none without a party.
fn party_line(state: Option<&str>, party: Option<(u64, u64)>) -> Option<String> {
    let (size, most) = party?;
    let count = match (size, most) {
        (size, most) if size > 0 && most > 0 => {
            Some(i18n::fill(tr("{0} of {1}"), &[&size, &most]))
        }
        (size, _) if size > 0 => Some(size.to_string()),
        _ => None,
    };
    match (state, count) {
        (Some(state), Some(count)) => Some(format!("{state} ({count})")),
        (state, count) => state.map(str::to_owned).or(count),
    }
}

/// "S1E2" for a picture's words holding "Season 1, Episode 2" (any
/// words, as Discord's `/\w+ (\d+), \w+ (\d+)/` finds them), as its
/// Watching card shows them; none for other words.
fn episode(words: &str) -> Option<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    words.match_indices(", ").find_map(|(at, _)| {
        let (before, after) = (&words[..at], &words[at + 2..]);
        let season_at = before.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let season = &before[season_at..];
        let named = before[..season_at].strip_suffix(' ')?;
        (!season.is_empty() && named.ends_with(word)).then_some(())?;
        let rest = after.trim_start_matches(word);
        (rest.len() < after.len()).then_some(())?;
        let number = rest.strip_prefix(' ')?;
        let episode = &number[..number.len()
            - number
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len()];
        (!episode.is_empty()).then(|| format!("S{season}E{episode}"))
    })
}

/// How long an activity has run, as Discord's timer writes it: `4:05`, or
/// `1:04:05` past an hour.
pub fn timer(seconds: u64) -> String {
    let (hours, minutes, rest) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

/// "1 Mutual Friend" or "5 Mutual Servers", as the card's line says it.
fn mutual_count(count: usize, friends: bool) -> String {
    if friends {
        i18n::trn("{n} Mutual Friend", "{n} Mutual Friends", count)
    } else {
        i18n::trn("{n} Mutual Server", "{n} Mutual Servers", count)
    }
}

/// A mutual tab's name, as the client's: none till Discord has said, then
/// how many.
fn mutual_label(known: bool, count: usize, friends: bool) -> String {
    match (known, count, friends) {
        (false, _, true) => tr("Mutual Friends").to_owned(),
        (false, _, false) => tr("Mutual Servers").to_owned(),
        (true, 0, true) => tr("No Mutual Friends").to_owned(),
        (true, 0, false) => tr("No Mutual Servers").to_owned(),
        (true, count, friends) => mutual_count(count, friends),
    }
}

/// A Discord wishlist's line: why it doesn't show, or that it's private.
fn wishlist_note(profile: &Profile) -> &'static str {
    let private = profile
        .wishlist_place
        .as_ref()
        .is_some_and(|place| place.private);
    match &profile.wishlist {
        Wishlist::Unasked | Wishlist::Loading => tr("Loading the wishlist..."),
        Wishlist::Hidden => tr("This wishlist is private."),
        Wishlist::Failed => tr("Couldn't load the wishlist."),
        Wishlist::Loaded(items) if private && !items.is_empty() => {
            tr("Your wishlist is private: only you can see it.")
        }
        Wishlist::Loaded(_) => "",
    }
}

/// How long ago a time in Unix milliseconds was, as the Activity tab says it.
pub fn time_ago(at: u64) -> String {
    let minutes = yapfast_core::clock::now_ms().saturating_sub(at) / 60_000;
    match minutes {
        0 => tr("Just now").to_owned(),
        1..60 => i18n::fill(tr("{}m ago"), &[&minutes]),
        60..1440 => i18n::fill(tr("{}h ago"), &[&(minutes / 60)]),
        _ => i18n::fill(tr("{}d ago"), &[&(minutes / 1440)]),
    }
}

/// `00:42`, `12:05` or `01:02:03`, padded as Discord's client writes them.
pub fn clock(seconds: u64) -> String {
    let (hours, minutes, rest) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes:02}:{rest:02}")
    }
}

/// The colours a profile is drawn in: Yapfast's own, or its theme's, as
/// Discord's client works them out. A theme is light when its first
/// colour's brightness is at least half; it lies under a 60% wash, white
/// on a light one and black on a dark one, and takes dark or white text.
#[derive(Clone, Copy, Debug)]
pub struct Inks {
    pub themed: bool,
    pub light: bool,
    /// The theme's first colour at full strength, and both under the wash.
    pub primary: Color,
    pub start: Color,
    pub end: Color,
    pub ink: Color,
    pub ink2: Color,
    pub ink3: Color,
    pub hover: Color,
    /// What lies behind the avatar's ring, where the banner ends.
    pub ground: Color,
    /// The status's bubble and its outline.
    pub bubble: Color,
    pub bubble_edge: Color,
    /// Tiles on the full profile, the note's field, the message box.
    pub tile: Color,
    pub field: Color,
    pub message: Color,
}

impl Inks {
    pub fn new(theme: &crate::theme::Theme, colors: Option<[u32; 2]>, plain: Color) -> Self {
        let Some([first, second]) = colors else {
            return Self {
                themed: false,
                light: false,
                primary: plain,
                start: plain,
                end: plain,
                ink: theme.text,
                ink2: theme.secondary,
                ink3: theme.muted,
                hover: theme.hover,
                ground: plain,
                bubble: theme.raised,
                bubble_edge: theme.edge,
                tile: theme.raised,
                field: theme.sunken,
                message: Color {
                    a: 0.08,
                    ..theme.text
                },
            };
        };
        let (primary, secondary) = (hex(first), hex(second));
        let light = 0.299 * primary.r + 0.587 * primary.g + 0.114 * primary.b >= 0.5;
        let wash = |color: Color| {
            let over = if light { Color::WHITE } else { Color::BLACK };
            mix(color, over, 0.4)
        };
        let alpha = |black: bool, a: f32| {
            let base = if black { Color::BLACK } else { Color::WHITE };
            Color { a, ..base }
        };
        let start = wash(primary);
        Self {
            themed: true,
            light,
            primary,
            start,
            end: wash(secondary),
            ink: alpha(light, 0.96),
            ink2: alpha(light, 0.88),
            ink3: alpha(light, 0.76),
            hover: alpha(light, if light { 0.08 } else { 0.04 }),
            ground: start,
            bubble: if light { Color::WHITE } else { hex(0x232427) },
            bubble_edge: if light {
                alpha(true, 0.15)
            } else {
                hex(0x404044)
            },
            tile: if light {
                alpha(false, 0.24)
            } else {
                alpha(false, 0.08)
            },
            field: if light {
                alpha(false, 0.3)
            } else {
                alpha(true, 0.2)
            },
            message: if light {
                alpha(false, 0.24)
            } else {
                alpha(false, 0.12)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use yapfast_core::yapfast_discord::activity::activities;

    use super::*;

    /// The card of one activity read from Discord's JSON.
    fn card(value: serde_json::Value) -> Option<ActivityLook> {
        activity_look(&activities(&json!([value]))[0])
    }

    /// Its heading, title and lines, as the card shows them.
    fn words(look: &ActivityLook) -> (&str, &str, Vec<&str>) {
        (
            &look.heading,
            &look.name,
            look.lines.iter().map(String::as_str).collect(),
        )
    }

    // Each case follows Discord's web client's activity card (module
    // 802880 `e0`: title `ee`, second line `et`, third line `en`; the
    // small lines of module 282197 and 78871) and its Spotify card
    // (module 960549 `K`).

    #[test]
    fn a_game_is_titled_by_its_name_then_details_and_state() {
        let game = card(json!({"type": 0, "name": "Factorio", "details": "Building",
            "state": "Nauvis", "timestamps": {"start": 1_000_000}}))
        .unwrap();
        assert_eq!(
            words(&game),
            ("Playing", "Factorio", vec!["Building", "Nauvis"])
        );
        assert_eq!((game.since, game.icon, game.bar), (Some(1000), 1, None));
        assert!(game.game);
        let bare = card(json!({"type": 0, "name": "Factorio"})).unwrap();
        assert_eq!(words(&bare), ("Playing", "Factorio", vec![]));
        assert_eq!(bare.since, None, "no start and no created_at: no timer");
    }

    #[test]
    fn a_games_party_takes_its_state_to_its_own_line() {
        let party = card(json!({"type": 0, "name": "Halo", "details": "Slayer",
            "state": "In a match", "party": {"size": [2, 4]}}))
        .unwrap();
        assert_eq!(words(&party), ("Playing", "Halo", vec!["Slayer"]));
        assert_eq!(party.party.as_deref(), Some("In a match (2 of 4)"));
        let bare = card(json!({"type": 0, "name": "Halo", "party": {"size": [3, 0]}})).unwrap();
        assert_eq!(bare.party.as_deref(), Some("3"));
        assert_eq!(party_line(None, Some((0, 0))), None);
    }

    #[test]
    fn a_stream_is_titled_by_its_details_then_state_and_picture_words() {
        let stream = card(json!({"type": 1, "name": "Twitch", "details": "Speedrun",
            "state": "Celeste", "url": "https://twitch.tv/ann",
            "assets": {"large_text": "Chapter 7"}, "timestamps": {"start": 5000}}))
        .unwrap();
        assert_eq!(
            words(&stream),
            ("Streaming Twitch", "Speedrun", vec!["Celeste", "Chapter 7"])
        );
        assert_eq!(stream.since, None, "streams have no timer");
        assert_eq!(stream.url.as_deref(), Some("https://twitch.tv/ann"));
        let untitled =
            card(json!({"type": 1, "name": "Mine", "url": "https://twitch.tv/ann"})).unwrap();
        assert_eq!(words(&untitled), ("Streaming", "Mine", vec![]));
    }

    #[test]
    fn another_apps_song_is_titled_by_its_details_then_state_and_album() {
        // A picture of Discord on a phone, 2026-11-22.
        let song = card(
            json!({"type": 2, "name": "Good Looking", "application_id": "1",
            "details": "Baby, I'm high octane", "state": "Fever in a shock wave",
            "assets": {"large_text": "Good Looking"},
            "timestamps": {"start": 1_000, "end": 181_000}}),
        )
        .unwrap();
        assert_eq!(
            words(&song),
            (
                "Listening to Good Looking",
                "Baby, I'm high octane",
                vec!["Fever in a shock wave", "Good Looking"]
            )
        );
        assert_eq!((song.bar, song.since), (Some((1, 181)), None));
        let no_details = card(json!({"type": 2, "name": "Radio", "state": "Live"})).unwrap();
        assert_eq!(words(&no_details), ("Listening to", "Radio", vec!["Live"]));
        let started =
            card(json!({"type": 2, "name": "Radio", "timestamps": {"start": 9000}})).unwrap();
        assert_eq!(
            (started.since, started.icon, started.bar),
            (Some(9), 2, None)
        );
    }

    #[test]
    fn spotify_lists_the_artists_and_no_album() {
        let song = card(json!({"type": 2, "name": "Spotify", "details": "Song",
            "state": "Ann; Bo", "sync_id": "6kkwzB6hXLIONkEk9JciA6",
            "assets": {"large_image": "spotify:ab67616d0000b273", "large_text": "Album",
                "small_image": "spotify:x"},
            "timestamps": {"start": 1_000, "end": 181_000}}))
        .unwrap();
        assert_eq!(
            words(&song),
            ("Listening to Spotify", "Song", vec!["Ann, Bo"])
        );
        assert!(song.link.is_some(), "the title opens the song");
        assert_eq!(song.small, None, "Spotify's card has no small picture");
        assert_eq!(song.bar, Some((1, 181)));
    }

    #[test]
    fn watching_shows_no_third_line_but_an_episode() {
        let show = card(json!({"type": 3, "name": "Crunchy", "details": "A show",
            "state": "Dubbed", "assets": {"large_text": "Season 1, Episode 2"},
            "timestamps": {"start": 1_000}}))
        .unwrap();
        assert_eq!(words(&show), ("Watching Crunchy", "A show", vec!["Dubbed"]));
        assert_eq!(show.episode.as_deref(), Some("S1E2"));
        assert_eq!((show.since, show.icon), (Some(1), 3));
        let film =
            card(json!({"type": 3, "name": "Films", "assets": {"large_text": "Dune"}})).unwrap();
        assert_eq!(words(&film), ("Watching", "Films", vec![]));
        assert_eq!(film.episode, None);
        assert_eq!(
            episode("My show - Season 10, Episode 3 (HD)").as_deref(),
            Some("S10E3")
        );
        assert_eq!(episode("Season one, Episode 3"), None);
    }

    #[test]
    fn competing_is_titled_by_its_details_then_state_and_picture_words() {
        let cup = card(json!({"type": 5, "name": "Arena", "details": "Finals",
            "state": "Round 2", "assets": {"large_text": "Gold"}, "created_at": 4000}))
        .unwrap();
        assert_eq!(
            words(&cup),
            ("Competing in Arena", "Finals", vec!["Round 2", "Gold"])
        );
        assert_eq!(cup.since, Some(4), "no start: counted from created_at");
    }

    #[test]
    fn hang_status_has_no_card_and_repeats_show_once() {
        assert_eq!(
            card(json!({"type": 6, "name": "Hang Status", "state": "chilling"})),
            None
        );
        let looks = activity_looks(&activities(&json!([
            {"type": 6, "name": "Hang Status", "state": "chilling"},
            {"type": 0, "name": "Halo"},
            {"type": 0, "name": "Halo", "details": "again"},
        ])));
        assert_eq!(looks.len(), 1);
    }

    #[test]
    fn a_small_picture_alone_takes_the_big_ones_place() {
        let small = card(json!({"type": 0, "name": "Game",
            "assets": {"small_image": "mp:external/a/b.png"}}))
        .unwrap();
        assert!(small.large.is_some());
        assert_eq!(small.small, None);
    }

    #[test]
    fn timers_read_as_discords() {
        assert_eq!(timer(65), "1:05");
        assert_eq!(timer(3_725), "1:02:05");
        assert_eq!(clock(65), "01:05");
    }
}
