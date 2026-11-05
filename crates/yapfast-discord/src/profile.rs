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
    /// The address of the frame around their avatar: the server's, else their
    /// own.
    #[serde(default)]
    pub decoration: Option<String>,
    /// The address of their banner: the server's, else their own.
    pub banner: Option<String>,
    /// The banner's colour when they have no banner picture.
    pub accent: Option<u32>,
    /// The profile's two theme colours, top and bottom: the server's, else
    /// their own.
    #[serde(default)]
    pub theme: Option<[u32; 2]>,
    /// The server tag beside their name.
    #[serde(default)]
    pub tag: Option<ready::ServerTag>,
    /// How they styled their name.
    #[serde(default)]
    pub name_style: Option<ready::NameStyle>,
    /// The layers of the effect playing over their card, bottom first.
    #[serde(default)]
    pub effect: Vec<EffectLayer>,
    /// The badges beside their name, the server's after their own.
    #[serde(default)]
    pub badges: Vec<Badge>,
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
    /// The widgets on their board, whose games only
    /// [`Discord::name_widgets`](crate::service::Discord::name_widgets) names.
    #[serde(default)]
    pub widgets: Vec<crate::widgets::Widget>,
    /// Where their wishlist is, if they have one that's said.
    #[serde(default)]
    pub wishlist: Option<crate::wishlist::Place>,
    /// They're a bot, whose badges open no window of badges.
    #[serde(default)]
    pub bot: bool,
    /// The frame around their card.
    #[serde(default)]
    pub frame: Option<Frame>,
    /// The collectibles behind [`Profile::effect`] and [`Profile::frame`],
    /// which [`Discord::profile`](crate::service::Discord::profile) asks
    /// after only when [`Collectibles`] wants them.
    #[serde(default)]
    pub effect_sku: Option<String>,
    #[serde(default)]
    pub frame_sku: Option<String>,
}

/// Which of a profile's collectibles to ask Discord about: each is a request
/// for what it's made of, then pictures. Settings turn them off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Collectibles {
    pub effect: bool,
    pub frame: bool,
}

/// A profile frame, measured on Discord's canvas.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    /// How wide the card is on the canvas, and how far past it the art runs.
    pub inner_width: u32,
    pub overflow_top: u32,
    pub overflow_bottom: u32,
    pub overflow_horizontal: u32,
    pub layers: Vec<FrameLayer>,
}

/// One picture of a profile frame.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrameLayer {
    /// The address of its still picture, the only one the client draws.
    pub src: String,
    /// `staple`, once at an end; `rail`, stretched down the card; `border`, repeated.
    pub kind: String,
    /// `top`, `bottom` or `center`.
    pub anchor: String,
    /// It goes over the card, not behind it.
    pub front: bool,
}

/// The profile frame's id in a profile, the server's before their own: the
/// one of `collectibles` of that type.
pub fn frame_sku<'a>(value: &'a Value, guild: Option<&str>) -> Option<&'a str> {
    let in_profile = |profile: &'a Value| {
        profile["collectibles"]
            .as_array()?
            .iter()
            // Discord's number for a frame; older answers named it.
            .find(|item| item["type"] == 3 || item["type"] == "profile_frame")?["sku_id"]
            .as_str()
    };
    guild
        .and_then(|_| in_profile(&value["guild_member_profile"]))
        .or_else(|| in_profile(&value["user_profile"]))
}

