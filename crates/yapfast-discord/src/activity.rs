//! What someone is doing, from their presence's activities, as their
//! profile shows it: a game, music, a stream or a Hang status, with its
//! pictures and times. A custom status shows on its own line, as
//! [`crate::member_list::activity`] writes it, so it isn't one of these.

use std::{borrow::Cow, sync::Arc};

use serde_json::Value;

use crate::{client, ready};

/// Someone's activities, shared from the gateway event to the profile card.
pub type Activities = Arc<[Activity]>;

/// The most activities kept for one person; Discord's profile shows a few.
const MOST: usize = 5;

/// What an application's icon's address starts with until Discord says
/// which picture it is; [`crate::service::Discord::download`] asks.
const ICON: &str = "discord-app-icon:";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Playing,
    Streaming,
    Listening,
    Watching,
    Competing,
    /// A Hang status, which Discord shows while they're in voice.
    Hang,
}

/// One activity, its text trimmed and its pictures as addresses.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Activity {
    pub kind: Kind,
    /// The game, the app, or a Hang status's words.
    pub name: Box<str>,
    pub details: Option<Box<str>>,
    pub state: Option<Box<str>>,
    /// How many are in their party, and how many it holds.
    pub party: Option<(u64, u64)>,
    /// Their party's id, which those playing together share; Active Now
    /// gives them one card.
    pub party_id: Option<Box<str>>,
    pub application_id: Option<Box<str>>,
    /// When it started and when it ends, in Unix milliseconds.
    pub start: Option<u64>,
    pub end: Option<u64>,
    /// When Discord was told of it, in Unix milliseconds: a card with no
    /// start counts from here, as Discord's does.
    pub created: Option<u64>,
    /// The addresses of its large and small pictures, and what they name.
    pub large_image: Option<Box<str>>,
    pub large_text: Option<Box<str>>,
    pub small_image: Option<Box<str>>,
    pub small_text: Option<Box<str>>,
    /// A stream's address, such as its Twitch channel.
    pub url: Option<Box<str>>,
    /// What its title opens: a Spotify song's page.
    pub link: Option<Box<str>>,
    /// A Hang status's emoji: Unicode, or a custom one as `<:name:id>`.
    pub emoji: Option<Box<str>>,
    /// Which of its words the line under a name shows, as the activity's
    /// `status_display_type` says: its name, state or details; none when
    /// it doesn't say.
    pub display: Option<Display>,
    /// The console it's played on, which its card's heading names.
    pub console: Option<Console>,
}

/// Which words of an activity the line under a name shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Display {
    Name,
    State,
    Details,
}

/// A console Discord heads a game's card with: "Playing on Xbox".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Console {
    Xbox,
    PlayStation,
}

impl Activity {
    /// An activity of `kind` named `name` and nothing more, which tests
    /// and the core's own cards fill in.
    pub fn new(kind: Kind, name: &str) -> Self {
        Self {
            kind,
            name: name.into(),
            details: None,
            state: None,
            party: None,
            party_id: None,
            application_id: None,
            start: None,
            end: None,
            created: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
            url: None,
            link: None,
            emoji: None,
            display: None,
            console: None,
        }
    }

    /// The words Discord's client shows for it under a name, beside its
    /// icon: the game, the stream's title, a Spotify song's artists, or
    /// what its `status_display_type` picks. None for a Hang status.
    pub fn short(&self) -> Option<String> {
        if self.kind == Kind::Hang {
            return None;
        }
        let name = &*self.name;
        let mut words = match self.kind {
            Kind::Streaming => self.details.as_deref().unwrap_or(name),
            _ => name,
        };
        match self.display {
            Some(Display::Name) => words = name,
            Some(Display::State) => words = self.state.as_deref().unwrap_or(words),
            Some(Display::Details) => words = self.details.as_deref().unwrap_or(words),
            None => {}
        }
        if self.kind == Kind::Playing {
            return Some(words.to_owned());
        }
        if self.spotify()
            && let Some(state) = &self.state
        {
            return Some(artists(state));
        }
        Some(words.to_owned())
    }

