//! This account's notification settings for one server, or for its DMs, as
//! Discord keeps them in its user guild settings: the server's mute and
//! level, which mentions call on it, whether muted channels hide, and each
//! channel's and category's own mute and level, whether a channel is
//! pinned to the top of the channel list, and which channels and categories
//! the channel list follows when it lists only those. READY lists them in
//! `user_guild_settings`, USER_GUILD_SETTINGS_UPDATE replaces one, and
//! `PATCH /users/@me/guilds/{guild}/settings` changes them; see
//! <https://docs.discord.food/resources/user-settings>.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use crate::message::iso_ms;

/// Which messages notify: Discord's `message_notifications`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Level {
    All,
    /// Large servers default to it, so a server not yet known is treated so.
    #[default]
    Mentions,
    Nothing,
}

impl Level {
    /// A level, or none where Discord's 3 means the server's default, or for
    /// a channel its category's.
    pub fn from(value: &Value) -> Option<Self> {
        match value.as_u64()? {
            0 => Some(Self::All),
            1 => Some(Self::Mentions),
            2 => Some(Self::Nothing),
            _ => None,
        }
    }

    /// Discord's number for a level, 3 for none.
    pub fn code(level: Option<Self>) -> u64 {
        match level {
            Some(Self::All) => 0,
            Some(Self::Mentions) => 1,
            Some(Self::Nothing) => 2,
            None => 3,
        }
    }
}

/// A mute: whether it was set, and when it ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Mute {
    pub muted: bool,
    /// When it ends, in Unix milliseconds; none while it lasts until unmuted.
    pub until: Option<u64>,
}

impl Mute {
    /// The `muted` flag and `mute_config` of a settings object, an
    /// override or a thread member.
    pub(crate) fn read(value: &Value) -> Self {
        let muted = value["muted"].as_bool().unwrap_or(false);
        Self {
            muted,
            until: value["mute_config"]["end_time"]
                .as_str()
                .and_then(iso_ms)
                .filter(|_| muted),
        }
    }

    /// Whether it's in force at `now`, in Unix milliseconds.
    pub fn active(&self, now: u64) -> bool {
        self.muted && self.until.is_none_or(|until| until > now)
    }
}

/// A channel override's flag that pins the channel to the top of the
/// channel list, Discord's `FAVORITED`.
pub const PINNED: u64 = 1 << 11;
/// A channel override's flag that follows the channel, or every channel in
/// a category, in a server whose channel list lists only those followed:
/// Discord's `OPT_IN_ENABLED`, which its client also sets with the first
/// pin.
const OPT_IN_ENABLED: u64 = 1 << 12;
/// A server's own flag that lists only the channels followed, which Discord's
/// client calls turning off Show All Channels: `OPT_IN_CHANNELS_ON`.
const OPT_IN_CHANNELS_ON: u64 = 1 << 14;

/// A channel's or category's own settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Override {
    pub mute: Mute,
    /// None: its category's level, or the server's.
    pub level: Option<Level>,
    /// Its `flags`, such as [`PINNED`].
    #[serde(default)]
    pub flags: u64,
}

impl Override {
    pub fn pinned(&self) -> bool {
        self.flags & PINNED != 0
    }

    /// The flags that pin the channel, or unpin it, as Discord's client
    /// writes them: pinning also opts the channel in, and unpinning leaves
    /// that and every other flag as it was.
    pub fn pin_flags(&self, pin: bool) -> u64 {
        if pin {
            self.flags | PINNED | OPT_IN_ENABLED
        } else {
            self.flags & !PINNED
        }
    }

    /// Whether the channel list follows it, see [`OPT_IN_ENABLED`].
    pub fn followed(&self) -> bool {
        self.flags & OPT_IN_ENABLED != 0
    }

    /// The flags that add the channel or category to the channel list, or
    /// remove it, as Discord's client writes them: removing a channel
    /// unpins it too.
    pub fn follow_flags(&self, follow: bool) -> u64 {
        if follow {
            self.flags | OPT_IN_ENABLED
        } else {
            self.flags & !(OPT_IN_ENABLED | PINNED)
        }
    }
}

/// This account's notification settings for one server, or its DMs.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Notifications {
    pub mute: Mute,
    /// None: the server's default, [`crate::ready::Server::default_level`].
    pub level: Option<Level>,
    /// @everyone and @here don't call on this account.
    pub suppress_everyone: bool,
    /// Role mentions don't call on this account.
    pub suppress_roles: bool,
    /// Muted channels don't show in the channel list.
    pub hide_muted_channels: bool,
    /// Channels' and categories' own settings, by id.
    pub overrides: HashMap<String, Override>,
    /// The server's own `flags`, such as [`OPT_IN_CHANNELS_ON`].
    #[serde(default)]
    pub flags: u64,
}

