//! A person's Discord profile: who they are across Discord, and in one
//! server. See <https://docs.discord.food/resources/user#get-user-profile>.

use serde_json::Value;

use crate::{
    message::{iso_ms, snowflake_ms},
    ready,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub user_id: String,
    /// Their nickname in the server, else their display name or username.
    pub name: String,
    pub username: String,
    /// The address of their avatar: the server's, else their own.
    pub avatar: String,
    /// The address of their banner: the server's, else their own.
    pub banner: Option<String>,
    /// The banner's colour when they have no banner picture.
    pub accent: Option<u32>,
    pub pronouns: Option<String>,
    /// Their "About me", in Discord Markdown.
    pub bio: Option<String>,
    /// Their roles' ids in the server.
    pub roles: Vec<String>,
    /// When they joined the server, in Unix milliseconds.
    pub joined: Option<u64>,
    /// When their account was made, in Unix milliseconds.
    pub created: u64,
    /// The servers this account shares with them.
    pub mutual_servers: Vec<MutualServer>,
    /// The friends this account shares with them.
    pub mutual_friends: Vec<MutualFriend>,
    /// The two colours of their profile's theme, a Nitro setting: the card's
    /// gradient runs from the first to the second.
    #[serde(default)]
    pub theme: Option<[u32; 2]>,
    /// The accounts on other services their profile shows.
    #[serde(default)]
    pub connections: Vec<Connection>,
}

/// An account on another service that a profile shows, such as Steam.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    /// The service, as Discord names it: `steam`, `twitch`, `github`.
    pub kind: String,
    pub name: String,
    pub verified: bool,
    /// Where the account is, when Discord's client knows how to link it.
    pub url: Option<String>,
    /// When the account was made, in Unix milliseconds, when the service says.
    #[serde(default)]
    pub since: Option<u64>,
}

/// A connected account's page on its service, where Discord's client links
/// one.
fn connection_url(kind: &str, id: &str, name: &str) -> Option<String> {
    Some(match kind {
        "steam" => format!("https://steamcommunity.com/profiles/{id}"),
        "twitch" => format!("https://twitch.tv/{name}"),
        "github" => format!("https://github.com/{name}"),
        "youtube" => format!("https://www.youtube.com/channel/{id}"),
        "twitter" | "x" => format!("https://twitter.com/{name}"),
        "reddit" => format!("https://reddit.com/u/{name}"),
        "spotify" => format!("https://open.spotify.com/user/{id}"),
        "tiktok" => format!("https://tiktok.com/@{name}"),
        "bluesky" => format!("https://bsky.app/profile/{name}"),
        _ => return None,
    })
}

/// A server this account shares with someone.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MutualServer {
    pub id: String,
    /// Their nickname there, when they set one.
    pub nick: Option<String>,
}

/// A friend this account shares with someone.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MutualFriend {
    pub id: String,
    pub name: String,
    /// The address of their avatar.
    pub avatar: String,
}

/// This account's own profile as Discord's profile editors change it, in
/// a server or its main one: what it set, and what shows where it set
/// nothing. An empty text or no picture goes by `fallback`, as Discord's
/// client shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnProfile {
    /// The server nickname, or the display name.
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    /// The addresses of the avatar and banner, when set.
    pub avatar: Option<String>,
    pub banner: Option<String>,
    /// The main profile's banner colour.
    pub accent: Option<u32>,
    /// In a server, its profile outside any server; outside, the username
    /// and Discord's default avatar.
    pub fallback: Profile,
}

/// The largest picture Discord's client uploads as an avatar or banner.
pub const PICTURE_LIMIT: u64 = 10 * 1024 * 1024;

/// This account's profile in `guild`, from the profile endpoint's answer
/// for its own id with `guild_id`.
pub fn parse_server(value: &Value, guild: &str) -> Option<OwnProfile> {
    let main = parse(value, None)?;
    let (server, member) = (&value["guild_member_profile"], &value["guild_member"]);
    let owned = |value: &Value| text(value).map(str::to_owned).unwrap_or_default();
    Some(OwnProfile {
        name: owned(&member["nick"]),
        pronouns: owned(&server["pronouns"]),
        bio: text(&server["bio"])
            .or_else(|| text(&member["bio"]))
            .map(str::to_owned)
            .unwrap_or_default(),
        avatar: text(&member["avatar"])
            .map(|hash| ready::member_avatar(guild, &main.user_id, hash)),
        banner: text(&server["banner"])
            .or_else(|| text(&member["banner"]))
            .map(|hash| ready::member_banner(guild, &main.user_id, hash)),
        accent: None,
        fallback: main,
    })
}