    /// The line Discord shows for it, as the subline's tooltip says it
    /// and as a member with no custom status is summed up, such as
    /// "Playing Factorio"; none for a Hang status.
    pub fn line(&self) -> Option<String> {
        let words = self.short()?;
        Some(match self.kind {
            Kind::Playing => format!("Playing {words}"),
            Kind::Streaming => format!("Streaming {words}"),
            Kind::Listening => format!("Listening to {words}"),
            Kind::Watching => format!("Watching {words}"),
            Kind::Competing => format!("Competing in {words}"),
            Kind::Hang => return None,
        })
    }

    /// Spotify's song, as Discord's card shows it: titled by the song, then
    /// by whom.
    pub fn spotify(&self) -> bool {
        self.kind == Kind::Listening && &*self.name == "Spotify" && self.application_id.is_none()
    }

    /// What Discord heads its card with: "Playing", "Listening to Spotify",
    /// "Watching", or "Competing in" with the name once there are
    /// details. None for a Hang status, which has no card.
    pub fn heading(&self) -> Option<String> {
        let named = |verb: &str| match self.details {
            Some(_) => format!("{verb} {}", self.name),
            None => verb.to_owned(),
        };
        Some(match self.kind {
            Kind::Playing => match self.console {
                Some(Console::Xbox) => "Playing on Xbox".into(),
                Some(Console::PlayStation) => "Playing on PlayStation".into(),
                None => "Playing".into(),
            },
            Kind::Streaming => match &*self.name {
                "Twitch" | "YouTube" => format!("Streaming {}", self.name),
                _ => "Streaming".into(),
            },
            Kind::Listening => named("Listening to"),
            Kind::Watching => named("Watching"),
            Kind::Competing => named("Competing in"),
            Kind::Hang => return None,
        })
    }
}

/// A Spotify song's artists as Discord lists them: its state's "; "
/// between names becomes ", ".
pub fn artists(state: &str) -> String {
    state.split("; ").collect::<Vec<_>>().join(", ")
}

/// The activities Discord gives cards and counts on a subline: no Hang
/// status, and one of each application or name, the first.
pub fn shown(activities: &[Activity]) -> Vec<&Activity> {
    let mut shown: Vec<&Activity> = Vec::new();
    for activity in activities
        .iter()
        .filter(|activity| activity.kind != Kind::Hang)
    {
        let seen = shown.iter().any(|kept| {
            (kept.application_id.is_some() && kept.application_id == activity.application_id)
                || kept.name == activity.name
        });
        if !seen {
            shown.push(activity);
        }
    }
    shown
}

/// How the one line under a name shows an activity, as Discord's member
/// list, DM list and account panel do: the first activity's icon, its
/// words when there's no custom status, how many more there are, then a
/// bullet before the custom status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subline {
    /// 0 for none, then 1 a controller (playing or competing), 2 a music
    /// note (listening) and 3 a screen (watching or streaming), all in the
    /// online green as Discord draws them.
    pub icon: u8,
    /// A bullet follows the icon: the line is a custom status.
    pub bullet: bool,
    /// What replaces the line, the activity's own words; none keeps the
    /// line.
    pub name: Option<String>,
    /// How many activities there are past the first, shown as "+2".
    pub more: u8,
}

/// The [`Subline`] for someone's activities and the line
/// [`crate::member_list::activity`] wrote for them.
pub fn subline(activities: &[Activity], line: Option<&str>) -> Subline {
    let shown = shown(activities);
    let Some(first) = shown.first() else {
        return Subline::default();
    };
    let icon = match first.kind {
        Kind::Playing | Kind::Competing => 1,
        Kind::Listening => 2,
        Kind::Watching | Kind::Streaming => 3,
        Kind::Hang => 0,
    };
    let more = u8::try_from(shown.len() - 1).unwrap_or(u8::MAX);
    if line.is_none_or(|line| line.is_empty()) || line == first.line().as_deref() {
        Subline {
            icon,
            bullet: false,
            name: first.short(),
            more,
        }
    } else {
        Subline {
            icon,
            bullet: true,
            name: None,
            more,
        }
    }
}

