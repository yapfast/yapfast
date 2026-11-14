//! What a profile shows beyond the core's `Profile`, worked out once each
//! time the core's view of it changes, as the Slint client's `profile_data`
//! does: the friendship it offers, whether it messages, the mutual lines
//! and tabs, the bio laid out, the activities written out, and the
//! colours its theme gives.

use std::sync::Arc;

use iced::Color;
use yapfast_core::{
    application::{AppState, MenuChoice, ProfileView, Relation},
    chat::{Activities, Activity, Presence, Profile, Wishlist},
};

use crate::{looks::Status, markdown::Body, style::hex, theme::mix};

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

/// A profile as its card, the DM's panel and the full profile show it.
#[derive(Debug)]
pub struct Look {
    /// The view it was made of, and how this account stood with them,
    /// which a new one is compared with.
    pub view: ProfileView,
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

/// One of someone's activities, as the Slint client's `ActivityRow`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityLook {
    /// "PLAYING", "LISTENING TO SPOTIFY" and the like.
    pub heading: String,
    pub name: String,
    pub name_emoji: Option<String>,
    pub details: String,
    pub state: String,
    pub large: Option<String>,
    pub small: Option<String>,
    /// When it started and ends, in Unix seconds.
    pub start: Option<u64>,
    pub end: Option<u64>,
    /// A stream's address, and what the title opens.
    pub url: Option<String>,
    pub link: Option<String>,
}

impl Look {
    pub fn new(
        state: &AppState,
        view: &ProfileView,
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
                format!("Message @{}", profile.name)
            } else {
                String::new()
            },
            mutual_friends: mutual_count(profile.friends.len(), "Friend"),
            mutual_servers: mutual_count(profile.servers.len(), "Server"),
            friends_label: mutual_label(mutuals_known, profile.friends.len(), "Friend"),
            servers_label: mutual_label(mutuals_known, profile.servers.len(), "Server"),
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
                "Loading the profile...".to_owned()
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
/// it over what their profile said as it opened, as the Slint client's.
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
    activities.iter().map(activity_look).collect()
}

/// An activity as its card shows it, as the Slint client writes it out.
fn activity_look(activity: &Activity) -> ActivityLook {
    let text = |text: &Option<Box<str>>| text.as_deref().map(str::to_owned);
    // A Hang status's emoji goes before its words, as a status's does.
    let name = match &activity.emoji {
        Some(emoji) => format!("{emoji} {}", activity.name),
        None => activity.name.to_string(),
    };
    let (name_emoji, name) = yapfast_core::discord::status_parts(&name);
    let state = match (&activity.state, activity.party) {
        (Some(state), Some((size, most))) => format!("{state} ({size} of {most})"),
        (None, Some((size, most))) => format!("{size} of {most}"),
        (state, None) => state.as_deref().unwrap_or_default().to_owned(),
    };
    // Spotify's card is titled by the song, then by whom and on which album.
    let (name, details, state) = if activity.spotify() {
        let after = |word: &str, text: &Option<Box<str>>| {
            text.as_deref()
                .map_or(String::new(), |text| format!("{word} {text}"))
        };
        (
            activity.details.as_deref().unwrap_or(name).to_owned(),
            after("by", &activity.state),
            after("on", &activity.large_text),
        )
    } else {
        (
            name.to_owned(),
            activity.details.as_deref().unwrap_or_default().to_owned(),
            state,
        )
    };
    ActivityLook {
        heading: activity.heading().to_uppercase(),
        name,
        name_emoji,
        details,
        state,
        large: text(&activity.large_image),
        small: text(&activity.small_image),
        start: activity.start.map(|ms| ms / 1000),
        end: activity.end.map(|ms| ms / 1000),
        url: text(&activity.url),
        link: text(&activity.link),
    }
}

impl ActivityLook {
    /// It counts time, which a clock brings up to date each second.
    pub fn timed(&self) -> bool {
        self.start.is_some() || self.end.is_some()
    }
}

/// "1 Mutual Friend" or "5 Mutual Servers", as the card's line says it.
fn mutual_count(count: usize, what: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} Mutual {what}{plural}")
}

/// A mutual tab's name, as the client's: none till Discord has said, then
/// how many.
fn mutual_label(known: bool, count: usize, what: &str) -> String {
    match (known, count) {
        (false, _) => format!("Mutual {what}s"),
        (true, 0) => format!("No Mutual {what}s"),
        (true, 1) => format!("1 Mutual {what}"),
        (true, count) => format!("{count} Mutual {what}s"),
    }
}

/// A Discord wishlist's line: why it doesn't show, or that it's private.
fn wishlist_note(profile: &Profile) -> &'static str {
    let private = profile
        .wishlist_place
        .as_ref()
        .is_some_and(|place| place.private);
    match &profile.wishlist {
        Wishlist::Unasked | Wishlist::Loading => "Loading the wishlist...",
        Wishlist::Hidden => "This wishlist is private.",
        Wishlist::Failed => "Couldn't load the wishlist.",
        Wishlist::Loaded(items) if private && !items.is_empty() => {
            "Your wishlist is private: only you can see it."
        }
        Wishlist::Loaded(_) => "",
    }
}

/// How long ago a time in Unix milliseconds was, as the Activity tab says it.
pub fn time_ago(at: u64) -> String {
    let minutes = yapfast_core::clock::now_ms().saturating_sub(at) / 60_000;
    match minutes {
        0 => "Just now".to_owned(),
        1..60 => format!("{minutes}m ago"),
        60..1440 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1440),
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