impl Notifications {
    /// A user guild settings object.
    pub fn read(value: &Value) -> Self {
        let overrides = value["channel_overrides"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|channel| {
                Some((
                    channel["channel_id"].as_str()?.to_owned(),
                    Override {
                        mute: Mute::read(channel),
                        level: Level::from(&channel["message_notifications"]),
                        flags: channel["flags"].as_u64().unwrap_or_default(),
                    },
                ))
            })
            .collect();
        Self {
            mute: Mute::read(value),
            level: Level::from(&value["message_notifications"]),
            suppress_everyone: value["suppress_everyone"].as_bool().unwrap_or(false),
            suppress_roles: value["suppress_roles"].as_bool().unwrap_or(false),
            hide_muted_channels: value["hide_muted_channels"].as_bool().unwrap_or(false),
            overrides,
            flags: value["flags"].as_u64().unwrap_or_default(),
        }
    }

    /// The settings a channel or category has of its own.
    pub fn of(&self, channel: &str) -> Override {
        self.overrides.get(channel).copied().unwrap_or_default()
    }

    /// Whether a channel or category is muted itself at `now`.
    pub fn channel_muted(&self, channel: &str, now: u64) -> bool {
        self.overrides
            .get(channel)
            .is_some_and(|channel| channel.mute.active(now))
    }

    /// The level that applies to a channel in `category`: its own, else
    /// its category's, else this account's for the server, else
    /// `default`, the server's.
    pub fn level(&self, channel: &str, category: Option<&str>, default: Level) -> Level {
        [Some(channel), category]
            .into_iter()
            .flatten()
            .find_map(|place| self.overrides.get(place)?.level)
            .or(self.level)
            .unwrap_or(default)
    }

    /// Whether the channel list lists only the channels followed, Show
    /// All Channels turned off. Discord's client heeds it only in a
    /// Community server.
    pub fn follows_only(&self) -> bool {
        self.flags & OPT_IN_CHANNELS_ON != 0
    }

    /// Whether the channel list follows a channel, itself or through its
    /// `category`.
    pub fn follows(&self, channel: &str, category: Option<&str>) -> bool {
        [Some(channel), category]
            .into_iter()
            .flatten()
            .any(|place| self.of(place).followed())
    }

    /// Whether the channel list follows any channel or category.
    pub fn follows_any(&self) -> bool {
        self.overrides.values().any(Override::followed)
    }

    /// The server's flags with Show All Channels turned on, or off with
    /// `show` false, the rest as they were.
    pub fn show_all_flags(&self, show: bool) -> u64 {
        if show {
            self.flags & !OPT_IN_CHANNELS_ON
        } else {
            self.flags | OPT_IN_CHANNELS_ON
        }
    }

    /// The channels pinned to the top of the channel list, in no order.
    pub fn pinned(&self) -> impl Iterator<Item = &str> {
        self.overrides
            .iter()
            .filter(|(_, channel)| channel.pinned())
            .map(|(id, _)| id.as_str())
    }

    /// The soonest end, after `now`, of a mute in force, server's or
    /// channel's, when the muted state shown changes by itself.
    pub fn next_end(&self, now: u64) -> Option<u64> {
        std::iter::once(&self.mute)
            .chain(self.overrides.values().map(|channel| &channel.mute))
            .filter(|mute| mute.active(now))
            .filter_map(|mute| mute.until)
            .min()
    }
}

/// A change to a server's or a channel's notification settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Mute for `seconds`, or with none until unmuted; or unmute with
    /// `muted` false.
    Mute {
        muted: bool,
        seconds: Option<u64>,
    },
    /// Which messages notify; none for the category's or server's default.
    Level(Option<Level>),
    /// These are the server's alone.
    HideMutedChannels(bool),
    SuppressEveryone(bool),
    SuppressRoles(bool),
}

