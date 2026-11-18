//! People: a profile's status line and activity cards, one's own as one's
//! sessions say them; a server tag's card; the activity panel.

use std::sync::Arc;

use serde_json::json;
use yapfast_core::application::{AppState, ProfileView};
use yapfast_core::chat::{
    ChatRoom, CustomStatus, FeedEntry, FeedPerson, Presence, Profile, TagServer,
};
use yapfast_core::discord::room_id;
use yapfast_core::yapfast_discord::activity::activities;
use yapfast_core::yapfast_discord::activity_feed::Kind;

use anomi_vm::kind::Expected;

use super::preview;
use crate::project::people::activity::{finished_ago, rows_of as panel};
use crate::project::people::card::card;
use crate::project::people::cards::{ShownTag, tag_card};
use crate::project::people::lines;
use crate::project::people::tests::member;

fn view(id: &str, activity: Option<&str>) -> ProfileView {
    ProfileView {
        profile: Profile {
            id: id.into(),
            name: "Sam".into(),
            activity: activity.map(str::to_owned),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn this_accounts_own_profile_shows_what_its_sessions_do() {
    let mut core = preview();
    core.state.discord_user = Some("42".into());
    core.state.discord_activities =
        activities(&json!([{"type": 0, "name": "Chess", "details": "In a match"}]));
    // No presence comes for oneself; one's own profile shows what one's
    // sessions say instead, and nobody else's does.
    let own = card(&core, &view("42", None), false);
    assert_eq!(own.activities.len(), 1);
    assert_eq!(own.activities[0].details, "In a match");
    assert!(card(&core, &view("7", None), false).activities.is_empty());
}

#[test]
fn ones_own_profile_says_the_custom_status_set_this_session() {
    let mut core = preview();
    core.state.discord_user = Some("42".into());
    core.state.online = true;
    core.state.custom_status = Some(CustomStatus {
        text: "Now".into(),
        emoji: Some("🙂".into()),
        ..Default::default()
    });
    let said = |core: &yapfast_core::application::Application, id| {
        card(core, &view(id, Some("Before")), false)
            .thought
            .expect("a status line")
            .text
    };
    assert_eq!(
        said(&core, "42"),
        "🙂 Now",
        "the newest status, not the list's"
    );
    assert_eq!(said(&core, "7"), "Before", "someone else's stays theirs");
    // Whatever the presence, as Discord's client shows the setting.
    core.state.online = false;
    assert_eq!(said(&core, "42"), "🙂 Now", "offline, still the setting");
}

#[test]
fn statuses_show_their_custom_emoji_as_pictures() {
    let state = AppState::default();
    let status = lines::status(&state, &Default::default(), Some("<:pog:5>  back\nsoon"));
    assert_eq!(status.text, "back soon");
    assert_eq!(status.emoji, Expected::Waiting, "its picture, asked for");
    let status = lines::status(&state, &Default::default(), Some("☕ brb"));
    assert_eq!(
        (status.text.as_str(), &status.emoji),
        ("☕ brb", &Expected::None)
    );
    // The account's own status, its server emoji a picture.
    let mut core = preview();
    core.state.discord_user = Some("42".into());
    core.state.custom_status = Some(CustomStatus {
        text: "Shipping it".into(),
        emoji: Some("pog".into()),
        emoji_id: Some("5".into()),
        expires_at_ms: None,
    });
    let own = card(&core, &view("42", None), false)
        .thought
        .expect("its line");
    assert_eq!(own.text, "Shipping it");
    assert_eq!(own.emoji, Expected::Waiting);
}

#[test]
fn a_profile_shows_activities_as_discord_cards() {
    let listed = activities(&json!([
        {"type": 2, "name": "Spotify", "details": "Weightless", "state": "Marconi Union",
         "sync_id": "6kkwzB6hXLIONkEk9JciA6",
         "timestamps": {"start": 1_700_000_000_000_u64, "end": 1_700_000_480_000_u64},
         "assets": {"large_image": "spotify:abc", "large_text": "Distance"}},
        {"type": 0, "name": "Factorio", "state": "In a party", "party": {"size": [2, 4]}},
        {"type": 2, "name": "Spotify", "details": "Episode 12",
         "assets": {"large_image": "spotify:def", "large_text": "A Podcast"},
         "metadata": {"type": "episode"}},
    ]));
    let cards = lines::cards(&AppState::default(), &listed);
    let spotify = &cards[0];
    assert_eq!(spotify.heading, "LISTENING TO SPOTIFY");
    let shown = |card: &anomi_vm::people::activity::card::Card| {
        (
            card.name.text.clone(),
            card.details.clone(),
            card.state.clone(),
        )
    };
    assert_eq!(
        shown(spotify),
        (
            "Weightless".into(),
            "by Marconi Union".into(),
            "on Distance".into()
        )
    );
    assert_eq!(spotify.end - spotify.start, 480);
    assert_eq!(
        spotify.link,
        "https://open.spotify.com/track/6kkwzB6hXLIONkEk9JciA6"
    );
    assert_eq!(cards[1].state, "In a party (2 of 4)");
    // A podcast's episode has no artist, and names its show as Discord's
    // card does.
    assert_eq!(
        shown(&cards[2]),
        ("Episode 12".into(), String::new(), "on A Podcast".into())
    );
    // The line under the name goes when it only names one of them.
    assert!(lines::names_an_activity("Playing Factorio", &listed));
    assert!(!lines::names_an_activity("Playing Chess", &listed));
}

#[test]
fn a_server_tags_card_fills_in_as_discord_answers() {
    let mut state = AppState::default();
    let tag = ShownTag {
        server: "discord:10".into(),
        text: "UFO".into(),
        badge: Expected::None,
    };
    // Before Discord answers, the tag shows alone, its letter in the icon's
    // place, and the rest loads.
    let asked = tag_card(&state, &tag);
    assert_eq!((asked.name.as_str(), asked.initial.as_str()), ("", "U"));
    assert_eq!(asked.tag.text, "UFO");
    assert_eq!((asked.online.as_str(), asked.note.as_str()), ("", ""));
    assert!(asked.loading);
    assert!(!asked.joined, "no way in to a server this account isn't in");
    let answer = |result| Some(("discord:10".to_owned(), result));
    state.tag_server = answer(Ok(Some(TagServer {
        name: "Aliens".into(),
        icon: Some("https://cdn.discordapp.com/icons/10/abc.png".into()),
        description: "We come in peace".into(),
        members: Some(4644),
        online: Some(1),
        // 2026-11-05, as the server's id says.
        created_ms: 1_793_880_000_000,
    })));
    let answered = tag_card(&state, &tag);
    assert_eq!(
        (answered.name.as_str(), answered.initial.as_str()),
        ("Aliens", "A")
    );
    assert_eq!(answered.icon, Expected::Waiting, "its icon, asked for");
    assert_eq!(answered.online, "1 Online");
    assert_eq!(
        answered.members, "4,644 Members",
        "grouped as Discord writes it"
    );
    assert!(answered.established.starts_with("Est. ") && answered.established.ends_with(" 2026"));
    assert_eq!(answered.description, "We come in peace");
    assert!(!answered.loading && !answered.joined);
    // Another server's answer isn't this card's.
    state.tag_server = Some(("discord:11".into(), Ok(None)));
    assert!(tag_card(&state, &tag).loading);
    // A server that shows its profile only to its members says so, as
    // Discord's client does; one that failed to load says that.
    state.tag_server = answer(Ok(None));
    let limited = tag_card(&state, &tag);
    assert_eq!(limited.name, "Private Server");
    assert_eq!(
        limited.note,
        "The server has limited who can see this profile."
    );
    assert!(limited.limited && !limited.failed);
    state.tag_server = answer(Err("Discord is out of reach".into()));
    let failed = tag_card(&state, &tag);
    assert_eq!(failed.name, "Unknown Server");
    assert_eq!(failed.note, "Unable to load server profile");
    assert!(failed.failed && !failed.limited);
}

/// A channel of the preview's, as a room of server `server`.
fn channel(state: &AppState, server: &str, channel: &str) -> Arc<ChatRoom> {
    Arc::new(ChatRoom {
        id: room_id(server, channel),
        direct: false,
        forum: false,
        ..(*state.rooms[0]).clone()
    })
}

#[test]
fn the_activity_panel_shows_discords_feed_in_its_order_for_those_the_channel_may_show() {
    let hour = 60 * 60_000;
    let now = 100 * hour;
    let entry = |author: &str, kind, title: &str, live, at_ms| FeedEntry {
        author: author.into(),
        kind,
        title: title.into(),
        picture: Some("discord-app-icon:42".into()),
        live,
        expires_ms: None,
        at_ms,
        hours: None,
        tracks: None,
    };
    let person = |id: &str| FeedPerson {
        id: id.into(),
        name: format!("Member {id}").into(),
        avatar: "".into(),
    };
    let mut state = preview().state;
    let rooms = [("1", "2"), ("1", "3"), ("4", "3")].map(|(s, c)| channel(&state, s, c));
    state.rooms = rooms.to_vec();
    let room = room_id("1", "2");
    state.selected = Some(room.clone());
    state.activity_feed = vec![
        entry("7", Kind::Game, "Factorio", true, now - 3 * hour),
        entry("8", Kind::Listened, "Marconi Union", true, now - 60_000),
        entry("9", Kind::Game, "Celeste", false, now - 3 * hour),
        FeedEntry {
            hours: Some(13),
            ..entry("7", Kind::TopGame, "Factorio", false, now - 50 * hour)
        },
        FeedEntry {
            tracks: Some(31),
            ..entry(
                "8",
                Kind::TopArtist,
                "Marconi Union",
                false,
                now - 50 * hour,
            )
        },
        entry("6", Kind::Game, "Offline's", true, now - hour),
        entry("5", Kind::Game, "Elsewhere", false, now - hour),
    ]
    .into();
    state.activity_people = Some((room.clone(), ["7", "8", "9", "6"].map(person).into()));
    let playing = member("7", "7", Presence::DoNotDisturb, "");
    let playing = yapfast_core::chat::ChatMember {
        activities: activities(&json!([{"type": 0, "name": "Factorio",
            "timestamps": {"start": now - 5 * 60_000}}])),
        ..playing
    };
    state.members = Some(Ok(Arc::new(vec![
        playing,
        member("6", "6", Presence::Offline, ""),
    ])));
    let rows = panel(&state, now);
    let shown: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|row| (row.name.as_str(), row.game.as_str(), row.since.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            // Their presence says when it started, which the feed's id may not.
            ("Member 7", "Factorio", "for 5 min"),
            ("Member 8", "Marconi Union", "for 1 min"),
            ("Member 9", "Celeste", "3h ago"),
            ("Member 7", "Factorio", "Most played: 13h"),
            ("Member 8", "Marconi Union", "31 tracks this week"),
        ],
    );
    assert!(rows[0].live && rows[1].live && !rows[2].live);
    // Another channel of the server keeps them until its answer comes;
    // another server's shows none, and nothing without an answer.
    state.selected = Some(room_id("1", "3"));
    assert_eq!(panel(&state, now).len(), 5);
    state.selected = Some(room_id("4", "3"));
    assert!(panel(&state, now).is_empty());
    state.selected = Some(room);
    state.activity_people = None;
    assert!(panel(&state, now).is_empty());
}

#[test]
fn a_finished_activity_says_how_long_ago_in_discords_steps() {
    let (minute, hour, day) = (60_000, 3_600_000, 86_400_000);
    let at = 100 * day;
    for (ago, words) in [
        (5_000, "5s ago"),
        (12 * minute, "12m ago"),
        (3 * hour, "3h ago"),
        (13 * hour, "1d ago"),
        (8 * day, "8d ago"),
        (15 * day, "2w ago"),
        (60 * day, "2mo ago"),
    ] {
        assert_eq!(finished_ago(at - ago, at), words);
    }
}
