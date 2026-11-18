//! The Activity panel above the member list: Discord's feed of what those
//! sharing the server do and did lately, in its order (`activity_panel`,
//! `src/ui/profile.rs:53`). Made only while the member list shows, from the
//! feed's `Arc`, the people the room may show and the members (whose
//! presence says more of what's live); its words of time say the minute,
//! so the minute's clock writes them again (`crate::clocks`).

use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_lang::{tr, trn};

use anomi_vm::kind::Doing;
use anomi_vm::people::activity::{Activity, Row};

use crate::pictures::{self, Requests, Watch};
use crate::project::rows;

/// What the panel was made from.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<Key>,
    pictures: Watch<()>,
}

#[derive(PartialEq)]
struct Key {
    feed: usize,
    people: usize,
    members: usize,
    room: Option<String>,
    minute: u64,
}

impl Key {
    fn of(state: &AppState) -> Self {
        let members = match &state.members {
            Some(Ok(members)) => Arc::as_ptr(members).addr(),
            _ => 0,
        };
        Self {
            feed: Arc::as_ptr(&state.activity_feed).cast::<()>().addr(),
            people: state
                .activity_people
                .as_ref()
                .map_or(0, |(_, people)| Arc::as_ptr(people).cast::<()>().addr()),
            members,
            room: state.selected.clone(),
            minute: yapfast_core::clock::now_ms() / 60_000,
        }
    }
}

/// Shows the panel while the member list is `wanted`.
pub(super) fn show(
    state: &AppState,
    wanted: bool,
    activity: &mut Activity,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    activity.hidden = state.activity_cards_hidden;
    if !wanted {
        if shown.key.is_some() || !activity.rows.is_empty() {
            activity.rows.clear();
            *shown = Shown::default();
        }
        return;
    }
    let key = Key::of(state);
    let moved = !shown.pictures.moved(state).is_empty();
    if !moved && shown.key.as_ref() == Some(&key) {
        return;
    }
    let mut watch = std::mem::take(&mut shown.pictures);
    let now_ms = yapfast_core::clock::now_ms();
    activity.rows = pictures::noting(state, &mut watch, || rows_of(state, now_ms));
    shown.pictures = watch;
    shown.key = Some(key);
    if !activity.rows.is_empty() {
        want(state, requests);
    }
}

