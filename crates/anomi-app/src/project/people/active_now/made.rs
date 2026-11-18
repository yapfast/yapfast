//! Active Now's cards from the friends and who's in voice, as
//! `src/ui/active_now.rs` makes them (`cards::active_now`).

use std::cmp::Reverse;
use std::collections::HashMap;

use yapfast_core::application::AppState;
use yapfast_core::chat::{Activity, ActivityKind, ChatRoom, Friend};
use yapfast_lang::{tr, trn};

use anomi_vm::kind::Avatar;
use anomi_vm::people::active_now::{Card, Kind, Row};

use super::super::lines::seconds;
use crate::project::rows;
use crate::project::sidebar::people::{VoicePlace, voice_places};

const FACES: usize = 3;
/// Xbox's application, whose games group by name.
const XBOX: &str = "438122941302046720";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Group<'a> {
    Voice(&'a str),
    Party(&'a str),
    Person(&'a str),
}

type Order = (Reverse<bool>, Reverse<bool>, Reverse<bool>, bool, String);

struct Doing<'a> {
    activity: &'a Activity,
    friends: Vec<&'a Friend>,
}

pub(super) fn active_now(state: &AppState, rooms: &HashMap<&str, &ChatRoom>) -> Vec<Card> {
    let voice = voice_places(state, rooms);
    let mut groups: Vec<Vec<&Friend>> = Vec::new();
    let mut index: HashMap<Group, usize> = HashMap::new();
    for friend in &state.friends {
        let group = match voice.get(friend.id.as_str()) {
            Some(place) => Group::Voice(place.room),
            None => match friend
                .activities
                .first()
                .and_then(|activity| activity.party_id.as_deref())
            {
                Some(party) => Group::Party(party),
                None => Group::Person(&friend.id),
            },
        };
        match index.get(&group) {
            Some(&at) => groups[at].push(friend),
            None => {
                index.insert(group, groups.len());
                groups.push(vec![friend]);
            }
        }
    }
    let mut cards: Vec<(Order, Card)> = groups
        .iter()
        .filter_map(|friends| {
            let room = voice
                .get(friends[0].id.as_str())
                .and_then(|place| Some((place, *rooms.get(place.room)?)));
            group_card(state, room, friends)
        })
        .collect();
    cards.sort_by(|(one, _), (other, _)| one.cmp(other));
    cards.into_iter().map(|(_, card)| card).collect()
}

fn group_card(
    state: &AppState,
    room: Option<(&VoicePlace, &ChatRoom)>,
    friends: &[&Friend],
) -> Option<(Order, Card)> {
    let mut doing: Vec<(String, Doing)> = Vec::new();
    let mut spotify = false;
    for &friend in friends {
        let Some(activity) = friend.activities.first() else {
            continue;
        };
        let Some(application) = application(activity) else {
            continue;
        };
        spotify = activity.spotify();
        match doing.iter_mut().find(|(each, _)| *each == application) {
            Some((_, at)) => at.friends.push(friend),
            None => doing.push((
                application,
                Doing {
                    activity,
                    friends: vec![friend],
                },
            )),
        }
    }
    if room.is_none() && doing.is_empty() {
        return None;
    }
    let mut doing: Vec<Doing> = doing.into_iter().map(|(_, doing)| doing).collect();
    doing.sort_by(|one, other| {
        other
            .activity
            .start
            .unwrap_or_default()
            .cmp(&one.activity.start.unwrap_or_default())
            .then_with(|| one.activity.name.cmp(&other.activity.name))
    });
    let people = room.map_or(friends.len(), |(_, room)| {
        room.people.len().max(friends.len())
    });
    let order = (
        Reverse(people > 1),
        Reverse(room.is_some()),
        Reverse(!doing.is_empty()),
        spotify,
        friends
            .iter()
            .map(|friend| friend.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(" "),
    );
    let card = match room {
        Some((place, room)) => voice_card(state, place, room, friends, doing.first()),
        None => doing_card(state, friends, &doing),
    };
    Some((order, card))
}

/// What groups friends doing the same thing: the app, Spotify, a
/// stream's address, or an Xbox game's name.
fn application(activity: &Activity) -> Option<String> {
    if activity.spotify() {
        return Some("spotify".to_owned());
    }
    if activity.kind == ActivityKind::Streaming
        && let Some(url) = activity.url.as_deref().filter(|url| stream_link(url))
    {
        return Some(format!("url:{url}"));
    }
    match activity.application_id.as_deref()? {
        XBOX => Some(format!("name:{}", activity.name)),
        id => Some(id.to_owned()),
    }
}

fn stream_link(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.strip_prefix("twitch.tv/")
        .is_some_and(|name| !name.is_empty())
        || rest.starts_with("youtube.com/watch?v=")
}

fn voice_card(
    state: &AppState,
    place: &VoicePlace,
    room: &ChatRoom,
    friends: &[&Friend],
    doing: Option<&Doing>,
) -> Card {
    let server = state
        .servers
        .iter()
        .find(|server| server.channels.iter().any(|channel| channel == place.room));
    let (faces, more) = faces(
        room.people
            .iter()
            .map(|person| face(state, &person.id, &person.name, person.avatar.as_deref())),
    );
    let mut rows = vec![Row {
        kind: Kind::Voice,
        title: place.server.to_owned(),
        detail: place.channel.to_owned(),
        icon: rows::avatar(
            state,
            place.server,
            place.server,
            server.and_then(|server| server.icon.as_deref()),
        ),
        faces,
        more,
        room: place.room.to_owned(),
        ..Row::default()
    }];
    let activity = doing.map(|doing| doing.activity);
    rows.extend(
        activity
            .filter(|activity| has_details(activity))
            .map(|activity| detail_row(state, activity)),
    );
    card(
        state,
        place.room.to_owned(),
        friends,
        activity.map_or_else(|| tr!("In a Voice Channel").to_owned(), self::doing),
        activity.map_or(0, |activity| seconds(activity.start)),
        activity.and_then(|activity| activity.large_image.as_deref()),
        rows,
    )
}

fn doing_card(state: &AppState, friends: &[&Friend], doing: &[Doing]) -> Card {
    let first = &doing[0];
    let activity = first.activity;
    let what = self::doing(activity);
    let mut rows: Vec<Row> = doing
        .iter()
        .map(|doing| doing.activity)
        .filter(|activity| has_details(activity))
        .take(2)
        .map(|activity| detail_row(state, activity))
        .collect();
    if first.friends.len() > 1 {
        let (faces, more) = faces(
            first
                .friends
                .iter()
                .map(|friend| face(state, &friend.id, &friend.name, friend.avatar.as_deref())),
        );
        rows.push(Row {
            kind: Kind::Game,
            title: what.clone(),
            detail: trn!("{n} Person", "{n} People", first.friends.len()),
            icon: rows::avatar(state, &what, &what, activity.large_image.as_deref()),
            faces,
            more,
            ..Row::default()
        });
    }
    card(
        state,
        format!("person:{}", friends[0].id),
        friends,
        what,
        seconds(activity.start),
        activity.large_image.as_deref(),
        rows,
    )
}

fn card(
    state: &AppState,
    id: String,
    friends: &[&Friend],
    doing: String,
    start: i64,
    icon: Option<&str>,
    rows: Vec<Row>,
) -> Card {
    let first = friends[0];
    let names: Vec<&str> = friends.iter().map(|friend| friend.name.as_str()).collect();
    Card {
        id,
        title: super::names(&names),
        doing,
        start,
        face: face(state, &first.id, &first.name, first.avatar.as_deref()),
        icon: rows::expected(state, icon),
        rows,
        person: first.id.clone(),
    }
}

/// What they're doing, as the card's line says it.
fn doing(activity: &Activity) -> String {
    match activity.kind {
        ActivityKind::Playing => activity.name.to_string(),
        _ => activity.line().unwrap_or_else(|| activity.name.to_string()),
    }
}

fn has_details(activity: &Activity) -> bool {
    activity.details.is_some() || activity.state.is_some()
}

fn detail_row(state: &AppState, activity: &Activity) -> Row {
    let title = activity.details.as_deref().unwrap_or(&activity.name);
    Row {
        kind: Kind::Detail,
        title: title.to_owned(),
        detail: activity
            .state
            .as_deref()
            .map_or_else(|| tr!("with {}", activity.name), str::to_owned),
        icon: rows::avatar(
            state,
            &activity.name,
            &activity.name,
            activity
                .small_image
                .as_deref()
                .or(activity.large_image.as_deref()),
        ),
        start: seconds(activity.start),
        ..Row::default()
    }
}

fn face(state: &AppState, id: &str, name: &str, avatar: Option<&str>) -> Avatar {
    rows::avatar(state, name, id, avatar)
}

fn faces(people: impl Iterator<Item = Avatar>) -> (Vec<Avatar>, u32) {
    let all: Vec<Avatar> = people.collect();
    let more = u32::try_from(all.len().saturating_sub(FACES)).unwrap_or(u32::MAX);
    (all.into_iter().take(FACES).collect(), more)
}
