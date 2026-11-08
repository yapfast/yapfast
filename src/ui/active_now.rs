//! Discord's Active Now beside the friends list: friends together in a
//! server's voice channel, or playing the same game, a card each, with how
//! long they've been at it. See `ActiveNowColumn` in ui/active_now.slint.

use std::collections::HashMap;

use yapfast_core::chat::ChatRoom;
#[cfg(feature = "discord")]
use yapfast_core::chat::{Friend, Presence};

use super::*;

/// When ActivityClock's seconds start, as in `profile`.
const ACTIVITY_EPOCH: u64 = 1_577_836_800;

/// The most faces a row shows before "+N".
#[cfg(feature = "discord")]
const FACES: usize = 3;

/// Discord's Unix milliseconds on ActivityClock; 0 when unknown.
#[cfg(feature = "discord")]
fn clock(ms: Option<u64>) -> i32 {
    ms.map_or(0, |ms| {
        i32::try_from((ms / 1000).saturating_sub(ACTIVITY_EPOCH)).unwrap_or(0)
    })
}

#[cfg(feature = "discord")]
fn face(id: &str, name: &str, avatar: Option<&str>) -> ActiveFace {
    ActiveFace {
        initial: initial(name),
        tint: tint(id),
        picture_url: avatar.unwrap_or_default().into(),
    }
}

/// "A", "A and B", "A, B, and 1 other", "A, B, and 4 others".
#[cfg(any(feature = "discord", test))]
fn names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [one, two] => format!("{one} and {two}"),
        [one, two, rest @ ..] => {
            let others = if rest.len() == 1 { "other" } else { "others" };
            format!("{one}, {two}, and {} {others}", rest.len())
        }
    }
}

/// A face row of people, and how many more there are.
#[cfg(feature = "discord")]
fn faces(people: impl Iterator<Item = ActiveFace>) -> (ModelRc<ActiveFace>, i32) {
    let all: Vec<_> = people.collect();
    let more = i32::try_from(all.len().saturating_sub(FACES)).unwrap_or(i32::MAX);
    let shown: Vec<_> = all.into_iter().take(FACES).collect();
    (ModelRc::new(VecModel::from(shown)), more)
}

#[cfg(feature = "discord")]
mod playing {
    use yapfast_core::chat::{Activity, ActivityKind};

    /// What a friend is doing that Active Now tells of: a game first, then
    /// a stream, music or a show; none for only a custom status or a Hang.
    pub(super) fn main(activities: &[Activity]) -> Option<&Activity> {
        let rank = |activity: &&Activity| match activity.kind {
            ActivityKind::Playing => 0,
            ActivityKind::Streaming => 1,
            ActivityKind::Competing => 2,
            ActivityKind::Watching => 3,
            ActivityKind::Listening => 4,
            ActivityKind::Hang => 5,
        };
        activities
            .iter()
            .filter(|activity| activity.kind != ActivityKind::Hang)
            .min_by_key(rank)
    }

    /// How a card says it: "Dota 2", "Listening to Spotify".
    pub(super) fn doing(activity: &Activity) -> String {
        match activity.kind {
            ActivityKind::Playing => activity.name.to_string(),
            _ => activity.line().unwrap_or_else(|| activity.name.to_string()),
        }
    }

    /// The activity's own row, when it says more than its name: Rich
    /// Presence's details and state, such as Medal's "Clipping Dota 2".
    pub(super) fn has_details(activity: &Activity) -> bool {
        activity.details.is_some() || activity.state.is_some()
    }
}

impl Adapter {
    pub(super) fn active_now_cards(
        &self,
        rooms_by_id: &HashMap<&str, &ChatRoom>,
    ) -> Vec<ActiveNowCard> {
        #[cfg(not(feature = "discord"))]
        {
            let _ = rooms_by_id;
            Vec::new()
        }
        #[cfg(feature = "discord")]
        {
            active_now(&self.app.state, rooms_by_id)
        }
    }
}