/// What the panel shows in the open channel: the feed, in its order, of
/// those `state.activity_people` says it may show. Discord shows the panel
/// in a server's channels, not its forums; between two channels of a
/// server the last answer shows until this one's comes.
#[cfg(feature = "discord")]
pub(in crate::project) fn rows_of(state: &AppState, now_ms: u64) -> Vec<Row> {
    use yapfast_core::chat::{ActivityKind, ChatMember, FeedEntry, Presence};
    use yapfast_core::discord::parse_room;
    use yapfast_core::yapfast_discord::activity::small_picture;
    use yapfast_core::yapfast_discord::activity_feed::Kind;
    let Some(room) = state
        .selected_room()
        .filter(|room| !room.direct && !room.forum)
    else {
        return Vec::new();
    };
    let Some((asked, people)) = &state.activity_people else {
        return Vec::new();
    };
    let server = |room| parse_room(room).map(|(server, _)| server);
    if server(asked) != server(&room.id) {
        return Vec::new();
    }
    let members: &[ChatMember] = match &state.members {
        Some(Ok(members)) => members,
        _ => &[],
    };
    // What their presence says of it, which is more than the feed does.
    fn doing<'a>(
        member: &'a ChatMember,
        entry: &FeedEntry,
    ) -> Option<&'a yapfast_core::chat::Activity> {
        member.activities.iter().find(|activity| match entry.kind {
            Kind::Game | Kind::Launched => {
                activity.kind == ActivityKind::Playing && *activity.name == *entry.title
            }
            Kind::Listened => activity.kind == ActivityKind::Listening,
            Kind::Watched => activity.kind == ActivityKind::Watching,
            Kind::TopGame | Kind::TopArtist => false,
        })
    }
    state
        .activity_feed
        .iter()
        .filter_map(|entry| {
            let person = people.iter().find(|person| person.id == entry.author)?;
            let live = entry.live_at(now_ms);
            let member = live
                .then(|| members.iter().find(|member| *member.id == *entry.author))
                .flatten();
            // Discord's client leaves out what someone offline "is" doing.
            if member.is_some_and(|member| member.presence == Some(Presence::Offline)) {
                return None;
            }
            let activity = member.and_then(|member| doing(member, entry));
            let since = match (entry.hours, entry.tracks) {
                (Some(hours), _) if entry.kind == Kind::TopGame => tr!("Most played: {}h", hours),
                (_, Some(tracks)) if entry.kind == Kind::TopArtist => {
                    trn!("{n} track this week", "{n} tracks this week", tracks)
                }
                _ if live => running_for(
                    activity
                        .and_then(|activity| activity.start)
                        .unwrap_or(entry.at_ms),
                    now_ms,
                ),
                _ => finished_ago(entry.at_ms, now_ms),
            };
            let picture = activity
                .and_then(|activity| activity.large_image.as_deref())
                .or(entry.picture.as_deref())
                .unwrap_or_default();
            let avatar = Some(&*person.avatar).filter(|avatar| !avatar.is_empty());
            Some(Row {
                id: person.id.to_string(),
                name: person.name.to_string(),
                avatar: rows::avatar(state, &person.name, &person.name, avatar),
                doing: match entry.kind {
                    Kind::Game | Kind::Launched => Doing::Playing,
                    Kind::Listened => Doing::Listening,
                    Kind::Watched => Doing::Watching,
                    Kind::TopGame | Kind::TopArtist => Doing::Competing,
                },
                live,
                game: entry.title.to_string(),
                since,
                thumb: rows::expected(state, Some(&small_picture(picture))),
            })
        })
        .collect()
}

#[cfg(not(feature = "discord"))]
pub(in crate::project) fn rows_of(_state: &AppState, _now_ms: u64) -> Vec<Row> {
    Vec::new()
}

/// Asks for the avatars and pictures of the panel's people.
#[cfg(feature = "discord")]
fn want(state: &AppState, requests: &mut Requests) {
    use yapfast_core::yapfast_discord::activity::small_picture;
    let Some((_, people)) = &state.activity_people else {
        return;
    };
    requests.want(
        state,
        people
            .iter()
            .map(|person| &*person.avatar)
            .filter(|avatar| !avatar.is_empty()),
    );
    let pictures: Vec<String> = state
        .activity_feed
        .iter()
        .filter_map(|entry| entry.picture.as_deref())
        .map(|picture| small_picture(picture).into_owned())
        .collect();
    requests.want(state, pictures.iter().map(String::as_str));
}

#[cfg(not(feature = "discord"))]
fn want(_state: &AppState, _requests: &mut Requests) {}

/// A running time as the panel words it: "for 12 min".
fn running_for(started_ms: u64, now_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(started_ms) / 60_000;
    match minutes {
        0 => tr!("just started").to_owned(),
        1..=59 => tr!("for {} min", minutes),
        60..=1439 => tr!("for {} hr", minutes / 60),
        _ => tr!("for {} d", minutes / 1440),
    }
}

/// How long ago something finished, in Discord's steps: "3h ago".
pub(in crate::project) fn finished_ago(at_ms: u64, now_ms: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        0..MINUTE => tr!("{}s ago", seconds),
        MINUTE..HOUR => tr!("{}m ago", seconds / MINUTE),
        _ if seconds < 12 * HOUR => tr!("{}h ago", seconds / HOUR),
        _ if seconds < 9 * DAY => tr!("{}d ago", (seconds / DAY).max(1)),
        _ if seconds < 28 * DAY => tr!("{}w ago", seconds / (7 * DAY)),
        _ => tr!("{}mo ago", (seconds / (30 * DAY)).max(1)),
    }
}

/// Whether the panel shows times the minute's clock says again.
pub(crate) fn timed(activity: &Activity) -> bool {
    activity.wants_clock() && activity.showing() > 0
}
