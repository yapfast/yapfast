//! Discord's Active Now beside the friends list: friends together in a
//! server's voice channel, or playing the same game, a card each, with how
//! long they've been at it. See `ActiveNowColumn` in ui/active_now.slint.

use std::collections::HashMap;

use yapfast_core::chat::ChatRoom;

use super::*;

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
            cards::active_now(&self.app.state, rooms_by_id)
        }
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

#[cfg(feature = "discord")]
mod cards {
    use std::collections::HashMap;

    use yapfast_core::chat::{Activity, ActivityKind, ChatRoom, Friend, Presence};

    use super::super::profile::{activity_at, presence_name};
    use super::super::refresh::{VoicePlace, voice_places};
    use super::*;

    /// The most faces a row shows before "+N".
    const FACES: usize = 3;

    pub(super) fn active_now(
        state: &AppState,
        rooms_by_id: &HashMap<&str, &ChatRoom>,
    ) -> Vec<ActiveNowCard> {
        let voice = voice_places(state, rooms_by_id);
        // Friends together in one server's voice channel share a card; the
        // others share one for what they're doing.
        let mut in_room: Vec<(&str, Vec<&Friend>)> = Vec::new();
        let mut alone: Vec<&Friend> = Vec::new();
        let around = state
            .friends
            .iter()
            .filter(|friend| !matches!(friend.presence, None | Some(Presence::Offline)));
        for friend in around {
            match voice.get(friend.id.as_str()) {
                Some(place) => match in_room.iter_mut().find(|(room, _)| *room == place.room) {
                    Some((_, friends)) => friends.push(friend),
                    None => in_room.push((place.room, vec![friend])),
                },
                None => alone.push(friend),
            }
        }
        let mut cards: Vec<ActiveNowCard> = in_room
            .iter()
            .filter_map(|(room, friends)| {
                let room = rooms_by_id.get(room)?;
                Some(voice_card(
                    state,
                    room,
                    &voice[friends[0].id.as_str()],
                    friends,
                ))
            })
            .collect();
        cards.extend(game_cards(&alone));
        cards
    }