/// This account's main profile, from the profile endpoint's answer for its
/// own id without a server.
pub fn parse_main(value: &Value) -> Option<OwnProfile> {
    let profile = parse(value, None)?;
    let (user, own) = (&value["user"], &value["user_profile"]);
    let id = profile.user_id.clone();
    let fallback = Profile {
        name: profile.username.clone(),
        avatar: ready::avatar(&id, None),
        banner: None,
        accent: None,
        pronouns: None,
        bio: None,
        ..profile.clone()
    };
    Some(OwnProfile {
        name: user["global_name"].as_str().unwrap_or_default().to_owned(),
        pronouns: profile.pronouns.unwrap_or_default(),
        bio: profile.bio.unwrap_or_default(),
        avatar: text(&user["avatar"]).map(|hash| ready::avatar(&id, Some(hash))),
        banner: profile.banner,
        accent: color(&own["accent_color"]).or_else(|| color(&user["accent_color"])),
        fallback,
    })
}

/// A colour field that may be missing or null.
fn color(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|color| u32::try_from(color).ok())
}

/// A profile's theme: `theme_colors`, two colours as numbers.
fn theme_colors(value: &Value) -> Option<[u32; 2]> {
    let colors = value.as_array()?;
    Some([color(colors.first()?)?, color(colors.get(1)?)?])
}

/// A text field that may be missing, null or empty.
fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

