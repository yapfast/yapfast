//! The feed behind the member list's Activity panel, from
//! `GET /content-inventory/users/@me`: what people this account knows are
//! doing, marked live, or did lately. It isn't any one server's: the member
//! list shows those of its entries whose author can see the channel.

use std::sync::Arc;

use serde_json::Value;

use crate::{
    activity,
    message::{iso_ms, snowflake_ms},
};

/// A trait's `type` saying how long an entry ran, in seconds.
const DURATION_SECONDS: u64 = 2;
/// A trait's `type` saying whether an entry is going on now.
const IS_LIVE: u64 = 3;
/// A trait's `type` saying what stretch of time an entry sums up.
const AGGREGATE_RANGE: u64 = 4;
/// An [`AGGREGATE_RANGE`]'s `range` for a week.
const RANGE_WEEK: u64 = 1;
/// A trait's `type` saying how many times something happened in its range.
const AGGREGATE_COUNT: u64 = 12;

/// What an entry is about; each shows differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A game played, or being played.
    Game,
    /// Something watched, or being watched.
    Watched,
    /// The game most played this week.
    TopGame,
    /// Music listened to, or being listened to.
    Listened,
    /// The artist most listened to this week.
    TopArtist,
    /// An Activity launched in a call.
    Launched,
}

/// One card's worth of the feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub author: Arc<str>,
    pub kind: Kind,
    /// The game, what was watched, the artist or the Activity.
    pub title: Box<str>,
    /// The address of its picture; see [`crate::activity`]'s for an icon's.
    pub picture: Option<Box<str>>,
    pub live: bool,
    /// When it stops being live, in Unix milliseconds.
    pub expires_ms: Option<u64>,
    /// When it was made, in Unix milliseconds.
    pub at_ms: u64,
    /// How many hours a top game was played this week.
    pub hours: Option<u32>,
    /// How many tracks of a top artist were heard.
    pub tracks: Option<u32>,
}

impl Entry {
    /// Whether it is going on now: an entry marked live stops being so at
    /// its expiry, as Discord's client counts it.
    pub fn live_at(&self, now_ms: u64) -> bool {
        self.live && self.expires_ms.is_none_or(|expires| expires > now_ms)
    }
}

/// Someone a channel's panel may show, by the name and picture it shows them
/// by there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub id: Arc<str>,
    pub name: Box<str>,
    pub avatar: Box<str>,
}

/// The feed as Discord answered it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Feed {
    /// In Discord's ranked order.
    pub entries: Arc<[Entry]>,
    /// What the next request sends, for Discord to answer from where this
    /// one left off.
    pub refresh_token: Option<String>,
    /// How long to wait before asking again.
    pub wait_ms: Option<u64>,
    /// Set when Discord says when the feed is stale itself, with
    /// `CONTENT_INVENTORY_INBOX_STALE`; asking waits for that then.
    pub stale_after_ms: Option<u64>,
}

/// Reads Discord's answer, leaving out entries Yapfast can't show.
pub fn parse(value: &Value) -> Feed {
    let entries: Vec<Entry> = value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| parse_entry(&entry["content"]))
        .collect();
    Feed {
        entries: entries.into(),
        refresh_token: value["refresh_token"].as_str().map(str::to_owned),
        wait_ms: value["wait_ms_until_next_fetch"].as_u64(),
        stale_after_ms: value["refresh_stale_inbox_after_ms"].as_u64(),
    }
}

fn parse_entry(content: &Value) -> Option<Entry> {
    let extra = &content["extra"];
    let text = |value: &Value| -> Option<Box<str>> {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(Box::from)
    };
    let application = extra["application_id"].as_str();
    let icon = || application.and_then(activity::application_icon);
    let (kind, title, picture) = match content["content_type"].as_u64()? {
        1 => (Kind::Game, text(&extra["game_name"])?, icon()),
        2 => (
            Kind::Watched,
            text(&extra["media_title"])?,
            extra["media_assets_large_image"]
                .as_str()
                .and_then(|key| activity::asset(application, key)),
        ),
        3 => (Kind::TopGame, text(&extra["game_name"])?, icon()),
        5 => {
            let media = &extra["entries"][0]["media"];
            let artist = text(&media["artists"][0]["name"]).or_else(|| text(&media["title"]))?;
            (
                Kind::Listened,
                artist.replace("; ", ", ").into(),
                text(&media["image_url"]),
            )
        }
        6 => (
            Kind::TopArtist,
            text(&extra["artist"]["name"])?,
            text(&extra["media"]["image_url"]),
        ),
        8 => (Kind::Launched, text(&extra["activity_name"])?, icon()),
        _ => return None,
    };
    let trait_of = |wanted: u64| {
        content["traits"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|found| found["type"].as_u64() == Some(wanted))
    };
    Some(Entry {
        author: Arc::from(content["author_id"].as_str()?),
        kind,
        title,
        picture,
        live: trait_of(IS_LIVE).is_some_and(|live| live["is_live"].as_bool() == Some(true)),
        expires_ms: content["expires_at"].as_str().and_then(iso_ms),
        at_ms: snowflake_ms(content["id"].as_str()?),
        hours: trait_of(AGGREGATE_RANGE)
            .filter(|range| range["range"].as_u64() == Some(RANGE_WEEK))
            .and(trait_of(DURATION_SECONDS))
            .and_then(|duration| duration["duration_seconds"].as_u64())
            .map(|seconds| u32::try_from((seconds + 1800) / 3600).unwrap_or(u32::MAX)),
        tracks: trait_of(AGGREGATE_COUNT)
            .and_then(|count| count["count"].as_u64())
            .map(|count| u32::try_from(count).unwrap_or(u32::MAX)),
    })
}