    /// Friends in one voice channel: the server, the channel and who's in
    /// it, and what most of them play.
    fn voice_card(
        state: &AppState,
        room: &ChatRoom,
        place: &VoicePlace,
        friends: &[&Friend],
    ) -> ActiveNowCard {
        let server = state
            .servers
            .iter()
            .find(|server| server.channels.iter().any(|channel| channel == place.room));
        let mut games: Vec<(&str, Vec<&Activity>)> = Vec::new();
        for activity in friends
            .iter()
            .filter_map(|friend| main_activity(&friend.activities))
        {
            match games.iter_mut().find(|(name, _)| *name == &*activity.name) {
                Some((_, activities)) => activities.push(activity),
                None => games.push((&activity.name, vec![activity])),
            }
        }
        games.sort_by_key(|(_, activities)| std::cmp::Reverse(activities.len()));
        let game = games.first().map(|(_, activities)| activities.as_slice());
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
            room: place.room.into(),
            ..Default::default()
        }];
        rows.extend(
            game.into_iter()
                .flatten()
                .copied()
                .filter(|activity| has_details(activity))
                .take(1)
                .map(detail_row),
        );
        card(
            place.room.to_owned(),
            friends,
            game.map_or("In a Voice Channel".to_owned(), |activities| {
                doing(activities[0])
            }),
            game.map_or(0, |activities| earliest_start(activities.iter().copied())),
            game.and_then(|activities| activities[0].large_image.as_deref()),
            rows,
        )
    }

    /// The friends in no voice channel, a card for each game or activity.
    fn game_cards(alone: &[&Friend]) -> Vec<ActiveNowCard> {
        let mut by_game: Vec<(String, Vec<(&Friend, &Activity)>)> = Vec::new();
        for &friend in alone {
            let Some(activity) = main_activity(&friend.activities) else {
                continue;
            };
            let what = doing(activity);
            match by_game.iter_mut().find(|(each, _)| *each == what) {
                Some((_, friends)) => friends.push((friend, activity)),
                None => by_game.push((what, vec![(friend, activity)])),
            }
        }
        by_game
            .into_iter()
            .map(|(what, playing)| {
                let activity = playing[0].1;
                let mut rows: Vec<ActiveNowRow> = playing
                    .iter()
                    .map(|(_, activity)| *activity)
                    .filter(|activity| has_details(activity))
                    .take(2)
                    .map(detail_row)
                    .collect();
                if playing.len() > 1 {
                    let (people, more) = faces(playing.iter().map(|(friend, _)| {
                        face(&friend.id, &friend.name, friend.avatar.as_deref())
                    }));
                    rows.push(ActiveNowRow {
                        kind: ActiveRowKind::Game,
                        title: what.as_str().into(),
                        detail: format!("{} People", playing.len()).into(),
                        icon_url: activity.large_image.as_deref().unwrap_or_default().into(),
                        initial: initial(&what),
                        tint: tint(&what),
                        faces: people,
                        more,
                        ..Default::default()
                    });
                }
                let friends: Vec<&Friend> = playing.iter().map(|(friend, _)| *friend).collect();
                card(
                    format!("game:{what}"),
                    &friends,
                    what,
                    earliest_start(playing.iter().map(|(_, activity)| *activity)),
                    activity.large_image.as_deref(),
                    rows,
                )
            })
            .collect()
    }

    /// A card headed by its friends' names and the first one's face.
    fn card(
        id: String,
        friends: &[&Friend],
        doing: String,
        start: i32,
        icon: Option<&str>,
        rows: Vec<ActiveNowRow>,
    ) -> ActiveNowCard {
        let first = friends[0];
        let names: Vec<&str> = friends.iter().map(|friend| friend.name.as_str()).collect();
        ActiveNowCard {
            id: id.into(),
            title: super::names(&names).into(),
            doing: doing.into(),
            start,
            face: face(&first.id, &first.name, first.avatar.as_deref()),
            presence: presence_name(first.presence).into(),
            icon_url: icon.unwrap_or_default().into(),
            rows: ModelRc::new(VecModel::from(rows)),
            person: first.id.as_str().into(),
        }
    }

    /// When the first of these started, on ActivityClock; 0 when none says.
    fn earliest_start<'a>(activities: impl Iterator<Item = &'a Activity>) -> i32 {
        activities
            .map(|activity| activity_at(activity.start))
            .filter(|start| *start > 0)
            .min()
            .unwrap_or(0)
    }

    /// What a friend is doing that Active Now tells of: a game first, then
    /// a stream, a contest, a show or music; none for only a custom status
    /// or a Hang.
    fn main_activity(activities: &[Activity]) -> Option<&Activity> {
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
    fn doing(activity: &Activity) -> String {
        match activity.kind {
            ActivityKind::Playing => activity.name.to_string(),
            _ => activity.line().unwrap_or_else(|| activity.name.to_string()),
        }
    }

    /// The activity's own row, when it says more than its name: Rich
    /// Presence's details and state, such as Medal's "Clipping Dota 2".
    fn has_details(activity: &Activity) -> bool {
        activity.details.is_some() || activity.state.is_some()
    }

    fn detail_row(activity: &Activity) -> ActiveNowRow {
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
            start: activity_at(activity.start),
            ..Default::default()
        }
    }

    fn face(id: &str, name: &str, avatar: Option<&str>) -> ActiveFace {
        ActiveFace {
            initial: initial(name),
            tint: tint(id),
            picture_url: avatar.unwrap_or_default().into(),
        }
    }

    /// A face row of people, and how many more there are.
    fn faces(people: impl Iterator<Item = ActiveFace>) -> (ModelRc<ActiveFace>, i32) {
        let all: Vec<_> = people.collect();
        let more = i32::try_from(all.len().saturating_sub(FACES)).unwrap_or(i32::MAX);
        let shown: Vec<_> = all.into_iter().take(FACES).collect();
        (ModelRc::new(VecModel::from(shown)), more)
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