/// The frame in a collectible Discord describes. Its layers are `{id, type,
/// order, anchor}` and show from `/media/v1/collectibles-shop/{sku}/{id}/static`,
/// the back ones first, each kept in its order.
pub fn frame(product: &Value) -> Option<Frame> {
    let item = product["items"]
        .as_array()?
        .iter()
        .find(|item| item["layers"].is_array())?;
    let sku = text(&item["sku_id"]).or_else(|| text(&product["sku_id"]))?;
    let number = |value: &Value| value.as_u64().and_then(|n| u32::try_from(n).ok());
    let mut layers: Vec<FrameLayer> = item["layers"]
        .as_array()?
        .iter()
        .filter_map(|layer| {
            let id = text(&layer["id"])?;
            Some(FrameLayer {
                src: format!(
                    "https://cdn.discordapp.com/media/v1/collectibles-shop/{sku}/{id}/static"
                ),
                kind: text(&layer["type"])?.to_owned(),
                anchor: text(&layer["anchor"]).unwrap_or("center").to_owned(),
                front: layer["order"] != "back",
            })
        })
        .collect();
    layers.sort_by_key(|layer| layer.front);
    Some(Frame {
        inner_width: number(&item["inner_width"]).filter(|width| *width > 0)?,
        overflow_top: number(&item["overflow_top"]).unwrap_or(0),
        overflow_bottom: number(&item["overflow_bottom"]).unwrap_or(0),
        overflow_horizontal: number(&item["overflow_horizontal"]).unwrap_or(0),
        layers,
    })
}

/// A past thing on someone's Activity tab.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentEntry {
    pub title: String,
    pub subtitle: String,
    /// The address of its picture.
    pub picture: Option<String>,
    /// It's music, marked with a note.
    pub music: bool,
    /// When it ended, in milliseconds.
    pub at: Option<u64>,
}

/// What's in someone's outbox that this shows: music, what they watched and
/// games they played, by the client's numbers for `content_type`.
pub fn recent_entries(value: &Value) -> Vec<RecentEntry> {
    const PLAYED_GAME: u64 = 1;
    const WATCHED_MEDIA: u64 = 2;
    const TOP_GAME: u64 = 3;
    const LISTENED_SESSION: u64 = 5;
    value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let extra = &entry["extra"];
            let at = entry["ended_at"]
                .as_str()
                .or_else(|| entry["started_at"].as_str())
                .and_then(iso_ms)
                // A session has no times of its own; its id says when it began.
                .or_else(|| {
                    text(&entry["id"])
                        .map(crate::message::snowflake_ms)
                        .filter(|ms| *ms > 0)
                });
            match entry["content_type"].as_u64()? {
                LISTENED_SESSION => {
                    let media = &extra["entries"][0]["media"];
                    Some(RecentEntry {
                        title: text(&media["title"])?.to_owned(),
                        subtitle: text(&media["artists"][0]["name"])
                            .unwrap_or_default()
                            .to_owned(),
                        picture: text(&media["image_url"]).map(str::to_owned),
                        music: true,
                        at,
                    })
                }
                WATCHED_MEDIA => Some(RecentEntry {
                    title: text(&extra["media_title"])?.to_owned(),
                    at,
                    ..Default::default()
                }),
                PLAYED_GAME | TOP_GAME => Some(RecentEntry {
                    title: text(&extra["game_name"])?.to_owned(),
                    at,
                    ..Default::default()
                }),
                _ => None,
            }
        })
        .collect()
}

/// A layer of a profile effect, on a canvas Discord measures in its own
/// pixels and times in milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EffectLayer {
    /// The address of its animated picture.
    pub src: String,
    /// When it starts, after the effect does, and how long it takes.
    pub start: u32,
    pub duration: u32,
    /// It plays again and again, waiting `loop_delay` between.
    pub looping: bool,
    pub loop_delay: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The profile effect's id in a profile, the server's before their own.
pub fn effect_sku<'a>(value: &'a Value, guild: Option<&str>) -> Option<&'a str> {
    let server =
        guild.and_then(|_| value["guild_member_profile"]["profile_effect"]["sku_id"].as_str());
    server.or_else(|| value["user_profile"]["profile_effect"]["sku_id"].as_str())
}

