//! Server Boosts: the boosts this account can still give, from `GET
//! /users/@me/guilds/premium/subscription-slots`. A slot is one boost of
//! the account's Nitro; it is free to give, or already on a server, or
//! resting after a change (a slot moved too soon after it was last placed
//! has a cooldown), or canceled. Giving one is `PUT
//! /guilds/{id}/premium/subscriptions` with the slot's id.

use chrono::{DateTime, Utc};
use serde_json::Value;

/// What the account's boost slots are doing, for one server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slots {
    /// The ids of the slots that can boost a server now.
    pub free: Vec<String>,
    /// How many of the account's boosts are on the server asked about.
    pub here: u32,
    /// How many are on other servers.
    pub elsewhere: u32,
    /// How many are free but resting after being moved.
    pub resting: u32,
}

/// The slots in an answer from the slots list, as they stand at `now`, for
/// the server `guild`. A canceled slot that isn't on a server gives
/// nothing, and one that is still on a server counts where it is. A
/// cooldown that can't be read counts as resting, so a boost is not asked
/// for that Discord would refuse.
pub fn slots(value: &Value, guild: &str, now: DateTime<Utc>) -> Slots {
    let mut slots = Slots::default();
    for slot in value.as_array().into_iter().flatten() {
        let applied = slot["premium_guild_subscription"]
            .as_object()
            .filter(|boost| boost.get("ended").and_then(Value::as_bool) != Some(true));
        if let Some(boost) = applied {
            if boost.get("guild_id").and_then(Value::as_str) == Some(guild) {
                slots.here += 1;
            } else {
                slots.elsewhere += 1;
            }
            continue;
        }
        if slot["canceled"].as_bool() == Some(true) {
            continue;
        }
        let Some(id) = slot["id"].as_str() else {
            continue;
        };
        let resting = match slot["cooldown_ends_at"].as_str() {
            None => false,
            Some(ends) => DateTime::parse_from_rfc3339(ends).map_or(true, |ends| ends > now),
        };
        if resting {
            slots.resting += 1;
        } else {
            slots.free.push(id.to_owned());
        }
    }
    slots
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-20T12:00:00+00:00")
            .unwrap()
            .to_utc()
    }

    #[test]
    fn sorts_the_slots_by_what_they_are_doing() {
        let list = json!([
            {"id": "1", "canceled": false, "cooldown_ends_at": null, "premium_guild_subscription": null},
            {"id": "2", "canceled": false, "cooldown_ends_at": "2026-11-20T11:00:00+00:00", "premium_guild_subscription": null},
            {"id": "3", "canceled": false, "cooldown_ends_at": "2026-11-21T11:00:00+00:00", "premium_guild_subscription": null},
            {"id": "4", "canceled": true, "cooldown_ends_at": null, "premium_guild_subscription": null},
            {"id": "5", "canceled": false, "premium_guild_subscription": {"guild_id": "10", "ended": false}},
            {"id": "6", "canceled": false, "premium_guild_subscription": {"guild_id": "20", "ended": false}},
            {"id": "7", "canceled": false, "cooldown_ends_at": null, "premium_guild_subscription": {"guild_id": "10", "ended": true}},
            {"id": "8", "canceled": true, "premium_guild_subscription": {"guild_id": "10", "ended": false}},
            {"canceled": false},
        ]);
        let slots = slots(&list, "10", now());
        assert_eq!(
            slots.free,
            ["1", "2", "7"],
            "a cooldown that is over is free"
        );
        assert_eq!((slots.here, slots.elsewhere, slots.resting), (2, 1, 1));
    }

    #[test]
    fn a_cooldown_that_cannot_be_read_rests() {
        let list = json!([{"id": "1", "cooldown_ends_at": "soon"}]);
        let slots = slots(&list, "10", now());
        assert!(slots.free.is_empty());
        assert_eq!(slots.resting, 1);
    }

    #[test]
    fn no_list_is_no_slots() {
        assert_eq!(slots(&json!({}), "10", now()), Slots::default());
        assert_eq!(slots(&json!(null), "10", now()), Slots::default());
    }
}
