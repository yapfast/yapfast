//! A person's Discord profile: who they are across Discord, and in one
//! server. See <https://docs.discord.food/resources/user#get-user-profile>.

use serde_json::Value;

use crate::{
    message::{Tag, iso_ms, snowflake_ms},
    profile_effect, ready,
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
    /// The server tag they wear by their name.
    #[serde(default)]
    pub tag: Option<Tag>,
    /// The badges their profile shows, in Discord's order.
    #[serde(default)]
    pub badges: Vec<Badge>,
    /// The SKU of the profile effect they equip, which
    /// [`crate::service::Discord::profile_effect`] describes.
    #[serde(default)]
    pub effect: Option<String>,
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

/// One of the badges a profile shows: HypeSquad, Active Developer, a
/// subscriber's Nitro tier and the like.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Badge {
    pub id: String,
    /// What it says on hover, such as "HypeSquad Bravery".
    pub description: String,
    /// The address of its picture.
    pub icon: String,
    /// Where Discord's client sends a click on it, when it does.
    pub link: Option<String>,
    /// The address of the picture Discord's profile gives it for its level,
    /// the key to its details in [`with_levels`].
    #[serde(default)]
    pub level_icon: Option<String>,
    /// What its tooltip says beyond the description, once [`with_levels`]
    /// has found it: only the badges that level up have any.
    #[serde(default)]
    pub level: Option<BadgeLevel>,
}

/// What Discord's tooltip shows for a badge that levels up, such as playing
/// time: its kind above, its level in capitals, how far along the person is,
/// and a large picture of the level.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BadgeLevel {
    /// The kind of badge ("Spielzeit"), when the level has a name of its own.
    pub kind: Option<String>,
    /// The level's name ("Eisern"), or the badge's when it has no levels.
    pub title: String,
    /// How far along they are ("1.816 Stunden gespielt").
    pub progress: String,
    /// The address of the large picture, an SVG.
    pub picture: String,
}