/// Whether a server shows the panel, as Discord's client decides: what its
/// owner chose, else whether it has under 10,000 members.
pub fn shown_in(features: Option<bool>, member_count: Option<u64>) -> bool {
    features.unwrap_or_else(|| member_count.is_some_and(|count| count < 10_000))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ID: &str = "1522378878083924051";

    fn feed(content: Value) -> Feed {
        parse(&json!({"entries": [{"content": content}]}))
    }

    #[test]
    fn reads_a_live_game_and_when_it_stops_being_live() {
        let feed = parse(&json!({
            "refresh_token": "next",
            "wait_ms_until_next_fetch": 300_000,
            "entries": [{"content": {
                "id": ID, "author_id": "7", "content_type": 1,
                "expires_at": "2026-11-05T00:00:00+00:00",
                "traits": [{"type": 3, "is_live": true}],
                "extra": {"type": "played_game_extra", "game_name": "Factorio", "application_id": "42"},
            }}],
        }));
        assert_eq!(feed.refresh_token.as_deref(), Some("next"));
        assert_eq!(feed.wait_ms, Some(300_000));
        assert_eq!(feed.stale_after_ms, None);
        let entry = &feed.entries[0];
        assert_eq!(&*entry.author, "7");
        assert_eq!(entry.kind, Kind::Game);
        assert_eq!(&*entry.title, "Factorio");
        assert_eq!(
            entry.picture.as_deref(),
            activity::application_icon("42").as_deref()
        );
        assert_eq!(entry.at_ms, snowflake_ms(ID));
        let expires = entry.expires_ms.unwrap();
        assert!(entry.live_at(expires - 1));
        assert!(!entry.live_at(expires));
    }

    #[test]
    fn a_finished_entry_is_not_live() {
        let entry = feed(json!({
            "id": ID, "author_id": "7", "content_type": 1, "traits": [],
            "extra": {"game_name": "Factorio"},
        }));
        assert!(!entry.entries[0].live_at(0));
    }

    #[test]
    fn reads_music_what_was_watched_and_the_weeks_top_ones() {
        let listened = feed(json!({
            "id": ID, "author_id": "7", "content_type": 5,
            "extra": {"entries": [{"media": {
                "title": "Weightless", "image_url": "https://i.scdn.co/image/abc",
                "artists": [{"name": "Marconi Union; Friends"}],
            }}]},
        }));
        let entry = &listened.entries[0];
        assert_eq!(entry.kind, Kind::Listened);
        assert_eq!(&*entry.title, "Marconi Union, Friends");
        assert_eq!(
            entry.picture.as_deref(),
            Some("https://i.scdn.co/image/abc")
        );

        let watched = feed(json!({
            "id": ID, "author_id": "7", "content_type": 2,
            "extra": {"media_title": "A show", "application_id": "42", "media_assets_large_image": "cover"},
        }));
        assert_eq!(&*watched.entries[0].title, "A show");
        assert!(
            watched.entries[0]
                .picture
                .as_deref()
                .unwrap()
                .contains("/app-assets/42/cover")
        );

        let game = feed(json!({
            "id": ID, "author_id": "7", "content_type": 3,
            "traits": [{"type": 4, "range": 1}, {"type": 2, "duration_seconds": 45_000}],
            "extra": {"game_name": "Factorio"},
        }));
        assert_eq!(game.entries[0].hours, Some(13));

        let artist = feed(json!({
            "id": ID, "author_id": "7", "content_type": 6,
            "traits": [{"type": 12, "count": 31}],
            "extra": {"artist": {"name": "A band"}, "media": {"image_url": "https://i.scdn.co/image/x"}},
        }));
        assert_eq!(artist.entries[0].tracks, Some(31));
        assert_eq!(artist.entries[0].kind, Kind::TopArtist);
    }

    #[test]
    fn leaves_out_what_it_cannot_show() {
        let custom = feed(json!({"id": ID, "author_id": "7", "content_type": 7, "extra": {}}));
        assert!(custom.entries.is_empty());
        let nameless = feed(json!({"id": ID, "author_id": "7", "content_type": 1, "extra": {}}));
        assert!(nameless.entries.is_empty());
        let authorless = feed(json!({"id": ID, "content_type": 1, "extra": {"game_name": "A"}}));
        assert!(authorless.entries.is_empty());
    }

    #[test]
    fn a_server_shows_it_by_its_owners_choice_else_under_ten_thousand_members() {
        assert!(shown_in(Some(true), Some(50_000)));
        assert!(!shown_in(Some(false), Some(5)));
        assert!(shown_in(None, Some(9_999)));
        assert!(!shown_in(None, Some(10_000)));
        assert!(!shown_in(None, None));
    }
}