/// The layers of the effect in a collectible Discord describes, bottom first.
pub fn effect_layers(product: &Value) -> Vec<EffectLayer> {
    let number = |value: &Value| {
        value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    let place = |value: &Value| {
        value
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0)
    };
    let mut layers: Vec<_> = product["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["effects"].as_array())
        .flatten()
        .filter_map(|effect| {
            let src = text(&effect["src"])?.to_owned();
            let duration = number(&effect["duration"]);
            (duration > 0).then_some(())?;
            Some((
                effect["zIndex"].as_i64().unwrap_or(0),
                EffectLayer {
                    src,
                    start: number(&effect["start"]),
                    duration,
                    looping: effect["loop"] == true,
                    loop_delay: number(&effect["loopDelay"]),
                    x: place(&effect["position"]["x"]),
                    y: place(&effect["position"]["y"]),
                    width: number(&effect["width"]),
                    height: number(&effect["height"]),
                },
            ))
        })
        .collect();
    layers.sort_by_key(|(z, _)| *z);
    layers.into_iter().map(|(_, layer)| layer).collect()
}

/// A level of a badge that levels up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BadgeLevel {
    pub name: String,
    pub icon: String,
    pub owned: bool,
    /// What it takes, such as `3 months`.
    pub milestone: String,
}

/// A badge in the account's badge window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirectoryBadge {
    pub id: u64,
    pub name: String,
    pub owned: bool,
    /// Still to be had, rather than given out once.
    pub earnable: bool,
    pub description: String,
    /// What it says of this account, such as `Subscriber since 2/22/26`.
    pub info: String,
    /// 1 common, 2 rare, 3 epic, 5 mythic.
    pub rarity: u8,
    pub icon: String,
    /// Its large moving picture, when it has one Yapfast can draw.
    pub art: Option<String>,
    /// When the level it's at was reached, as a date.
    pub unlocked: Option<String>,
    /// The next level, and how far along to it, out of `span`.
    pub next_name: Option<String>,
    pub next_icon: Option<String>,
    pub done: u64,
    pub span: u64,
    pub helper: String,
    pub levels: Vec<BadgeLevel>,
    /// The level the client shows it at: the one reached, else the next, else
    /// the first; none for a badge without levels.
    pub level: Option<String>,
    /// That level is the first, which the client's tile dates differently.
    pub first_level: bool,
    /// That level's rarity, and its picture, moving if it has one Yapfast can
    /// draw, else its icon.
    pub level_rarity: u8,
    pub level_art: String,
}

/// The number the badge window knows a profile's badge by, from its `id`, as
/// the client's `legacyBadgeId` map does; the newer ones are their own names.
pub fn directory_id(id: &str) -> Option<u64> {
    Some(match id {
        "staff" | "staff_hidden" | "staff_personal" => 22,
        "premium" => 1,
        "partner" => 2,
        "certified_moderator" => 3,
        "hypesquad" => 4,
        "hypesquad_house_1" => 5,
        "hypesquad_house_2" => 6,
        "hypesquad_house_3" => 7,
        "bug_hunter_level_1" => 8,
        "bug_hunter_level_2" => 9,
        "verified_developer" => 10,
        "early_supporter" => 11,
        "legacy_username" => 13,
        "quest_completed" => 14,
        "april_fools_2026" => 15,
        "orb_profile_badge" => 16,
        "gifting" => 17,
        "account_age" => 18,
        "streaming" => 19,
        "game_time" => 20,
        "game_variety" => 21,
        id if id.starts_with("premium_tenure_") && id.ends_with("_month_v2") => 1,
        id if id.starts_with("guild_booster_lvl") => 12,
        _ => return None,
    })
}