impl Change {
    /// The body of `PATCH /users/@me/guilds/{guild}/settings` that makes this
    /// change, to the server or with `channel`, to a channel or category in
    /// it: Discord takes the channels' overrides by id, and keeps what a
    /// body leaves out. None for a change only a server has.
    pub fn body(self, channel: Option<&str>) -> Option<Value> {
        let mut fields = Map::new();
        match self {
            Self::Mute { muted, seconds } => {
                let config = muted.then(|| match seconds {
                    Some(seconds) => {
                        let end = chrono::Utc::now()
                            + chrono::Duration::seconds(i64::try_from(seconds).unwrap_or(i64::MAX));
                        json!({
                            "selected_time_window": seconds,
                            "end_time": end.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                        })
                    }
                    // Discord's client gives -1 for a mute without an end.
                    None => json!({"selected_time_window": -1, "end_time": null}),
                });
                fields.insert("muted".into(), muted.into());
                fields.insert("mute_config".into(), config.into());
            }
            Self::Level(level) => {
                fields.insert("message_notifications".into(), Level::code(level).into());
            }
            Self::HideMutedChannels(_) | Self::SuppressEveryone(_) | Self::SuppressRoles(_)
                if channel.is_some() =>
            {
                return None;
            }
            Self::HideMutedChannels(on) => {
                fields.insert("hide_muted_channels".into(), on.into());
            }
            Self::SuppressEveryone(on) => {
                fields.insert("suppress_everyone".into(), on.into());
            }
            Self::SuppressRoles(on) => {
                fields.insert("suppress_roles".into(), on.into());
            }
        }
        Some(match channel {
            Some(channel) => json!({"channel_overrides": {channel: fields}}),
            None => Value::Object(fields),
        })
    }
}

/// The body of `PATCH /users/@me/guilds/{guild}/settings` that sets the
/// server's own `flags`, which turn Show All Channels on or off, and the
/// `flags` of the channels it `follow`s with them, as Discord's client
/// follows every channel when it turns Show All Channels off with none
/// followed; see [`Notifications::show_all_flags`].
pub fn show_all_body(flags: u64, follow: &[(String, u64)]) -> Value {
    let mut body = json!({"flags": flags});
    if !follow.is_empty() {
        body["channel_overrides"] = follow
            .iter()
            .map(|(channel, flags)| (channel.clone(), json!({"flags": flags})))
            .collect::<Map<_, _>>()
            .into();
    }
    body
}

/// The body of `PATCH /users/@me/guilds/settings` that sets a channel's
/// override `flags`, as Discord's client pins and unpins a channel, and
/// adds it to the channel list or removes it; see [`Override::pin_flags`]
/// and [`Override::follow_flags`].
pub fn flags_body(guild: &str, channel: &str, flags: u64) -> Value {
    json!({"guilds": {guild: {"channel_overrides": {channel: {"flags": flags}}}}})
}

