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

    use yapfast_core::chat::{Activity, ActivityKind, ChatRoom, Friend};

    use super::super::profile::{activity_at, presence_name};
    use super::super::refresh::{VoicePlace, voice_places};
    use super::*;

    /// The most faces a row shows before "+N".
    const FACES: usize = 3;

    /// Xbox's application, whose games Discord tells apart by name.
    const XBOX: &str = "438122941302046720";

    /// The cards, as Discord's NowPlayingViewStore makes them: friends in
    /// one voice channel share a card, then those in one game party, and
    /// everyone else has their own. Each friend counts by their first
    /// activity, and a card needs a voice channel or an activity Discord
    /// can name.
    pub(super) fn active_now(
        state: &AppState,
        rooms_by_id: &HashMap<&str, &ChatRoom>,
    ) -> Vec<ActiveNowCard> {
        let voice = voice_places(state, rooms_by_id);
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
        let mut cards: Vec<(Order, ActiveNowCard)> = groups
            .iter()
            .filter_map(|friends| {
                let room = voice
                    .get(friends[0].id.as_str())
                    .and_then(|place| Some((place, *rooms_by_id.get(place.room)?)));
                group_card(state, room, friends)
            })
            .collect();
        cards.sort_by(|(one, _), (other, _)| one.cmp(other));
        cards.into_iter().map(|(_, card)| card).collect()
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    enum Group<'a> {
        Voice(&'a str),
        Party(&'a str),
        Person(&'a str),
    }

    /// Discord's order: cards of more than one person first, then those in
    /// voice, then those doing something, Spotify's after the rest, then by
    /// name.
    type Order = (
        std::cmp::Reverse<bool>,
        std::cmp::Reverse<bool>,
        std::cmp::Reverse<bool>,
        bool,
        String,
    );

    /// One application's activity in a card, and the friends at it.
    struct Doing<'a> {
        activity: &'a Activity,
        friends: Vec<&'a Friend>,
    }

    fn group_card(
        state: &AppState,
        room: Option<(&VoicePlace, &ChatRoom)>,
        friends: &[&Friend],
    ) -> Option<(Order, ActiveNowCard)> {
        // Each application once, with the friends at it, the latest started
        // first.
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
            std::cmp::Reverse(people > 1),
            std::cmp::Reverse(room.is_some()),
            std::cmp::Reverse(!doing.is_empty()),
            spotify,
            friends
                .iter()
                .map(|friend| friend.name.to_lowercase())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let card = match room {
            Some((place, room)) => voice_card(state, place, room, friends, doing.first()),
            None => doing_card(friends, &doing),
        };
        Some((order, card))
    }

    /// What Discord's Active Now tells an activity by: Spotify, a Twitch or
    /// YouTube stream, its application, or an Xbox game's name. Anything
    /// else, a Hang or a game without an application, has no card.
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

    /// A Twitch channel or a YouTube video, as Discord's
    /// `^https?://(?:www\.)?(twitch\.tv/(.+)|youtube.com/watch\?v=)`.
    pub(super) fn stream_link(url: &str) -> bool {
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

    /// Friends in one voice channel: the server, the channel and who's in
    /// it, and what the one latest at it is doing.
    fn voice_card(
        state: &AppState,
        place: &VoicePlace,
        room: &ChatRoom,
        friends: &[&Friend],
        doing: Option<&Doing>,
    ) -> ActiveNowCard {
        let server = state
            .servers
            .iter()
            .find(|server| server.channels.iter().any(|channel| channel == place.room));
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
        let activity = doing.map(|doing| doing.activity);
        rows.extend(
            activity
                .filter(|activity| has_details(activity))
                .map(detail_row),
        );
        card(
            place.room.to_owned(),
            friends,
            activity.map_or("In a Voice Channel".to_owned(), self::doing),
            activity.map_or(0, |activity| activity_at(activity.start)),
            activity.and_then(|activity| activity.large_image.as_deref()),
            rows,
        )
    }

    /// Friends in no voice channel: one, or a game party, and what they're
    /// doing.
    fn doing_card(friends: &[&Friend], doing: &[Doing]) -> ActiveNowCard {
        let first = &doing[0];
        let activity = first.activity;
        let what = self::doing(activity);
        let mut rows: Vec<ActiveNowRow> = doing
            .iter()
            .map(|doing| doing.activity)
            .filter(|activity| has_details(activity))
            .take(2)
            .map(detail_row)
            .collect();
        if first.friends.len() > 1 {
            let (people, more) = faces(
                first
                    .friends
                    .iter()
                    .map(|friend| face(&friend.id, &friend.name, friend.avatar.as_deref())),
            );
            rows.push(ActiveNowRow {
                kind: ActiveRowKind::Game,
                title: what.as_str().into(),
                detail: format!("{} People", first.friends.len()).into(),
                icon_url: activity.large_image.as_deref().unwrap_or_default().into(),
                initial: initial(&what),
                tint: tint(&what),
                faces: people,
                more,
                ..Default::default()
            });
        }
        card(
            format!("person:{}", friends[0].id),
            friends,
            what,
            activity_at(activity.start),
            activity.large_image.as_deref(),
            rows,
        )
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

    /// Discord's NowPlayingViewStore: a card per party or per person, never
    /// per game; a Hang or a game without an application has none; parties
    /// first, Spotify last, then by name.
    #[cfg(feature = "discord")]
    #[test]
    fn active_now_groups_and_orders_cards_as_discords_does() {
        let friend =
            |id: &str, name: &str, activity: serde_json::Value| yapfast_core::chat::Friend {
                id: id.into(),
                name: name.into(),
                nickname: None,
                avatar: None,
                presence: Some(yapfast_core::chat::Presence::Online),
                activity: None,
                activities: yapfast_core::yapfast_discord::activity::activities(
                    &serde_json::json!([activity]),
                ),
                since: None,
            };
        let game = |party: Option<&str>| {
            let mut game =
                serde_json::json!({"type": 0, "name": "Dota 2", "application_id": "570"});
            if let Some(party) = party {
                game["party"] = serde_json::json!({"id": party, "size": [2, 5]});
            }
            game
        };
        let mut state = AppState::default();
        state.friends = vec![
            friend(
                "1",
                "Eve",
                serde_json::json!({"type": 2, "name": "Spotify", "details": "Song", "state": "Band", "party": {"id": "spotify:1"}}),
            ),
            friend("2", "bob", game(None)),
            friend("3", "Ann", game(None)),
            friend("4", "Cara", game(Some("p1"))),
            friend("5", "Dan", game(Some("p1"))),
            friend(
                "6",
                "Finn",
                serde_json::json!({"type": 6, "name": "Hang Status", "state": "custom", "details": "brb"}),
            ),
            friend(
                "7",
                "Gus",
                serde_json::json!({"type": 0, "name": "Solitaire"}),
            ),
        ];
        let cards = cards::active_now(&state, &HashMap::new());
        let titles: Vec<String> = cards.iter().map(|card| card.title.to_string()).collect();
        assert_eq!(titles, ["Cara and Dan", "Ann", "bob", "Eve"]);
        assert_eq!(cards[0].doing, "Dota 2");
        assert!(cards[0].rows.iter().any(|row| row.detail == "2 People"));
        assert_eq!(cards[3].doing, "Listening to Spotify");
    }

    #[cfg(feature = "discord")]
    #[test]
    fn a_stream_counts_only_from_twitch_or_youtube_as_in_discord() {
        assert!(cards::stream_link("https://www.twitch.tv/someone"));
        assert!(cards::stream_link("http://youtube.com/watch?v=abc"));
        assert!(!cards::stream_link("https://twitch.tv/"));
        assert!(!cards::stream_link("https://kick.com/someone"));
    }
}