/// The badges in Discord's answer, in its order: for `@me` every badge there
/// is, and for someone else what they own.
pub fn badge_directory(value: &Value) -> Vec<DirectoryBadge> {
    let level_name = |tier: &Value| {
        text(&tier["name"])
            .or_else(|| text(&tier["milestone_text"]))
            .unwrap_or_default()
            .to_owned()
    };
    value["badges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|badge| {
            let tiers = badge["tiers"].as_array().map_or(&[][..], Vec::as_slice);
            let tier = |key: &Value| {
                tiers
                    .iter()
                    .find(|tier| key.is_string() && tier["key"] == *key)
            };
            let next = tier(&badge["next_tier"]);
            // The client's choice of which level to show it at.
            let shown = tier(if badge["owned"] == true {
                &badge["current_tier"]
            } else {
                &badge["next_tier"]
            })
            .or(tiers.first());
            let rarity = badge["rarity"]
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let icon = text(&badge["simple_icon_raster_url"]).unwrap_or_default();
            let progress = &badge["progress"][0];
            let (floor, threshold, current) = (
                progress["floor"].as_u64().unwrap_or(0),
                progress["threshold"].as_u64().unwrap_or(0),
                progress["current"].as_u64().unwrap_or(0),
            );
            let obtained = badge["current_tier"]
                .as_str()
                .and_then(|key| badge["tier_obtained_at"][key].as_str())
                .or_else(|| badge["obtained_at"].as_str());
            Some(DirectoryBadge {
                id: badge["badge_id"].as_u64()?,
                name: text(&badge["name"])?.to_owned(),
                owned: badge["owned"] == true,
                earnable: badge["is_earnable"] == true,
                description: text(&badge["description"]).unwrap_or_default().to_owned(),
                info: text(&badge["info_label"]).unwrap_or_default().to_owned(),
                rarity,
                icon: icon.to_owned(),
                art: text(&badge["complex_icon_animated_url"]).map(str::to_owned),
                unlocked: obtained
                    .filter(|_| badge["owned"] == true)
                    .and_then(iso_ms)
                    .map(crate::message::long_date),
                next_name: next.map(level_name),
                next_icon: next
                    .and_then(|tier| text(&tier["simple_icon_raster_url"]))
                    .map(str::to_owned),
                done: current.saturating_sub(floor),
                span: threshold.saturating_sub(floor),
                helper: text(&progress["progress_helper_text"])
                    .unwrap_or_default()
                    .to_owned(),
                levels: tiers
                    .iter()
                    .map(|tier| BadgeLevel {
                        name: level_name(tier),
                        icon: text(&tier["simple_icon_raster_url"])
                            .unwrap_or_default()
                            .to_owned(),
                        owned: tier["owned"] == true,
                        milestone: text(&tier["milestone_text"]).unwrap_or_default().to_owned(),
                    })
                    .collect(),
                level: shown.map(level_name),
                first_level: shown.is_none_or(|shown| tiers.first() == Some(shown)),
                level_rarity: shown
                    .and_then(|tier| tier["rarity"].as_u64())
                    .and_then(|n| u8::try_from(n).ok())
                    .unwrap_or(rarity),
                level_art: shown
                    .and_then(|tier| text(&tier["complex_icon_animated_url"]))
                    .or_else(|| text(&badge["complex_icon_animated_url"]))
                    .or_else(|| shown.and_then(|tier| text(&tier["simple_icon_raster_url"])))
                    .unwrap_or(icon)
                    .to_owned(),
            })
        })
        .collect()
}

/// A badge on a profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Badge {
    /// What it says it is, such as "Early Supporter".
    pub description: String,
    /// The address of its picture.
    pub icon: String,
    /// Where clicking it goes, when it goes anywhere.
    pub link: Option<String>,
    /// The number the badge window knows it by, when it knows it.
    #[serde(default)]
    pub directory: Option<u64>,
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

/// A text field that may be missing, null or empty.
fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

/// A profile's badges, leaving out any without a picture.
fn badges(value: &Value) -> impl Iterator<Item = Badge> + '_ {
    value.as_array().into_iter().flatten().filter_map(|badge| {
        Some(Badge {
            description: text(&badge["description"]).unwrap_or_default().to_owned(),
            icon: ready::badge_icon(text(&badge["icon"])?),
            link: text(&badge["link"]).map(str::to_owned),
            directory: text(&badge["id"]).and_then(directory_id),
        })
    })
}

