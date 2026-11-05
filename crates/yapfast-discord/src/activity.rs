//! What someone is doing, from their presence's activities, as their
//! profile shows it: a game, music, a stream or a Hang status, with its
//! pictures and times. A custom status shows on its own line, as
//! [`crate::member_list::activity`] writes it, so it isn't one of these.

use std::sync::Arc;

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
    pub application_id: Option<Box<str>>,
    /// When it started and when it ends, in Unix milliseconds.
    pub start: Option<u64>,
    pub end: Option<u64>,
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
}

impl Activity {
    /// The line Discord shows under a name for it, as for a member with no
    /// custom status, such as "Playing Factorio"; none for a Hang status.
    pub fn line(&self) -> Option<String> {
        Some(match self.kind {
            Kind::Playing => format!("Playing {}", self.name),
            Kind::Streaming => format!(
                "Streaming {}",
                self.details.as_deref().unwrap_or(&self.name)
            ),
            Kind::Listening => format!("Listening to {}", self.name),
            Kind::Watching => format!("Watching {}", self.name),
            Kind::Competing => format!("Competing in {}", self.name),
            Kind::Hang => return None,
        })
    }

    /// Spotify's song, as Discord's card shows it: titled by the song, then
    /// by whom and on which album.
    pub fn spotify(&self) -> bool {
        self.kind == Kind::Listening && &*self.name == "Spotify" && self.application_id.is_none()
    }

    /// What Discord heads its card with, such as "Listening to Spotify".
    pub fn heading(&self) -> String {
        match self.kind {
            Kind::Playing => "Playing".into(),
            Kind::Streaming => "Streaming".into(),
            Kind::Listening => format!("Listening to {}", self.name),
            Kind::Watching => "Watching".into(),
            Kind::Competing => "Competing in".into(),
            Kind::Hang => "Hanging out".into(),
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
        assert_eq!(game.heading(), "Playing");
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
        assert_eq!(parsed[0].heading(), "Listening to Spotify");
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
}