#[cfg(feature = "discord")]
fn active_now(state: &AppState, rooms_by_id: &HashMap<&str, &ChatRoom>) -> Vec<ActiveNowCard> {
    let around: Vec<&Friend> = state
        .friends
        .iter()
        .filter(|friend| !matches!(friend.presence, None | Some(Presence::Offline)))
        .collect();
    let voice = super::refresh::voice_places(state, rooms_by_id);
    // Friends together in one server's voice channel share a card.
    let mut in_room: Vec<(&str, Vec<&Friend>)> = Vec::new();
    let mut alone: Vec<&Friend> = Vec::new();
    for friend in &around {
        match voice.get(friend.id.as_str()) {
            Some(place) => match in_room.iter_mut().find(|(room, _)| *room == place.room) {
                Some((_, friends)) => friends.push(friend),
                None => in_room.push((place.room, vec![friend])),
            },
            None => alone.push(friend),
        }
    }
    let mut cards = Vec::new();
    for (room_id, friends) in &in_room {
        let Some(room) = rooms_by_id.get(room_id) else {
            continue;
        };
        let place = &voice[friends[0].id.as_str()];
        let server = state
            .servers
            .iter()
            .find(|server| server.channels.iter().any(|channel| channel == room_id));
        // What most of them play, if any of them do.
        let mut games: Vec<(&str, Vec<&yapfast_core::chat::Activity>)> = Vec::new();
        for friend in friends {
            if let Some(activity) = playing::main(&friend.activities) {
                match games.iter_mut().find(|(name, _)| **name == *activity.name) {
                    Some((_, activities)) => activities.push(activity),
                    None => games.push((&activity.name, vec![activity])),
                }
            }
        }
        games.sort_by_key(|(_, activities)| std::cmp::Reverse(activities.len()));
        let game = games.first();
        let (people, more) = faces(
            room.people
                .iter()
                .map(|person| face(&person.id, &person.name, person.avatar.as_deref())),
        );
        let mut rows = vec![ActiveNowRow {
            kind: ActiveRowKind::Voice,
            title: place.server.into(),
            detail: place.channel.into(),
            icon_url: server
                .and_then(|server| server.icon.as_deref())
                .unwrap_or_default()
                .into(),
            initial: initial(place.server),
            tint: tint(place.server),
            faces: people,
            more,
            room: (*room_id).into(),
            ..Default::default()
        }];
        if let Some((_, activities)) = game {
            rows.extend(
                activities
                    .iter()
                    .filter(|activity| playing::has_details(activity))
                    .take(1)
                    .map(|activity| detail_row(activity)),
            );
        }
        let first = friends[0];
        cards.push(ActiveNowCard {
            id: (*room_id).into(),
            title: names(
                &friends
                    .iter()
                    .map(|friend| friend.name.as_str())
                    .collect::<Vec<_>>(),
            )
            .into(),
            doing: game
                .map_or("In a Voice Channel".to_owned(), |(_, activities)| {
                    playing::doing(activities[0])
                })
                .into(),
            start: game.map_or(0, |(_, activities)| {
                activities
                    .iter()
                    .map(|activity| clock(activity.start))
                    .filter(|start| *start > 0)
                    .min()
                    .unwrap_or(0)
            }),
            face: face(&first.id, &first.name, first.avatar.as_deref()),
            presence: super::profile::presence_name(first.presence).into(),
            icon_url: game
                .and_then(|(_, activities)| activities[0].large_image.as_deref())
                .unwrap_or_default()
                .into(),
            rows: ModelRc::new(VecModel::from(rows)),
            person: first.id.as_str().into(),
        });
    }
    // The others, by what they're doing: a card for each game.
    let mut by_game: Vec<(String, Vec<(&Friend, &yapfast_core::chat::Activity)>)> = Vec::new();
    for friend in alone {
        let Some(activity) = playing::main(&friend.activities) else {
            continue;
        };
        let doing = playing::doing(activity);
        match by_game.iter_mut().find(|(each, _)| *each == doing) {
            Some((_, friends)) => friends.push((friend, activity)),
            None => by_game.push((doing, vec![(friend, activity)])),
        }
    }
    for (doing, friends) in by_game {
        let (first, activity) = friends[0];
        let mut rows: Vec<ActiveNowRow> = friends
            .iter()
            .filter(|(_, activity)| playing::has_details(activity))
            .take(2)
            .map(|(_, activity)| detail_row(activity))
            .collect();
        if friends.len() > 1 {
            let (people, more) = faces(
                friends
                    .iter()
                    .map(|(friend, _)| face(&friend.id, &friend.name, friend.avatar.as_deref())),
            );
            let count = friends.len();
            rows.push(ActiveNowRow {
                kind: ActiveRowKind::Game,
                title: doing.as_str().into(),
                detail: format!("{count} People").into(),
                icon_url: activity.large_image.as_deref().unwrap_or_default().into(),
                initial: initial(&doing),
                tint: tint(&doing),
                faces: people,
                more,
                ..Default::default()
            });
        }
        cards.push(ActiveNowCard {
            id: format!("game:{doing}").into(),
            title: names(
                &friends
                    .iter()
                    .map(|(friend, _)| friend.name.as_str())
                    .collect::<Vec<_>>(),
            )
            .into(),
            start: friends
                .iter()
                .map(|(_, activity)| clock(activity.start))
                .filter(|start| *start > 0)
                .min()
                .unwrap_or(0),
            doing: doing.into(),
            face: face(&first.id, &first.name, first.avatar.as_deref()),
            presence: super::profile::presence_name(first.presence).into(),
            icon_url: activity.large_image.as_deref().unwrap_or_default().into(),
            rows: ModelRc::new(VecModel::from(rows)),
            person: first.id.as_str().into(),
        });
    }
    cards
}

#[cfg(feature = "discord")]
fn detail_row(activity: &yapfast_core::chat::Activity) -> ActiveNowRow {
    let title = activity.details.as_deref().unwrap_or(&activity.name);
    ActiveNowRow {
        kind: ActiveRowKind::Detail,
        title: title.into(),
        detail: activity
            .state
            .as_deref()
            .map_or_else(|| format!("with {}", activity.name), str::to_owned)
            .into(),
        icon_url: activity
            .small_image
            .as_deref()
            .or(activity.large_image.as_deref())
            .unwrap_or_default()
            .into(),
        initial: initial(&activity.name),
        tint: tint(&activity.name),
        start: clock(activity.start),
        ..Default::default()
    }
}

impl Adapter {
    /// Keeps ActivityClock current, once a second, while Active Now shows a
    /// timer, and stops when it doesn't.
    pub(super) fn tick_active_now(&self, window: &YapfastWindow, timed: bool) {
        if !timed {
            self.active_now_clock.stop();
            return;
        }
        if self.active_now_clock.running() {
            return;
        }
        let now = || {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            i32::try_from(seconds.saturating_sub(ACTIVITY_EPOCH)).unwrap_or(i32::MAX)
        };
        window.global::<ActivityClock>().set_now(now());
        let window = window.as_weak();
        self.active_now_clock.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(1),
            move || {
                if let Some(window) = window.upgrade() {
                    window.global::<ActivityClock>().set_now(now());
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_shorten_as_discords_active_now_does() {
        assert_eq!(names(&["A"]), "A");
        assert_eq!(names(&["A", "B"]), "A and B");
        assert_eq!(names(&["A", "B", "C"]), "A, B, and 1 other");
        assert_eq!(names(&["A", "B", "C", "D", "E"]), "A, B, and 3 others");
    }
}