/// A profile's two theme colours, when it has both.
fn theme_colors(value: &Value) -> Option<[u32; 2]> {
    let [top, bottom] = value.as_array()?.as_slice() else {
        return None;
    };
    Some([color(top)?, color(bottom)?])
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
        (Some(guild), Some(hash)) => ready::profile_member_avatar(guild, &id, hash),
        _ => ready::profile_avatar(&id, user["avatar"].as_str()),
    };
    let decoration = text(&member["avatar_decoration_data"]["asset"])
        .or_else(|| text(&user["avatar_decoration_data"]["asset"]))
        .map(ready::avatar_decoration);
    let banner = match (guild, text(&server["banner"])) {
        (Some(guild), Some(hash)) => Some(ready::member_banner(guild, &id, hash)),
        _ => text(&own["banner"])
            .or_else(|| text(&user["banner"]))
            .map(|hash| ready::banner(&id, hash)),
    };
    let member_badges = match guild {
        Some(_) => value["guild_badges"].clone(),
        None => Value::Null,
    };
    Some(Profile {
        name,
        username: user["username"].as_str().unwrap_or_default().to_owned(),
        avatar,
        decoration,
        banner,
        accent: color(&server["accent_color"])
            .or_else(|| color(&own["accent_color"]))
            .or_else(|| color(&user["accent_color"])),
        theme: theme_colors(&server["theme_colors"]).or_else(|| theme_colors(&own["theme_colors"])),
        tag: ready::server_tag(user),
        name_style: ready::name_style(user),
        effect: Vec::new(),
        badges: badges(&value["badges"])
            .chain(badges(&member_badges))
            .collect(),
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
        widgets: crate::widgets::parse(value),
        wishlist: crate::wishlist::place(value),
        bot: user["bot"] == true,
        frame: None,
        effect_sku: effect_sku(value, guild).map(str::to_owned),
        frame_sku: frame_sku(value, guild).map(str::to_owned),
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
    fn a_badge_shows_at_its_level_and_one_without_levels_at_its_own_name() {
        let value = json!({"badges": [
            {
                "badge_id": 20, "name": "Game Time", "owned": true, "rarity": 1,
                "is_earnable": true, "current_tier": "b", "next_tier": "c",
                "simple_icon_raster_url": "https://icon",
                "complex_icon_static_url": "https://art.svg",
                "tier_obtained_at": {"b": "2026-10-27T00:00:00+00:00"},
                "tiers": [
                    {"key": "a", "name": "Casual", "owned": true, "rarity": 1, "milestone_text": "1 hour", "simple_icon_raster_url": "https://a"},
                    {"key": "b", "name": "Ironclad", "owned": true, "rarity": 3, "milestone_text": "2 hours", "simple_icon_raster_url": "https://b"},
                    {"key": "c", "name": "Eternal", "owned": false, "rarity": 5, "milestone_text": "3 hours", "simple_icon_raster_url": "https://c"},
                ],
            },
            {
                "badge_id": 14, "name": "Quests", "owned": true, "rarity": 1,
                "simple_icon_raster_url": "https://quests",
            },
        ]});
        let badges = badge_directory(&value);
        assert_eq!(badges[0].level.as_deref(), Some("Ironclad"));
        assert!(!badges[0].first_level);
        assert_eq!(badges[0].level_rarity, 3);
        // An SVG can't be drawn, so the level's raster stands in.
        assert_eq!(badges[0].level_art, "https://b");
        assert!(badges[0].unlocked.as_deref().unwrap().ends_with(", 2026"));
        assert_eq!(badges[1].level, None);
        assert_eq!(badges[1].unlocked, None);
        assert_eq!(badges[1].level_art, "https://quests");
    }

    #[test]
    fn discord_numbers_a_frames_collectible_and_dates_a_session_by_its_id() {
        let profile = json!({"user_profile": {"collectibles": [
            {"expires_at": null, "sku_id": "1", "type": 1},
            {"expires_at": null, "sku_id": "3", "type": 3},
        ]}});
        assert_eq!(frame_sku(&profile, None), Some("3"));
        let outbox = json!({"entries": [{
            "id": "1522378878083924051", "content_type": 5,
            "expires_at": "2026-11-04T23:42:33+00:00",
            "extra": {"last_update": "2026-11-04T23:12:33+00:00", "entries": [
                {"media": {"title": "Song", "artists": [{"name": "Band"}]}},
            ]},
        }]});
        let entries = recent_entries(&outbox);
        assert_eq!(
            entries[0].at,
            Some(crate::message::snowflake_ms("1522378878083924051"))
        );
    }

    #[test]
    fn a_frame_comes_from_its_collectible_with_the_back_layers_first() {
        let profile = json!({"user_profile": {"collectibles": [
            {"type": "profile_effect", "sku_id": "8"},
            {"type": "profile_frame", "sku_id": "9"},
        ]}});
        assert_eq!(frame_sku(&profile, None), Some("9"));
        let frame = frame(&json!({"sku_id": "9", "items": [{
            "sku_id": "9", "inner_width": 1200, "overflow_top": 304, "overflow_bottom": 212,
            "overflow_horizontal": 56,
            "layers": [
                {"id": "a", "type": "staple", "order": "front", "anchor": "top"},
                {"id": "b", "type": "border", "order": "back"},
            ],
        }]}))
        .unwrap();
        assert_eq!(frame.inner_width, 1200);
        assert_eq!(frame.layers[0].kind, "border");
        assert!(frame.layers[1].front);
        assert!(
            frame.layers[1]
                .src
                .ends_with("/media/v1/collectibles-shop/9/a/static")
        );
    }

    #[test]
    fn recent_activity_reads_songs_and_games_and_leaves_the_rest() {
        let entries = recent_entries(&json!({"entries": [
            {"content_type": 5, "ended_at": "2026-10-27T00:00:00+00:00", "extra": {"entries": [{"media": {
                "title": "A song", "artists": [{"name": "A band"}], "image_url": "https://art",
            }}]}},
            {"content_type": 1, "extra": {"game_name": "A game"}},
            {"content_type": 7, "extra": {}},
        ]}));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].subtitle, "A band");
        assert!(entries[0].music && entries[0].at.is_some());
        assert_eq!(entries[1].title, "A game");
    }

    #[test]
    fn a_profiles_badges_know_their_number_in_the_window() {
        for (id, number) in [
            ("quest_completed", Some(14)),
            ("premium_tenure_12_month_v2", Some(1)),
            ("guild_booster_lvl3", Some(12)),
            ("game_time", Some(20)),
            ("something_new", None),
        ] {
            assert_eq!(directory_id(id), number, "{id}");
        }
    }

    #[test]
    fn badges_come_with_pictures_and_the_servers_only_in_a_server() {
        let value = json!({
            "user": {"id": "1", "username": "alice"},
            "badges": [
                {"id": "a", "description": "Early", "icon": "abc", "link": "https://x.test"},
                {"id": "b", "description": "No picture"},
            ],
            "guild_badges": [{"id": "c", "description": "Guild", "icon": "def"}],
        });
        let outside = parse(&value, None).unwrap().badges;
        assert_eq!(outside.len(), 1);
        assert_eq!(outside[0].description, "Early");
        assert!(outside[0].icon.ends_with("/badge-icons/abc.png"));
        assert_eq!(outside[0].link.as_deref(), Some("https://x.test"));
        let inside = parse(&value, Some("9")).unwrap().badges;
        assert_eq!(inside.len(), 2);
        assert_eq!(inside[1].description, "Guild");
        assert_eq!(inside[1].link, None);
    }

    #[test]
    fn the_badge_window_reads_levels_and_progress() {
        let value = serde_json::json!({"badges": [
            {"badge_id": 1, "name": "Nitro", "owned": true, "is_earnable": true, "rarity": 2,
             "simple_icon_raster_url": "https://x/n.png", "current_tier": "gold", "next_tier": "platinum",
             "info_label": "Subscriber since 2/22/26",
             "obtained_at": "2026-02-22T17:49:00+00:00",
             "tier_obtained_at": {"gold": "2026-08-22T17:49:00+00:00"},
             "progress": [{"current": 255, "floor": 181, "threshold": 365, "progress_helper_text": "4 more months"}],
             "tiers": [
                {"key": "gold", "name": "Gold", "owned": true, "simple_icon_raster_url": "https://x/g.png", "milestone_text": "6 months"},
                {"key": "platinum", "name": "Platinum", "owned": false, "simple_icon_raster_url": "https://x/p.png"},
             ]},
            {"badge_id": 12, "name": "Server Boosting", "owned": false, "is_earnable": true,
             "next_tier": "level_1", "tiers": [{"key": "level_1", "milestone_text": "1 month"}]},
            {"name": "No id"},
        ]});
        let badges = badge_directory(&value);
        assert_eq!(badges.len(), 2);
        let nitro = &badges[0];
        assert_eq!(nitro.next_name.as_deref(), Some("Platinum"));
        assert_eq!((nitro.done, nitro.span), (74, 184));
        assert_eq!(nitro.helper, "4 more months");
        assert!(nitro.unlocked.as_deref().unwrap().contains("2026"));
        // A level without a name goes by what it takes, and nothing's unlocked yet.
        assert_eq!(badges[1].next_name.as_deref(), Some("1 month"));
        assert_eq!(badges[1].unlocked, None);
    }

    #[test]
    fn effects_come_in_layers_in_order() {
        let product = serde_json::json!({"items": [{"effects": [
            {"src": "https://x/top", "zIndex": 101, "start": 2000, "duration": 3000, "loop": true,
             "loopDelay": 500, "position": {"x": 1, "y": -2}, "width": 450, "height": 880},
            {"src": "https://x/under", "zIndex": 100, "duration": 2000, "loop": false},
            {"src": "https://x/never", "zIndex": 1, "duration": 0},
        ]}]});
        let layers = effect_layers(&product);
        assert_eq!(
            layers.iter().map(|l| l.src.as_str()).collect::<Vec<_>>(),
            ["https://x/under", "https://x/top"]
        );
        let top = &layers[1];
        assert!(top.looping);
        assert_eq!(
            (top.start, top.duration, top.loop_delay, top.x, top.y),
            (2000, 3000, 500, 1, -2)
        );
        let profile = serde_json::json!({
            "user_profile": {"profile_effect": {"sku_id": "own"}},
            "guild_member_profile": {"profile_effect": {"sku_id": "there"}},
        });
        assert_eq!(effect_sku(&profile, None), Some("own"));
        assert_eq!(effect_sku(&profile, Some("1")), Some("there"));
        // A profile names its collectibles, and has none of their layers
        // until they're asked for.
        let parsed = parse(
            &serde_json::json!({
                "user": {"id": "1"},
                "user_profile": {
                    "profile_effect": {"sku_id": "own"},
                    "collectibles": [{"type": 3, "sku_id": "frame"}],
                },
            }),
            None,
        )
        .unwrap();
        assert_eq!(
            (parsed.effect_sku.as_deref(), parsed.frame_sku.as_deref()),
            (Some("own"), Some("frame"))
        );
        assert!(parsed.effect.is_empty() && parsed.frame.is_none());
    }

    #[test]
    fn the_avatars_frame_prefers_the_server() {
        let value = serde_json::json!({
            "user": {"id": "1", "username": "ann", "avatar_decoration_data": {"asset": "own"}},
            "guild_member": {"avatar_decoration_data": {"asset": "there"}},
        });
        let frame = |guild| parse(&value, guild).unwrap().decoration.unwrap();
        assert!(frame(None).contains("/avatar-decoration-presets/own.png"));
        assert!(frame(Some("2")).contains("/avatar-decoration-presets/there.png"));
    }

    #[test]
    fn theme_colors_prefer_the_server_and_need_both() {
        let value = |server: Value, own: Value| {
            json!({
                "user": {"id": "1", "username": "alice"},
                "user_profile": {"theme_colors": own},
                "guild_member_profile": {"theme_colors": server},
            })
        };
        let own = value(Value::Null, json!([1, 2]));
        assert_eq!(parse(&own, Some("9")).unwrap().theme, Some([1, 2]));
        let both = value(json!([3, 4]), json!([1, 2]));
        assert_eq!(parse(&both, Some("9")).unwrap().theme, Some([3, 4]));
        assert_eq!(parse(&both, None).unwrap().theme, Some([1, 2]));
        let half = value(Value::Null, json!([1]));
        assert_eq!(parse(&half, None).unwrap().theme, None);
    }
}
