//! A server's scheduled events: what one shows from its link, and being
//! interested in it. Discord's client resolves an event with
//! `GET /guilds/{guild}/scheduled-events/{event}`, reads this account's
//! subscriptions with `GET /users/@me/scheduled-events`, and subscribes
//! or unsubscribes with `PUT` or `DELETE` on
//! `/guilds/{guild}/scheduled-events/{event}/users/@me`; see
//! https://docs.discord.food/resources/guild-scheduled-event.

use serde_json::Value;

/// An event that hasn't started yet, and one going on now.
const SCHEDULED: u64 = 1;
const ACTIVE: u64 = 2;
/// The response of a subscription the Interested button makes.
pub const INTERESTED: u64 = 1;

/// What a scheduled event shows from its link, from
/// `GET /guilds/{guild}/scheduled-events/{event}` with its user count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub id: String,
    pub guild: String,
    pub name: String,
    /// What the event says about itself, when it says anything.
    pub description: Option<String>,
    /// When it starts and ends, as Discord timestamps them; an event
    /// somewhere else always says when it ends.
    pub starts_at: String,
    pub ends_at: Option<String>,
    /// The voice or stage channel it happens in, when it happens in one.
    pub channel: Option<String>,
    /// Where it happens, when it happens somewhere else.
    pub location: Option<String>,
    /// Who made the event, by the name Discord shows for them, when it
    /// says.
    pub creator: Option<String>,
    /// How many people are interested, when Discord counts them.
    pub interested_count: Option<u64>,
    /// The event is going on now.
    pub active: bool,
    /// The event has ended or was canceled, so interest can't change.
    pub over: bool,
    /// This account is interested in the event.
    pub interested: bool,
}

impl Preview {
    /// The preview of a scheduled event object, with whether this
    /// account is interested in it.
    pub fn of(event: &Value, interested: bool) -> Option<Self> {
        let status = event["status"].as_u64().unwrap_or_default();
        let creator = &event["creator"];
        Some(Self {
            id: event["id"].as_str()?.to_owned(),
            guild: event["guild_id"].as_str()?.to_owned(),
            name: event["name"].as_str().unwrap_or_default().to_owned(),
            description: event["description"].as_str().map(str::to_owned),
            starts_at: event["scheduled_start_time"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            ends_at: event["scheduled_end_time"].as_str().map(str::to_owned),
            channel: event["channel_id"].as_str().map(str::to_owned),
            location: event["entity_metadata"]["location"]
                .as_str()
                .map(str::to_owned),
            creator: creator["global_name"]
                .as_str()
                .or_else(|| creator["username"].as_str())
                .map(str::to_owned),
            interested_count: event["user_count"].as_u64(),
            active: status == ACTIVE,
            over: !matches!(status, SCHEDULED | ACTIVE),
            interested,
        })
    }
}

/// Whether the event `id` is among this account's subscriptions, from
/// `GET /users/@me/scheduled-events`: subscribed with an interested
/// response, as Discord's Interested button leaves it. A subscription to
/// one occurrence of a recurring event, with its exception id, is an
/// answer of its own and doesn't count for the series.
pub fn interested_in(subscriptions: &Value, id: &str) -> bool {
    subscriptions.as_array().is_some_and(|subscriptions| {
        subscriptions.iter().any(|subscription| {
            subscription["guild_scheduled_event_id"].as_str() == Some(id)
                && subscription["guild_scheduled_event_exception_id"].is_null()
                && subscription["response"].as_u64() == Some(INTERESTED)
        })
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(status: u64) -> Value {
        json!({
            "id": "1059954443799498922",
            "guild_id": "1046920999469330512",
            "name": "Alien meetup",
            "description": "Aliens only!",
            "channel_id": null,
            "creator": {"id": "10", "username": "dziurwa", "global_name": "Dziurwa"},
            "scheduled_start_time": "2023-12-31T23:00:00+00:00",
            "scheduled_end_time": "2024-01-01T23:00:00+00:00",
            "status": status,
            "entity_type": 3,
            "entity_metadata": {"location": "somewhere in ocean"},
            "user_count": 7,
        })
    }

    #[test]
    fn reads_what_an_event_shows_from_its_link() {
        let preview = Preview::of(&event(1), true).unwrap();
        assert_eq!(preview.name, "Alien meetup");
        assert_eq!(preview.location.as_deref(), Some("somewhere in ocean"));
        assert_eq!(preview.creator.as_deref(), Some("Dziurwa"));
        assert_eq!(preview.interested_count, Some(7));
        assert!(preview.interested && !preview.active && !preview.over);
    }

    #[test]
    fn an_ended_or_canceled_event_is_over() {
        assert!(Preview::of(&event(3), false).unwrap().over);
        assert!(Preview::of(&event(4), false).unwrap().over);
        let live = Preview::of(&event(2), false).unwrap();
        assert!(live.active && !live.over);
    }

    #[test]
    fn interest_reads_from_the_accounts_subscriptions() {
        let subscriptions = json!([
            {"guild_scheduled_event_id": "11", "user_id": "99", "response": 1},
            {"guild_scheduled_event_id": "12", "user_id": "99", "response": 0},
            // Interested in one occurrence of a recurring event only.
            {"guild_scheduled_event_id": "14", "guild_scheduled_event_exception_id": "15",
             "user_id": "99", "response": 1},
        ]);
        assert!(interested_in(&subscriptions, "11"));
        assert!(!interested_in(&subscriptions, "12"));
        assert!(!interested_in(&subscriptions, "14"));
        assert!(!interested_in(&subscriptions, "13"));
        assert!(!interested_in(&json!([]), "11"));
    }
}
