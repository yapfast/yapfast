//! What this account's own settings list: the apps it authorized, the
//! devices it is signed in on and the services it linked. See "Account
//! settings" in docs/discord/account-settings.md.

use serde_json::Value;

/// An app this account let into it: one authorization, which revoking ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedApp {
    /// The authorization's id, which revoking names; not the app's.
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// What it may do, as Discord names the scopes.
    pub scopes: Vec<String>,
}

impl AuthorizedApp {
    /// The apps of an answer from the list of this account's authorizations,
    /// in Discord's order. One without an id or a name is left out.
    pub fn list(answer: &Value) -> Vec<Self> {
        answer
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|authorization| {
                let app = &authorization["application"];
                let id = authorization["id"].as_str()?;
                let name = app["name"].as_str().filter(|name| !name.is_empty())?;
                Some(Self {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    description: app["description"].as_str().unwrap_or_default().to_owned(),
                    icon: app["id"]
                        .as_str()
                        .zip(app["icon"].as_str())
                        .map(|(app, icon)| {
                            format!("https://cdn.discordapp.com/app-icons/{app}/{icon}.png?size=64")
                        }),
                    scopes: authorization["scopes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|scope| scope.as_str().map(str::to_owned))
                        .collect(),
                })
            })
            .collect()
    }
}

/// A device this account is signed in on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// The hash of its session, which signing it out names.
    pub id: String,
    pub os: String,
    pub platform: String,
    pub location: String,
    /// When it last used the account, as Discord writes it: a timestamp.
    pub last_used: String,
    /// Whether this is the device Yapfast runs on, when Discord says so.
    pub current: bool,
}

impl Device {
    /// The devices of an answer from the list of this account's sessions,
    /// in Discord's order; `current` is the hash of the session READY named.
    pub fn list(answer: &Value, current: Option<&str>) -> Vec<Self> {
        answer["user_sessions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|session| {
                let id = session["id_hash"].as_str().filter(|id| !id.is_empty())?;
                let client = &session["client_info"];
                let text = |name: &str| client[name].as_str().unwrap_or_default().to_owned();
                Some(Self {
                    id: id.to_owned(),
                    os: text("os"),
                    platform: text("platform"),
                    location: text("location"),
                    last_used: session["approx_last_used_time"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    current: current == Some(id),
                })
            })
            .collect()
    }
}

/// A service this account linked, such as Spotify, and how it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// The service's id for Discord, such as `spotify`.
    pub service: String,
    /// The service's name to read, such as "Spotify".
    pub label: String,
    /// The account's id at the service.
    pub id: String,
    /// The account's name there.
    pub name: String,
    pub verified: bool,
    /// Whether the profile shows it.
    pub visible: bool,
    /// Whether it may show what the account plays or does as an activity.
    pub show_activity: bool,
    /// Whether the service took its access back, which linking again fixes.
    pub revoked: bool,
}

impl Connection {
    /// The accounts of an answer from the list of this account's
    /// connections, in Discord's order. One without a service or an id is
    /// left out.
    pub fn list(answer: &Value) -> Vec<Self> {
        answer
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|connection| {
                let service = connection["type"]
                    .as_str()
                    .filter(|kind| !kind.is_empty())?;
                let id = connection["id"].as_str().filter(|id| !id.is_empty())?;
                let flag = |name: &str| {
                    connection[name].as_bool().unwrap_or_else(|| {
                        connection[name].as_u64().is_some_and(|number| number > 0)
                    })
                };
                Some(Self {
                    service: service.to_owned(),
                    label: service_name(service),
                    id: id.to_owned(),
                    name: connection["name"].as_str().unwrap_or(id).to_owned(),
                    verified: flag("verified"),
                    visible: flag("visibility"),
                    show_activity: flag("show_activity"),
                    revoked: flag("revoked"),
                })
            })
            .collect()
    }
}