/// The body of `PATCH /users/@me/guilds/settings` that makes the same
/// change to each server of `guilds`, as Discord's client sends a server's
/// own settings: `{"guilds": {"<id>": <partial settings>}}`. A mute with an
/// end ends at the same time for each.
pub fn servers_body(guilds: &[&str], change: Change) -> Value {
    let fields = change.body(None).unwrap_or_default();
    let guilds: Map<String, Value> = guilds
        .iter()
        .map(|guild| ((*guild).to_owned(), fields.clone()))
        .collect();
    json!({ "guilds": guilds })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_several_servers_in_one_body() {
        let body = servers_body(&["10", "11"], Change::Level(Some(Level::Mentions)));
        assert_eq!(
            body,
            json!({"guilds": {
                "10": {"message_notifications": 1},
                "11": {"message_notifications": 1},
            }})
        );
        let body = servers_body(
            &["10", "11"],
            Change::Mute {
                muted: true,
                seconds: Some(900),
            },
        );
        let ten = &body["guilds"]["10"];
        assert_eq!(ten["muted"], true);
        assert_eq!(ten["mute_config"]["selected_time_window"], 900);
        assert!(ten["mute_config"]["end_time"].is_string());
        assert_eq!(ten, &body["guilds"]["11"], "one end for both");
        assert_eq!(
            servers_body(&["10"], Change::SuppressRoles(true)),
            json!({"guilds": {"10": {"suppress_roles": true}}})
        );
    }

    #[test]
    fn reads_mutes_levels_and_overrides() {
        let settings = Notifications::read(&json!({
            "guild_id": "10", "muted": true,
            "mute_config": {"end_time": "2030-01-01T00:00:00.000Z", "selected_time_window": 900},
            "message_notifications": 3, "suppress_everyone": true, "hide_muted_channels": true,
            "channel_overrides": [
                {"channel_id": "4", "muted": true, "mute_config": null, "message_notifications": 0},
                {"channel_id": "11", "muted": false, "message_notifications": 3},
                // An end that has passed: no longer muted.
                {"channel_id": "12", "muted": true,
                 "mute_config": {"end_time": "2020-01-01T00:00:00+00:00"}, "message_notifications": 2},
            ],
        }));
        let now = iso_ms("2026-10-13T00:00:00Z").unwrap();
        let end = iso_ms("2030-01-01T00:00:00Z").unwrap();
        assert_eq!(
            settings.mute,
            Mute {
                muted: true,
                until: Some(end)
            }
        );
        assert!(settings.mute.active(now));
        assert!(!settings.mute.active(end));
        assert_eq!(settings.level, None);
        assert!(settings.suppress_everyone && !settings.suppress_roles);
        assert!(settings.hide_muted_channels);
        assert!(settings.channel_muted("4", now));
        assert!(!settings.channel_muted("11", now));
        assert!(!settings.channel_muted("12", now));
        // The channel's own level, else its category's, else the server's.
        assert_eq!(
            settings.level("12", Some("4"), Level::Mentions),
            Level::Nothing
        );
        assert_eq!(settings.level("11", Some("4"), Level::Mentions), Level::All);
        assert_eq!(settings.level("11", None, Level::Mentions), Level::Mentions);
        assert_eq!(settings.next_end(now), Some(end));
        assert_eq!(settings.of("11").level, None);
    }

    #[test]
    fn pins_and_unpins_as_discords_client_does() {
        let settings = Notifications::read(&json!({
            "guild_id": "10",
            "channel_overrides": [
                {"channel_id": "4", "muted": false, "message_notifications": 3, "flags": 6144},
                {"channel_id": "11", "muted": true, "message_notifications": 3, "flags": 1024},
            ],
        }));
        assert_eq!(settings.pinned().collect::<Vec<_>>(), ["4"]);
        // A channel without an override pins with the opt-in flag too.
        assert_eq!(settings.of("12").pin_flags(true), 6144);
        assert_eq!(settings.of("11").pin_flags(true), 1024 | 6144);
        // Unpinning leaves the rest, the opt-in flag with it.
        assert_eq!(settings.of("4").pin_flags(false), 4096);
        assert_eq!(
            flags_body("10", "12", 6144),
            json!({"guilds": {"10": {"channel_overrides": {"12": {"flags": 6144}}}}})
        );
    }

    #[test]
    fn follows_channels_as_discords_client_does() {
        let settings = Notifications::read(&json!({
            "guild_id": "10",
            "flags": 16384 | 1,
            "channel_overrides": [
                {"channel_id": "4", "muted": false, "message_notifications": 3, "flags": 4096},
                {"channel_id": "11", "muted": false, "message_notifications": 3, "flags": 6144},
                {"channel_id": "12", "muted": true, "message_notifications": 3, "flags": 1024},
            ],
        }));
        assert!(settings.follows_only());
        assert!(settings.follows("11", None));
        assert!(settings.follows("13", Some("4")), "through its category");
        assert!(!settings.follows("12", None));
        assert!(settings.follows_any());
        // Following keeps the other flags; leaving unpins too.
        assert_eq!(settings.of("12").follow_flags(true), 1024 | 4096);
        assert_eq!(settings.of("11").follow_flags(false), 0);
        assert_eq!(settings.show_all_flags(true), 1);
        assert_eq!(settings.show_all_flags(false), 16384 | 1);
        assert!(!Notifications::default().follows_only());
        assert_eq!(show_all_body(0, &[]), json!({"flags": 0}));
        assert_eq!(
            show_all_body(16384, &[("11".into(), 4096), ("12".into(), 5120)]),
            json!({"flags": 16384, "channel_overrides": {
                "11": {"flags": 4096}, "12": {"flags": 5120},
            }})
        );
    }

    #[test]
    fn writes_changes_as_discord_takes_them() {
        assert_eq!(
            Change::Level(Some(Level::Nothing)).body(None),
            Some(json!({"message_notifications": 2}))
        );
        assert_eq!(
            Change::Level(None).body(Some("11")),
            Some(json!({"channel_overrides": {"11": {"message_notifications": 3}}}))
        );
        assert_eq!(
            Change::Mute {
                muted: false,
                seconds: None
            }
            .body(Some("11")),
            Some(json!({"channel_overrides": {"11": {"muted": false, "mute_config": null}}}))
        );
        assert_eq!(
            Change::Mute {
                muted: true,
                seconds: None
            }
            .body(None),
            Some(
                json!({"muted": true, "mute_config": {"selected_time_window": -1, "end_time": null}})
            )
        );
        let timed = Change::Mute {
            muted: true,
            seconds: Some(900),
        }
        .body(None)
        .unwrap();
        assert_eq!(timed["mute_config"]["selected_time_window"], 900);
        assert!(timed["mute_config"]["end_time"].as_str().is_some());
        assert_eq!(
            Change::HideMutedChannels(true).body(None),
            Some(json!({"hide_muted_channels": true}))
        );
        assert_eq!(Change::SuppressRoles(true).body(Some("11")), None);
    }
}