/// The activities of a presence, but a custom status, in Discord's order.
pub fn activities(activities: &Value) -> Activities {
    activities
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(activity)
        .take(MOST)
        .collect()
}

/// What another of this account's sessions shows: the first active session
/// but `own` of the gateway's list, as READY's `sessions` and
/// SESSIONS_REPLACE give it.
pub(crate) fn elsewhere(sessions: &Value, own: Option<&str>) -> Vec<Value> {
    sessions
        .as_array()
        .into_iter()
        .flatten()
        .find(|session| session["active"] == true && session["session_id"].as_str() != own)
        .and_then(|session| session["activities"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// This account's own activities as Discord's client shows them: what this
/// session sends and what another session does, each once, in the order
/// of anyone's, with one game at most.
pub(crate) fn own(here: &[Value], elsewhere: &[Value]) -> Activities {
    let rank = |activity: &Value| match activity["type"].as_u64() {
        Some(5) => 3,
        Some(1) => 2,
        Some(0) => 1,
        _ => 0,
    };
    let rich = |activity: &Value| {
        ["details", "party", "secrets", "state"]
            .iter()
            .any(|field| !activity[*field].is_null())
            || !activity["assets"]["large_image"].is_null()
            || !activity["assets"]["small_text"].is_null()
    };
    let mut all: Vec<&Value> = here
        .iter()
        .chain(elsewhere.iter())
        .filter(|activity| activity["type"] != 4)
        .collect();
    all.sort_by_key(|activity| {
        (
            std::cmp::Reverse(rank(activity)),
            std::cmp::Reverse(rich(activity)),
            std::cmp::Reverse(activity["created_at"].as_u64().unwrap_or(0)),
        )
    });
    let mut seen = Vec::new();
    all.retain(|activity| {
        let key = (
            &activity["type"],
            &activity["application_id"],
            &activity["name"],
        );
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    let mut games = 0;
    all.retain(|activity| {
        games += usize::from(activity["type"] == 0);
        activity["type"] != 0 || games == 1
    });
    all.into_iter().filter_map(activity).take(MOST).collect()
}

fn activity(activity: &Value) -> Option<Activity> {
    let kind = match activity["type"].as_u64()? {
        0 => Kind::Playing,
        1 => Kind::Streaming,
        2 => Kind::Listening,
        3 => Kind::Watching,
        5 => Kind::Competing,
        6 => Kind::Hang,
        _ => return None,
    };
    let text = |field: &str| {
        activity[field]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(Box::from)
    };
    let application_id = text("application_id");
    let timestamps = &activity["timestamps"];
    let assets = &activity["assets"];
    let image = |field: &str| {
        assets[field]
            .as_str()
            .and_then(|key| image(application_id.as_deref(), key))
    };
    let (name, details, emoji) = if kind == Kind::Hang {
        let emoji = custom_emoji(&activity["emoji"]);
        match activity["state"].as_str() {
            Some("custom") => (text("details")?, None, emoji),
            Some(state) => (hang_words(state).into(), None, emoji),
            None => return None,
        }
    } else {
        (text("name")?, text("details"), None)
    };
    let party = activity["party"]["size"]
        .as_array()
        .and_then(|size| Some((size.first()?.as_u64()?, size.get(1)?.as_u64()?)));
    Some(Activity {
        kind,
        name,
        details,
        state: if kind == Kind::Hang {
            None
        } else {
            text("state")
        },
        party,
        party_id: activity["party"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(Into::into),
        start: time(&timestamps["start"]),
        end: time(&timestamps["end"]),
        // A game Discord detected has no pictures, but its icon.
        large_image: image("large_image").or_else(|| {
            application_id
                .as_deref()
                .filter(|id| id.bytes().all(|byte| byte.is_ascii_digit()))
                .map(|id| format!("{ICON}{id}").into())
        }),
        large_text: assets["large_text"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(Box::from),
        small_image: image("small_image"),
        small_text: assets["small_text"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(Box::from),
        url: text("url").filter(|url| url.starts_with("https://")),
        link: spotify_link(activity),
        emoji,
        created: time(&activity["created_at"]),
        display: match activity["status_display_type"].as_u64() {
            Some(0) => Some(Display::Name),
            Some(1) => Some(Display::State),
            Some(2) => Some(Display::Details),
            _ => None,
        },
        console: match activity["platform"].as_str() {
            Some("xbox") => Some(Console::Xbox),
            Some("ps4" | "ps5") => Some(Console::PlayStation),
            _ => None,
        },
        application_id,
    })
}

/// A Spotify song's or episode's page, from the activity's `sync_id`, as
/// Discord's client opens it from the title.
fn spotify_link(activity: &Value) -> Option<Box<str>> {
    if activity["type"] != 2 || activity["name"] != "Spotify" {
        return None;
    }
    let id = activity["sync_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))?;
    let kind = match activity["metadata"]["type"].as_str() {
        Some("episode") => "episode",
        _ => "track",
    };
    Some(format!("https://open.spotify.com/{kind}/{id}").into())
}

/// A timestamp, which Discord sends as a number or a string of one.
fn time(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|&time| time > 0)
}

/// A Hang status's words for one of Discord's preset states.
fn hang_words(state: &str) -> &'static str {
    match state.split(':').next().unwrap_or_default() {
        "chilling" => "Chilling",
        "gaming" => "Gaming",
        "focusing" => "Focusing",
        "brb" => "Be right back",
        "eating" => "Eating",
        "watching" => "Watching stuff",
        _ => "Hanging out",
    }
}

/// An emoji object as a status writes it: Unicode, or `<:name:id>`.
fn custom_emoji(emoji: &Value) -> Option<Box<str>> {
    match (emoji["name"].as_str(), emoji["id"].as_str()) {
        (name, Some(id)) => Some(
            format!(
                "<{}:{}:{id}>",
                if emoji["animated"] == true { "a" } else { "" },
                name.unwrap_or("emoji"),
            )
            .into(),
        ),
        (Some(name), None) if !name.is_empty() => Some(name.into()),
        _ => None,
    }
}

/// The address of an activity's picture from its key: an application's
/// asset, an image through Discord's media proxy, or Spotify's, YouTube's or
/// Twitch's. Anyone may set these keys, so only plain ones resolve.
fn image(application: Option<&str>, key: &str) -> Option<Box<str>> {
    asset(application, key)
}

/// The address of a picture from its key; see [`image`].
pub(crate) fn asset(application: Option<&str>, key: &str) -> Option<Box<str>> {
    let plain = |text: &str, extra: &[u8]| {
        !text.is_empty()
            && !text.contains("..")
            && text.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || b"-_".contains(&byte) || extra.contains(&byte)
            })
    };
    let url = if let Some(path) = key.strip_prefix("mp:") {
        if !plain(path, b"./%=&?:+~") {
            return None;
        }
        format!("{}/{path}", ready::media())
    } else if let Some((service, id)) = key.split_once(':') {
        if !plain(id, b"") {
            return None;
        }
        let test = client::test_server();
        match (service, test) {
            ("spotify", None) => format!("https://i.scdn.co/image/{id}"),
            ("youtube", None) => format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"),
            ("twitch", None) => {
                format!("https://static-cdn.jtvnw.net/previews-ttv/live_user_{id}-320x180.jpg")
            }
            ("spotify" | "youtube" | "twitch", Some(server)) => format!("{server}/{service}/{id}"),
            _ => return None,
        }
    } else {
        let application = application.filter(|id| id.bytes().all(|byte| byte.is_ascii_digit()))?;
        if !plain(key, b"") {
            return None;
        }
        format!(
            "{}/app-assets/{application}/{key}.png?size=160",
            ready::cdn()
        )
    };
    Some(url.into())
}

/// An address standing for an application's icon, once it's named by id.
pub(crate) fn application_icon(id: &str) -> Option<Box<str>> {
    id.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| format!("{ICON}{id}").into())
}

/// The application whose icon an address of [`ICON`] stands for.
pub(crate) fn icon_application(url: &str) -> Option<&str> {
    url.strip_prefix(ICON)
}

/// The address of an application's icon, from its public object.
pub(crate) fn icon_url(application: &Value) -> Option<String> {
    let (id, icon) = (application["id"].as_str()?, application["icon"].as_str()?);
    let plain =
        |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_alphanumeric());
    (plain(id) && plain(icon))
        .then(|| format!("{}/app-icons/{id}/{icon}.png?size=160", ready::cdn()))
}

/// The address of a smaller copy of an activity's picture, for where it shows
/// small: decoded, Spotify's 640px cover takes 1.6 MiB and its 64px one 16 KiB.
/// YouTube's thumbnail goes from 480px wide to 120px, and the media proxy's
/// picture to 96px. An address of Discord's own CDN is 160px already.
pub fn small_picture(url: &str) -> Cow<'_, str> {
    const SPOTIFY: &str = "https://i.scdn.co/image/ab67616d0000b273";
    if let Some(cover) = url.strip_prefix(SPOTIFY) {
        return format!("https://i.scdn.co/image/ab67616d00004851{cover}").into();
    }
    if let Some(video) = url
        .strip_prefix("https://i.ytimg.com/vi/")
        .and_then(|rest| rest.strip_suffix("/hqdefault.jpg"))
    {
        return format!("https://i.ytimg.com/vi/{video}/default.jpg").into();
    }
    if url.starts_with(ready::media()) && !url.contains('?') {
        return format!("{url}?width=96&height=96").into();
    }
    url.into()
}

/// Whether `url` is a picture an activity may show from outside Discord:
/// Spotify's covers, YouTube's thumbnails or Twitch's previews.
pub fn outside_picture(url: &str) -> bool {
    [
        "https://i.scdn.co/image/",
        "https://i.ytimg.com/vi/",
        "https://static-cdn.jtvnw.net/previews-ttv/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Each of `values` read on its own: a presence keeps five.
    fn one_each(values: &Value) -> Vec<Activity> {
        values
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|value| activities(&json!([value])).to_vec())
            .collect()
    }

    #[test]
    fn subline_names_the_activity_without_a_status_and_bullets_a_status() {
        let parsed = activities(&json!([
            {"type": 0, "name": "Factorio"},
        ]));
        let without = subline(&parsed, Some("Playing Factorio"));
        assert_eq!(
            without,
            Subline {
                icon: 1,
                bullet: false,
                name: Some("Factorio".into()),
                more: 0,
            }
        );
        let with = subline(&parsed, Some("IT, AI and Games"));
        assert_eq!(
            with,
            Subline {
                icon: 1,
                bullet: true,
                name: None,
                more: 0,
            }
        );
        assert_eq!(
            subline(&parsed, None).name.as_deref(),
            Some("Factorio"),
            "no custom status: the activity's words"
        );
        assert_eq!(subline(&[], Some("brb")), Subline::default());
        let hang = activities(&json!([{"type": 6, "state": "custom", "details": "Chilling"}]));
        assert_eq!(subline(&hang, Some("Chilling")), Subline::default());
    }

    /// Each kind's icon and words under a name, as Discord's
    /// ActivityStatus (web module 566903 and 835072) picks them.
    #[test]
    fn subline_words_and_icons_follow_discords_activity_status() {
        let parsed = one_each(&json!([
            {"type": 0, "name": "Factorio", "details": "Building"},
            {"type": 1, "name": "Twitch", "details": "Speedrun", "url": "https://twitch.tv/ann"},
            {"type": 2, "name": "Spotify", "details": "Song", "state": "Ann; Bo"},
            {"type": 2, "name": "Good Looking", "details": "Baby, I'm high octane"},
            {"type": 3, "name": "YouTube", "details": "A video"},
            {"type": 5, "name": "Arena", "details": "Finals"},
        ]));
        let words: Vec<_> = parsed
            .iter()
            .map(|activity| {
                (
                    subline(std::slice::from_ref(activity), None).icon,
                    activity.short(),
                )
            })
            .collect();
        assert_eq!(
            words,
            [
                (1, Some("Factorio".into())),
                (3, Some("Speedrun".into())),
                (2, Some("Ann, Bo".into())),
                (2, Some("Good Looking".into())),
                (3, Some("YouTube".into())),
                (1, Some("Arena".into())),
            ]
        );
        assert_eq!(parsed[2].line().as_deref(), Some("Listening to Ann, Bo"));
        assert_eq!(subline(&parsed, None).more, 5, "+5 past the first");
        let named = one_each(&json!([
            {"type": 1, "name": "Twitch", "details": "Speedrun", "status_display_type": 0},
        ]));
        assert_eq!(
            named[0].short().as_deref(),
            Some("Twitch"),
            "0 asks for the name"
        );
        let shown = activities(&json!([
            {"type": 2, "name": "Music", "details": "Song", "state": "Artist", "status_display_type": 1},
            {"type": 2, "name": "Music", "details": "Song", "state": "Artist", "status_display_type": 2},
            {"type": 2, "name": "Music", "details": "Song", "status_display_type": 1},
        ]));
        assert_eq!(shown[0].short().as_deref(), Some("Artist"));
        assert_eq!(shown[1].short().as_deref(), Some("Song"));
        assert_eq!(
            shown[2].short().as_deref(),
            Some("Music"),
            "no state: the name"
        );
        assert_eq!(subline(&shown, None).more, 0, "one card per name");
    }

    /// The card headings of Discord's web module 770689.
    #[test]
    fn headings_name_the_app_only_once_there_are_details() {
        let parsed = one_each(&json!([
            {"type": 0, "name": "Halo"},
            {"type": 0, "name": "Halo", "platform": "xbox"},
            {"type": 0, "name": "Astro", "platform": "ps5"},
            {"type": 1, "name": "Twitch", "url": "https://twitch.tv/ann"},
            {"type": 1, "name": "Mine", "url": "https://twitch.tv/ann"},
            {"type": 2, "name": "Good Looking", "details": "Baby"},
            {"type": 2, "name": "Radio"},
            {"type": 3, "name": "YouTube", "details": "A video"},
            {"type": 3, "name": "Films"},
            {"type": 5, "name": "Arena", "details": "Finals"},
            {"type": 5, "name": "Cup"},
            {"type": 6, "name": "Hang Status", "state": "chilling"},
        ]));
        let headings: Vec<_> = parsed.iter().map(Activity::heading).collect();
        let want = [
            Some("Playing"),
            Some("Playing on Xbox"),
            Some("Playing on PlayStation"),
            Some("Streaming Twitch"),
            Some("Streaming"),
            Some("Listening to Good Looking"),
            Some("Listening to"),
            Some("Watching YouTube"),
            Some("Watching"),
            Some("Competing in Arena"),
            Some("Competing in"),
            None,
        ];
        assert_eq!(
            headings,
            want.map(|heading| heading.map(str::to_owned)).to_vec()
        );
    }

    #[test]
    fn shown_leaves_out_hang_and_repeats() {
        let parsed = activities(&json!([
            {"type": 6, "name": "Hang Status", "state": "chilling"},
            {"type": 0, "name": "Halo", "application_id": "1"},
            {"type": 0, "name": "Halo 2", "application_id": "1"},
            {"type": 2, "name": "Spotify"},
            {"type": 2, "name": "Spotify", "details": "Again"},
        ]));
        let names: Vec<_> = shown(&parsed)
            .iter()
            .map(|activity| &*activity.name)
            .collect();
        assert_eq!(names, ["Halo", "Spotify"]);
    }

    #[test]
    fn reads_a_game_with_its_pictures_and_times() {
        let parsed = activities(&json!([
            {"type": 4, "name": "Custom Status", "state": "brb"},
            {"type": 0, "name": "Factorio", "application_id": "42",
             "details": " Building a base ", "state": "In a party", "party": {"size": [2, 4]},
             "timestamps": {"start": 1_700_000_000_000_u64},
             "assets": {"large_image": "777", "large_text": "Nauvis", "small_image": "mp:external/a/b.png"}},
        ]));
        assert_eq!(parsed.len(), 1, "the custom status isn't one");
        let game = &parsed[0];
        assert_eq!((game.kind, &*game.name), (Kind::Playing, "Factorio"));
        assert_eq!(game.details.as_deref(), Some("Building a base"));
        assert_eq!(game.party, Some((2, 4)));
        assert_eq!(game.start, Some(1_700_000_000_000));
        assert_eq!(
            game.large_image.as_deref(),
            Some("https://cdn.discordapp.com/app-assets/42/777.png?size=160")
        );
        assert_eq!(game.large_text.as_deref(), Some("Nauvis"));
        assert_eq!(
            game.small_image.as_deref(),
            Some("https://media.discordapp.net/external/a/b.png")
        );
        assert_eq!(game.heading().as_deref(), Some("Playing"));
        assert_eq!(game.created, None);
    }

    #[test]
    fn small_pictures_ask_for_the_smaller_copy() {
        assert_eq!(
            small_picture("https://i.scdn.co/image/ab67616d0000b273abc"),
            "https://i.scdn.co/image/ab67616d00004851abc"
        );
        assert_eq!(
            small_picture("https://i.ytimg.com/vi/abc/hqdefault.jpg"),
            "https://i.ytimg.com/vi/abc/default.jpg"
        );
        let proxied = format!("{}/external/a/b.png", ready::media());
        assert_eq!(
            small_picture(&proxied),
            format!("{proxied}?width=96&height=96")
        );
        let icon = "https://cdn.discordapp.com/app-icons/42/abc.png?size=160";
        assert_eq!(small_picture(icon), icon);
    }

    #[test]
    fn reads_spotify_and_hang_statuses() {
        let parsed = activities(&json!([
            {"type": 2, "name": "Spotify", "details": "Song", "state": "Artist",
             "timestamps": {"start": "1000", "end": "181000"},
             "assets": {"large_image": "spotify:ab67616d0000b273"}},
            {"type": 6, "name": "Hang Status", "state": "brb:illocons"},
            {"type": 6, "name": "Hang Status", "state": "custom", "details": "Cooking",
             "emoji": {"name": "🍳"}},
        ]));
        assert_eq!(parsed[0].heading().as_deref(), Some("Listening to Spotify"));
        assert!(parsed[0].spotify());
        assert_eq!(parsed[0].link, None, "a song without its id links nowhere");
        assert_eq!(
            (parsed[0].start, parsed[0].end),
            (Some(1000), Some(181_000))
        );
        assert_eq!(
            parsed[0].large_image.as_deref(),
            Some("https://i.scdn.co/image/ab67616d0000b273")
        );
        assert_eq!(&*parsed[1].name, "Be right back");
        assert_eq!(
            (&*parsed[2].name, parsed[2].emoji.as_deref()),
            ("Cooking", Some("🍳"))
        );
    }

    #[test]
    fn a_spotify_song_links_to_its_page() {
        let parsed = activities(&json!([
            {"type": 2, "name": "Spotify", "sync_id": "6kkwzB6hXLIONkEk9JciA6"},
            {"type": 2, "name": "Spotify", "sync_id": "4rOoJ6Egrf8K2IrywzwOMk", "metadata": {"type": "episode"}},
            {"type": 2, "name": "Spotify", "sync_id": "../x"},
            {"type": 0, "name": "Spotify", "sync_id": "6kkwzB6hXLIONkEk9JciA6"},
        ]));
        let links: Vec<_> = parsed
            .iter()
            .map(|activity| activity.link.as_deref())
            .collect();
        assert_eq!(
            links,
            [
                Some("https://open.spotify.com/track/6kkwzB6hXLIONkEk9JciA6"),
                Some("https://open.spotify.com/episode/4rOoJ6Egrf8K2IrywzwOMk"),
                None,
                None,
            ]
        );
    }

    #[test]
    fn a_detected_game_shows_its_icon_and_its_line() {
        let parsed = activities(&json!([
            {"type": 0, "name": "Factorio", "application_id": "42"},
            {"type": 1, "name": "Twitch", "details": "Speedrun", "url": "https://twitch.tv/ann"},
            {"type": 1, "name": "Twitch", "url": "javascript:alert(1)"},
        ]));
        let icon = parsed[0].large_image.as_deref().unwrap();
        assert_eq!(icon_application(icon), Some("42"));
        assert_eq!(parsed[0].line().as_deref(), Some("Playing Factorio"));
        assert_eq!(parsed[1].line().as_deref(), Some("Streaming Speedrun"));
        assert_eq!(parsed[1].heading().as_deref(), Some("Streaming Twitch"));
        assert_eq!(parsed[1].url.as_deref(), Some("https://twitch.tv/ann"));
        assert_eq!(parsed[2].url, None, "only web addresses open");
        let found = json!({"id": "42", "icon": "abc"});
        assert_eq!(
            icon_url(&found).as_deref(),
            Some("https://cdn.discordapp.com/app-icons/42/abc.png?size=160")
        );
    }

    #[test]
    fn leaves_out_pictures_with_odd_keys() {
        for key in [
            "mp:../x",
            "spotify:a/b",
            "nothing:1",
            "../1",
            "mp:a b",
            "1?x",
        ] {
            assert_eq!(image(Some("42"), key), None, "{key}");
        }
        assert_eq!(image(None, "777"), None, "an asset needs its application");
        assert!(outside_picture("https://i.ytimg.com/vi/abc/hqdefault.jpg"));
        assert!(!outside_picture("https://i.ytimg.com.example.com/vi/abc"));
    }

    #[test]
    fn own_activities_gather_this_session_and_another_as_discords_client_does() {
        let status = json!({"type": 4, "name": "Custom Status", "state": "tea"});
        let song = json!({"type": 2, "name": "Spotify", "details": "Here"});
        let sessions = json!([
            {"session_id": "mine", "active": true, "activities": [{"type": 0, "name": "Mine"}]},
            {"session_id": "idle", "active": false, "activities": [{"type": 0, "name": "Idle"}]},
            {"session_id": "all", "active": true, "activities": [
                status,
                {"type": 2, "name": "Spotify", "details": "Elsewhere"},
                {"type": 0, "name": "Plain"},
                {"type": 0, "name": "Rich", "details": "In a match"},
            ]},
            {"session_id": "phone", "active": true, "activities": [{"type": 3, "name": "Later"}]},
        ]);
        let other = elsewhere(&sessions, Some("mine"));
        assert_eq!(other.len(), 4, "the first other active session");
        let shown = own(&[status.clone(), song], &other);
        let names: Vec<_> = shown
            .iter()
            .map(|activity| (activity.kind, activity.details.as_deref()))
            .collect();
        assert_eq!(
            names,
            [
                (Kind::Playing, Some("In a match")),
                (Kind::Listening, Some("Here")),
            ],
            "games first, the richest game alone, and this session's song over another's"
        );
        assert_eq!(elsewhere(&json!(null), None), Vec::<Value>::new());
        assert!(
            own(&[status], &[]).is_empty(),
            "a custom status has its own line"
        );
    }
}