/// A service's name to read, from the id Discord uses for it.
pub fn service_name(service: &str) -> String {
    match service {
        "spotify" => "Spotify".into(),
        "twitch" => "Twitch".into(),
        "youtube" => "YouTube".into(),
        "steam" => "Steam".into(),
        "github" => "GitHub".into(),
        "reddit" => "Reddit".into(),
        "twitter" | "twitter_legacy" => "X (Twitter)".into(),
        "xbox" => "Xbox".into(),
        "battlenet" => "Battle.net".into(),
        "epicgames" => "Epic Games".into(),
        "facebook" => "Facebook".into(),
        "tiktok" => "TikTok".into(),
        "playstation" | "playstation-stg" => "PlayStation Network".into(),
        "leagueoflegends" => "League of Legends".into(),
        "riotgames" => "Riot Games".into(),
        "paypal" => "PayPal".into(),
        "ebay" => "eBay".into(),
        "instagram" => "Instagram".into(),
        "bluesky" => "Bluesky".into(),
        "mastodon" => "Mastodon".into(),
        "crunchyroll" => "Crunchyroll".into(),
        "roblox" => "Roblox".into(),
        "amazon-music" => "Amazon Music".into(),
        "bungie" => "Bungie.net".into(),
        "domain" => "Domain".into(),
        other => {
            let mut letters = other.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn authorized_apps_keep_their_authorization_ids_scopes_and_icons() {
        let apps = AuthorizedApp::list(&json!([
            {
                "id": "900",
                "scopes": ["identify", "guilds"],
                "application": {"id": "5", "name": "Bot", "description": "Helps", "icon": "abc"}
            },
            {"id": "901", "scopes": [], "application": {"id": "6", "name": ""}},
            {"scopes": [], "application": {"id": "7", "name": "No id"}},
            {"id": "902", "application": {"id": "8", "name": "Plain"}}
        ]));
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].id, "900");
        assert_eq!(apps[0].scopes, ["identify", "guilds"]);
        assert_eq!(
            apps[0].icon.as_deref(),
            Some("https://cdn.discordapp.com/app-icons/5/abc.png?size=64")
        );
        assert_eq!(apps[1].name, "Plain");
        assert_eq!(apps[1].icon, None);
        assert!(AuthorizedApp::list(&json!({"message": "no"})).is_empty());
    }

    #[test]
    fn devices_mark_the_session_ready_named() {
        let answer = json!({"user_sessions": [
            {
                "id_hash": "aaa",
                "approx_last_used_time": "2026-11-20T10:00:00.000000+00:00",
                "client_info": {"os": "Windows", "platform": "Discord Client", "location": "Berlin, Germany"}
            },
            {"id_hash": "bbb", "client_info": {}},
            {"client_info": {"os": "Linux"}}
        ]});
        let devices = Device::list(&answer, Some("bbb"));
        assert_eq!(devices.len(), 2);
        assert!(!devices[0].current);
        assert!(devices[1].current);
        assert_eq!(devices[0].location, "Berlin, Germany");
        assert!(
            Device::list(&answer, None)
                .iter()
                .all(|device| !device.current)
        );
    }

    #[test]
    fn connections_read_numbers_and_booleans_as_flags() {
        let connections = Connection::list(&json!([
            {"type": "spotify", "id": "s1", "name": "me", "verified": true, "visibility": 1, "show_activity": true},
            {"type": "github", "id": "g1", "visibility": false, "revoked": true},
            {"type": "", "id": "x"},
            {"type": "steam"}
        ]));
        assert_eq!(connections.len(), 2);
        assert!(connections[0].visible && connections[0].show_activity && connections[0].verified);
        assert_eq!(connections[0].label, "Spotify");
        assert_eq!(
            connections[1].name, "g1",
            "the id stands in for a missing name"
        );
        assert!(connections[1].revoked && !connections[1].visible);
    }

    #[test]
    fn unknown_services_get_a_capital() {
        assert_eq!(service_name("newservice"), "Newservice");
        assert_eq!(service_name(""), "");
    }
}