/// A profile from the profile endpoint's answer. The server's profile, asked
/// for with `guild`, wins over the person's own where they set one.
pub fn parse(value: &Value, guild: Option<&str>) -> Option<Profile> {
    let user = &value["user"];
    let id = user["id"].as_str()?.to_owned();
    let own = &value["user_profile"];
    let (server, member) = match guild {
        Some(_) => (&value["guild_member_profile"], &value["guild_member"]),
        None => (&Value::Null, &Value::Null),
    };
    let name = text(&member["nick"])
        .or_else(|| ready::display_name(user))
        .unwrap_or("Discord user")
        .to_owned();
    let avatar = match (guild, text(&member["avatar"])) {
        (Some(guild), Some(hash)) => ready::member_avatar(guild, &id, hash),
        _ => ready::avatar(&id, user["avatar"].as_str()),
    };
    let banner = match (guild, text(&server["banner"])) {
        (Some(guild), Some(hash)) => Some(ready::member_banner(guild, &id, hash)),
        _ => text(&own["banner"])
            .or_else(|| text(&user["banner"]))
            .map(|hash| ready::banner(&id, hash)),
    };
    Some(Profile {
        name,
        username: user["username"].as_str().unwrap_or_default().to_owned(),
        avatar,
        banner,
        accent: color(&server["accent_color"])
            .or_else(|| color(&own["accent_color"]))
            .or_else(|| color(&user["accent_color"])),
        pronouns: text(&server["pronouns"])
            .or_else(|| text(&own["pronouns"]))
            .map(str::to_owned),
        bio: text(&server["bio"])
            .or_else(|| text(&own["bio"]))
            .or_else(|| text(&user["bio"]))
            .map(str::to_owned),
        roles: member["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|role| role.as_str().map(str::to_owned))
            .collect(),
        joined: member["joined_at"].as_str().and_then(iso_ms),
        created: snowflake_ms(&id),
        mutual_servers: value["mutual_guilds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|guild| {
                Some(MutualServer {
                    id: guild["id"].as_str()?.to_owned(),
                    nick: text(&guild["nick"]).map(str::to_owned),
                })
            })
            .collect(),
        mutual_friends: value["mutual_friends"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|friend| {
                let id = friend["id"].as_str()?;
                Some(MutualFriend {
                    id: id.to_owned(),
                    name: ready::display_name(friend)?.to_owned(),
                    avatar: ready::avatar(id, friend["avatar"].as_str()),
                })
            })
            .collect(),
        theme: theme_colors(&server["theme_colors"]).or_else(|| theme_colors(&own["theme_colors"])),
        connections: value["connected_accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|account| {
                let kind = text(&account["type"])?;
                let name = text(&account["name"])?;
                let id = account["id"].as_str().unwrap_or_default();
                Some(Connection {
                    kind: kind.to_owned(),
                    name: name.to_owned(),
                    verified: account["verified"].as_bool().unwrap_or(false),
                    url: connection_url(kind, id, name),
                    since: account["metadata"]["created_at"].as_str().and_then(iso_ms),
                })
            })
            .collect(),
        user_id: id,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_servers_profile_wins_over_their_own() {
        let answer = json!({
            "user": {"id": "1400000000000000200", "username": "ann", "global_name": "Ann",
                     "avatar": "abc", "banner": null, "accent_color": 16711680},
            "user_profile": {"bio": "Hi, I'm **Ann**", "pronouns": "she/her", "banner": "def",
                             "accent_color": null},
            "guild_member_profile": {"guild_id": "10", "bio": "", "pronouns": "", "banner": "ghi",
                                     "accent_color": 255},
            "guild_member": {"nick": "Annie", "avatar": null, "roles": ["30", "31"],
                             "joined_at": "2026-09-01T09:00:00.000000+00:00"},
            "mutual_guilds": [{"id": "10", "nick": "Annie"}, {"id": "11", "nick": null}],
            "mutual_friends": [{"id": "3", "username": "bob", "global_name": "Bob"}],
        });
        let profile = parse(&answer, Some("10")).unwrap();
        assert_eq!(profile.name, "Annie");
        assert_eq!(profile.username, "ann");
        assert!(
            profile
                .avatar
                .ends_with("/avatars/1400000000000000200/abc.png?size=128")
        );
        assert!(
            profile
                .banner
                .as_deref()
                .unwrap()
                .ends_with("/guilds/10/users/1400000000000000200/banners/ghi.png?size=600")
        );
        assert_eq!(profile.accent, Some(255));
        assert_eq!(
            profile.pronouns.as_deref(),
            Some("she/her"),
            "an empty server one falls back"
        );
        assert_eq!(profile.bio.as_deref(), Some("Hi, I'm **Ann**"));
        assert_eq!(profile.roles, ["30", "31"]);
        assert_eq!(profile.joined, Some(1_788_253_200_000));
        assert_eq!(profile.created, 1_753_856_410_742);
        assert_eq!(
            profile.mutual_servers,
            [
                MutualServer {
                    id: "10".into(),
                    nick: Some("Annie".into())
                },
                MutualServer {
                    id: "11".into(),
                    nick: None
                }
            ]
        );
        assert_eq!(profile.mutual_friends[0].id, "3");
        assert_eq!(profile.mutual_friends[0].name, "Bob");
        assert!(profile.mutual_friends[0].avatar.contains("/embed/avatars/"));

        let own = parse_server(&answer, "10").unwrap();
        assert_eq!(own.name, "Annie");
        assert_eq!(own.pronouns, "", "the main profile's shows instead");
        assert_eq!(own.bio, "");
        assert_eq!(own.avatar, None);
        assert!(
            own.banner
                .as_deref()
                .unwrap()
                .ends_with("/guilds/10/users/1400000000000000200/banners/ghi.png?size=600")
        );
        assert_eq!(own.fallback.name, "Ann");
        assert_eq!(own.fallback.pronouns.as_deref(), Some("she/her"));

        // Her main profile: what she set, over her username and Discord's
        // default avatar.
        let main = parse_main(&answer).unwrap();
        assert_eq!(main.name, "Ann");
        assert_eq!(main.pronouns, "she/her");
        assert_eq!(main.bio, "Hi, I'm **Ann**");
        assert!(
            main.avatar
                .as_deref()
                .unwrap()
                .contains("/avatars/1400000000000000200/abc.png")
        );
        assert!(
            main.banner
                .as_deref()
                .unwrap()
                .ends_with("/banners/1400000000000000200/def.png?size=600")
        );
        assert_eq!(
            main.accent,
            Some(16_711_680),
            "the user's, as the profile's is null"
        );
        assert_eq!(main.fallback.name, "ann");
        assert!(main.fallback.avatar.contains("/embed/avatars/"));
        assert_eq!((main.fallback.bio, main.fallback.banner), (None, None));

        // Outside a server: their own banner and name.
        let profile = parse(&answer, None).unwrap();
        assert!(
            profile
                .banner
                .as_deref()
                .unwrap()
                .ends_with("/banners/1400000000000000200/def.png?size=600")
        );
        assert_eq!(profile.accent, Some(16_711_680));
    }

    #[test]
    fn reads_the_theme_the_server_or_the_person_set() {
        let answer = json!({
            "user": {"id": "1400000000000000200", "username": "ann"},
            "user_profile": {"theme_colors": [1052945, 6631526]},
            "guild_member_profile": {"guild_id": "10", "theme_colors": null},
        });
        assert_eq!(
            parse(&answer, Some("10")).unwrap().theme,
            Some([1052945, 6631526])
        );
        let own = json!({"user": {"id": "1"}, "user_profile": {"theme_colors": [1]}});
        assert_eq!(parse(&own, None).unwrap().theme, None, "needs both");
        assert_eq!(
            parse(&json!({"user": {"id": "1"}}), None).unwrap().theme,
            None
        );
    }
}