/// Adds the levels from `GET /users/{id}/badges`, an answer of
/// `{"badges": [{name, info_label, current_tier, tiers, ...}]}`, to the
/// badges they belong to. Discord's client matches them by the picture of
/// the level, which both lists carry.
pub fn with_levels(badges: &mut [Badge], value: &Value) {
    for entry in value["badges"].as_array().into_iter().flatten() {
        if entry["owned"].as_bool() == Some(false) {
            continue;
        }
        let (Some(name), Some(progress)) = (text(&entry["name"]), text(&entry["info_label"]))
        else {
            continue;
        };
        let tier = entry["tiers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tier| tier["key"].as_str() == entry["current_tier"].as_str());
        let picture = [
            tier.map(|tier| &tier["complex_icon_static_url"]),
            Some(&entry["complex_icon_static_url"]),
            Some(&entry["simple_icon_url"]),
        ]
        .into_iter()
        .flatten()
        .find_map(text);
        let Some(picture) = picture else { continue };
        let level = BadgeLevel {
            kind: tier.map(|_| name.to_owned()),
            title: tier
                .and_then(|tier| text(&tier["name"]))
                .unwrap_or(name)
                .to_owned(),
            progress: progress.to_owned(),
            picture: picture.to_owned(),
        };
        let simple = text(&entry["simple_icon_url"]);
        let raster = text(&entry["simple_icon_raster_url"]);
        let badge = badges.iter_mut().find(|badge| {
            simple.is_some() && badge.level_icon.as_deref() == simple
                || raster.is_some_and(|raster| raster == badge.icon)
        });
        if let Some(badge) = badge {
            badge.level = Some(level);
        }
    }
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
        tag: Tag::parse(&user["primary_guild"]),
        effect: profile_effect::sku(value, guild.is_some()),
        theme: theme_colors(&server["theme_colors"]).or_else(|| theme_colors(&own["theme_colors"])),
        badges: value["badges"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|badge| {
                Some(Badge {
                    id: text(&badge["id"])?.to_owned(),
                    description: text(&badge["description"])?.to_owned(),
                    icon: ready::badge_icon(text(&badge["icon"])?),
                    link: text(&badge["link"]).map(str::to_owned),
                    level_icon: text(&badge["simple_icon_url"]).map(str::to_owned),
                    level: None,
                })
            })
            .collect(),
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
    fn a_profile_lists_its_badges_in_discords_order() {
        let answer = json!({
            "user": {"id": "1400000000000000200", "username": "ann"},
            "badges": [
                {"id": "hypesquad_house_1", "description": "HypeSquad Bravery",
                 "icon": "8a88d63823d8a71cd5e390baa45efa02",
                 "link": "https://support.discord.com/hc/en-us/articles/360035962891"},
                {"id": "active_developer", "description": "Active Developer",
                 "icon": "6bdc42827a38498929a4920da12695d9"},
                {"id": "broken", "description": "", "icon": "abc"},
                {"id": "no_icon", "description": "No icon"},
            ],
        });
        let profile = parse(&answer, None).unwrap();
        let ids: Vec<_> = profile
            .badges
            .iter()
            .map(|badge| badge.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["hypesquad_house_1", "active_developer"],
            "one without a description or a picture is left out"
        );
        assert_eq!(profile.badges[0].description, "HypeSquad Bravery");
        assert!(
            profile.badges[0]
                .icon
                .ends_with("/badge-icons/8a88d63823d8a71cd5e390baa45efa02.png")
        );
        assert!(profile.badges[0].link.is_some());
        assert_eq!(profile.badges[1].link, None);

        let none = json!({"user": {"id": "1", "username": "bob"}, "badges": null});
        assert!(parse(&none, None).unwrap().badges.is_empty());
    }

    #[test]
    fn a_levelling_badge_takes_its_level_from_the_badges_answer() {
        let mut profile = parse(
            &json!({
                "user": {"id": "1", "username": "bob"},
                "badges": [
                    {"id": "game_time", "description": "Spielzeit", "icon": "aaa",
                     "simple_icon_url": "https://cdn.example/ironclad.svg"},
                    {"id": "quest_completed", "description": "Quest abgeschlossen", "icon": "7d9a"},
                    {"id": "premium", "description": "Abonnent seit 06.05.26", "icon": "ccc"},
                ],
            }),
            None,
        )
        .unwrap();
        let quests_raster = ready::badge_icon("7d9a");
        with_levels(
            &mut profile.badges,
            &json!({"badges": [
                {"name": "Spielzeit", "info_label": "1.816 Stunden gespielt", "owned": true,
                 "simple_icon_url": "https://cdn.example/ironclad.svg",
                 "current_tier": "ironclad",
                 "tiers": [
                     {"key": "casual", "name": "Locker", "complex_icon_static_url": "https://cdn.example/casual.svg"},
                     {"key": "ironclad", "name": "Eisern", "complex_icon_static_url": "https://cdn.example/big.svg"},
                 ]},
                {"name": "Quests", "info_label": "7 Quests Abgeschlossen", "owned": true,
                 "simple_icon_url": "https://cdn.example/quest.svg", "simple_icon_raster_url": quests_raster},
                {"name": "Streaming", "info_label": "1 Stunde", "owned": false,
                 "simple_icon_url": "https://cdn.example/none.svg"},
            ]}),
        );
        let time = profile.badges[0].level.as_ref().unwrap();
        assert_eq!(time.kind.as_deref(), Some("Spielzeit"));
        assert_eq!(time.title, "Eisern");
        assert_eq!(time.progress, "1.816 Stunden gespielt");
        assert_eq!(time.picture, "https://cdn.example/big.svg");
        // One without levels is named for itself, with its small picture.
        let quests = profile.badges[1].level.as_ref().unwrap();
        assert_eq!(
            (quests.kind.as_deref(), quests.title.as_str()),
            (None, "Quests")
        );
        assert_eq!(quests.picture, "https://cdn.example/quest.svg");
        // Nitro isn't in the answer, so it keeps its description alone.
        assert_eq!(profile.badges[2].level, None);
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
