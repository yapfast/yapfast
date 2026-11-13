//! A Discord server's settings, as its Server Settings pages show them,
//! read from Discord's answers. Each page's are fetched when it opens and
//! dropped when it closes; see "Server settings" in docs/discord/server-settings.md.

use serde_json::Value;

use crate::ready;

/// A server's profile, as Server Profile shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerProfile {
    pub name: String,
    pub description: String,
    /// The address of its icon, when it has one.
    pub icon: Option<String>,
}

/// A server's profile from `GET /guilds/{id}/profile`.
pub fn parse_profile(guild: &str, profile: &Value) -> ServerProfile {
    ServerProfile {
        name: profile["name"].as_str().unwrap_or_default().to_owned(),
        description: profile["description"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        icon: profile["icon_hash"]
            .as_str()
            .map(|hash| ready::server_icon(guild, hash)),
    }
}

/// What a server's Engagement and Boost Perks pages show, from `GET
/// /guilds/{id}` or a GUILD_UPDATE.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildSettings {
    /// The channel system messages go to, by id.
    pub system_channel: Option<String>,
    /// Which system messages Discord leaves out: Discord's
    /// `system_channel_flags`, each bit suppressing one.
    pub system_flags: u64,
    /// Whether members who set nothing are notified of every message: 0,
    /// or only of mentions: 1.
    pub default_notifications: u8,
    /// The voice channel idle members move to, by id, and after how many
    /// seconds.
    pub afk_channel: Option<String>,
    pub afk_timeout: u32,
    /// The addresses of its banner and invite background, when set.
    pub banner: Option<String>,
    pub splash: Option<String>,
    pub progress_bar: bool,
    /// Its boost level, 0 to 3, and Discord's features, which the level
    /// unlocks, such as `BANNER`.
    pub premium_tier: u8,
    pub features: Vec<String>,
    /// What Safety Setup sets: what members must have done before they
    /// may talk, 0 none to 4 a verified phone; whose images Discord
    /// filters, 0 no one, 1 those without roles, 2 everyone; and whether
    /// moderators need two-factor authentication, 1, or not, 0.
    pub verification_level: u8,
    pub explicit_content_filter: u8,
    pub mfa_level: u8,
    /// When its paused invites and DMs resume, in Unix seconds, from its
    /// `incidents_data`.
    pub invites_paused_until: Option<u64>,
    pub dms_paused_until: Option<u64>,
    /// The emoji slots its boosts add to each kind's 50, from its
    /// `premium_features`, when Discord sends them.
    pub extra_emoji_slots: Option<u16>,
}

impl GuildSettings {
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|known| known == feature)
    }
}

/// A server's settings from its guild object, whose fields come at its top
/// or, as READY has them, under `properties`.
pub fn parse_guild(guild: &Value) -> GuildSettings {
    let id = guild["id"].as_str().unwrap_or_default();
    let field = |key: &str| match &guild[key] {
        Value::Null => &guild["properties"][key],
        found => found,
    };
    let text = |key: &str| field(key).as_str().map(str::to_owned);
    GuildSettings {
        system_channel: text("system_channel_id"),
        system_flags: field("system_channel_flags").as_u64().unwrap_or_default(),
        default_notifications: field("default_message_notifications")
            .as_u64()
            .unwrap_or_default() as u8,
        afk_channel: text("afk_channel_id"),
        afk_timeout: field("afk_timeout").as_u64().unwrap_or(300) as u32,
        banner: field("banner").as_str().map(|hash| ready::banner(id, hash)),
        splash: field("splash").as_str().map(|hash| ready::splash(id, hash)),
        progress_bar: field("premium_progress_bar_enabled")
            .as_bool()
            .unwrap_or_default(),
        premium_tier: field("premium_tier").as_u64().unwrap_or_default() as u8,
        features: field("features")
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|feature| Some(feature.as_str()?.to_owned()))
            .collect(),
        verification_level: field("verification_level").as_u64().unwrap_or_default() as u8,
        explicit_content_filter: field("explicit_content_filter")
            .as_u64()
            .unwrap_or_default() as u8,
        mfa_level: field("mfa_level").as_u64().unwrap_or_default() as u8,
        invites_paused_until: parse_incidents(field("incidents_data")).0,
        dms_paused_until: parse_incidents(field("incidents_data")).1,
        extra_emoji_slots: field("premium_features")["additional_emoji_slots"]
            .as_u64()
            .map(|slots| u16::try_from(slots).unwrap_or(u16::MAX)),
    }
}

/// When a server's paused invites and DMs resume, in Unix seconds, from
/// its `incidents_data`.
pub fn parse_incidents(incidents: &Value) -> (Option<u64>, Option<u64>) {
    let until = |key: &str| {
        incidents[key]
            .as_str()
            .and_then(crate::message::iso_ms)
            .map(|ms| ms / 1000)
    };
    (until("invites_disabled_until"), until("dms_disabled_until"))
}

/// An invite to a server, as Invites in its settings lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invite {
    pub code: String,
    pub inviter: Option<BannedUser>,
    /// The channel it leads to, by id and name.
    pub channel: Option<(String, String)>,
    pub uses: u64,
    /// Its limit, 0 for none.
    pub max_uses: u64,
    /// When it expires, in Unix seconds; none for never.
    pub expires: Option<u64>,
    /// The roles it gives, by id.
    pub roles: Vec<String>,
    /// It skips join applications: Discord's `IS_APPLICATION_BYPASS` flag.
    pub bypass: bool,
}

/// An invite from `GET /guilds/{id}/invites`.
pub fn parse_invite(invite: &Value) -> Option<Invite> {
    let created = invite["created_at"]
        .as_str()
        .and_then(crate::message::iso_ms)
        .unwrap_or_default()
        / 1000;
    let max_age = invite["max_age"].as_u64().unwrap_or_default();
    Some(Invite {
        code: invite["code"].as_str()?.to_owned(),
        inviter: parse_banned_user(&invite["inviter"]),
        channel: invite["channel"]["id"].as_str().map(|id| {
            let name = invite["channel"]["name"].as_str().unwrap_or_default();
            (id.to_owned(), name.to_owned())
        }),
        uses: invite["uses"].as_u64().unwrap_or_default(),
        max_uses: invite["max_uses"].as_u64().unwrap_or_default(),
        expires: (max_age > 0).then_some(created + max_age),
        // Ids, or role objects.
        roles: invite["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|role| role.as_str().or_else(|| role["id"].as_str()))
            .map(str::to_owned)
            .collect(),
        bypass: invite["flags"].as_u64().unwrap_or_default() & INVITE_BYPASS != 0,
    })
}

/// Discord's invite flag for an invite that skips join applications.
const INVITE_BYPASS: u64 = 1 << 3;

/// A member as Members' search finds them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoundMember {
    pub user: BannedUser,
    pub nick: Option<String>,
    pub roles: Vec<String>,
    /// When they joined the server, in Unix milliseconds.
    pub joined_at: u64,
    /// Until when they're timed out, and when Discord last saw them send
    /// unusual DMs, in Unix milliseconds; 0 for neither.
    pub timed_out_until: u64,
    pub unusual_dms_until: u64,
    /// Discord's member `flags`, which mark a quarantined name.
    pub flags: u64,
    /// Discord flagged the account for spam.
    pub spammer: bool,
    /// A bot's account, which can't be given a server.
    pub bot: bool,
    /// How they joined: Discord's `join_source_type`, the invite's code,
    /// and an integration's type.
    pub join_source: u8,
    pub invite_code: Option<String>,
    pub integration: Option<String>,
}

/// Discord's user flag for an account it suspects of spam.
const SPAMMER: u64 = 1 << 20;

/// The members of a `members-search` answer, and how many match in all.
pub fn parse_members_search(found: &Value) -> (Vec<FoundMember>, Option<u64>) {
    let time = |value: &Value| {
        value
            .as_str()
            .and_then(crate::message::iso_ms)
            .unwrap_or_default()
    };
    let members = found["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|found| {
            let member = &found["member"];
            let user = &member["user"];
            let user_flags = user["public_flags"]
                .as_u64()
                .or_else(|| user["flags"].as_u64())
                .unwrap_or_default();
            Some(FoundMember {
                user: parse_banned_user(user)?,
                nick: member["nick"]
                    .as_str()
                    .filter(|nick| !nick.is_empty())
                    .map(str::to_owned),
                roles: member["roles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|role| Some(role.as_str()?.to_owned()))
                    .collect(),
                joined_at: time(&member["joined_at"]),
                timed_out_until: time(&member["communication_disabled_until"]),
                unusual_dms_until: time(&member["unusual_dm_activity_until"]),
                flags: member["flags"].as_u64().unwrap_or_default(),
                spammer: user_flags & SPAMMER != 0,
                bot: user["bot"].as_bool().unwrap_or_default(),
                join_source: found["join_source_type"].as_u64().unwrap_or_default() as u8,
                invite_code: found["source_invite_code"].as_str().map(str::to_owned),
                integration: found["integration_type"].as_str().map(str::to_owned),
            })
        })
        .collect();
    (members, found["total_result_count"].as_u64())
}

/// Someone banned from a server, as Bans lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ban {
    pub user: BannedUser,
    /// Why, as the moderator gave it; none from a GUILD_BAN_ADD.
    pub reason: Option<String>,
}

/// A banned user, as the ban gives them: banned users may share no server
/// with this account, so their names aren't learned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BannedUser {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
    /// The address of their avatar, or Discord's default one.
    pub avatar: String,
}

/// A ban from `GET /guilds/{id}/bans`, its search, or a GUILD_BAN_ADD,
/// whose `user` Discord's client needs; one without is skipped.
pub fn parse_ban(ban: &Value) -> Option<Ban> {
    Some(Ban {
        user: parse_banned_user(&ban["user"])?,
        reason: ban["reason"]
            .as_str()
            .filter(|reason| !reason.is_empty())
            .map(str::to_owned),
    })
}

/// A user object, as a ban carries it.
pub fn parse_banned_user(user: &Value) -> Option<BannedUser> {
    let id = user["id"].as_str()?;
    Some(BannedUser {
        id: id.to_owned(),
        username: user["username"].as_str().unwrap_or_default().to_owned(),
        global_name: user["global_name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        avatar: ready::avatar(id, user["avatar"].as_str()),
    })
}

/// A server's custom emoji, as Emoji in its settings lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildEmoji {
    pub id: String,
    pub name: String,
    pub animated: bool,
    /// A server that lost boosts keeps emoji it no longer has room for,
    /// unavailable.
    pub available: bool,
    /// Who uploaded it, which `GET /guilds/{id}/emojis` says and the
    /// gateway doesn't.
    pub user: Option<BannedUser>,
}

/// An emoji from `GET /guilds/{id}/emojis` or its `POST`.
pub fn parse_emoji(emoji: &Value) -> Option<GuildEmoji> {
    Some(GuildEmoji {
        id: emoji["id"].as_str()?.to_owned(),
        name: emoji["name"].as_str()?.to_owned(),
        animated: emoji["animated"].as_bool().unwrap_or_default(),
        available: emoji["available"].as_bool().unwrap_or(true),
        user: parse_banned_user(&emoji["user"]),
    })
}

/// A server's sticker, as Stickers in its settings lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildSticker {
    pub id: String,
    pub name: String,
    /// Its related emoji: a Unicode emoji, or a custom emoji's id.
    pub tags: String,
    pub description: String,
    /// Discord's `format_type`: 1 PNG, 2 APNG, 3 Lottie, 4 GIF.
    pub format: u8,
    pub available: bool,
    /// Who uploaded it: a user, or, as the gateway has it, only an id.
    pub user: Option<BannedUser>,
    pub user_id: Option<String>,
}

/// A sticker from `GET /guilds/{id}/stickers`, its `POST` or `PATCH`, or
/// GUILD_STICKERS_UPDATE.
pub fn parse_sticker(sticker: &Value) -> Option<GuildSticker> {
    let user = parse_banned_user(&sticker["user"]);
    Some(GuildSticker {
        id: sticker["id"].as_str()?.to_owned(),
        name: sticker["name"].as_str()?.to_owned(),
        tags: sticker["tags"].as_str().unwrap_or_default().to_owned(),
        description: sticker["description"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        format: sticker["format_type"].as_u64().unwrap_or(1) as u8,
        available: sticker["available"].as_bool().unwrap_or(true),
        user_id: user
            .as_ref()
            .map(|user| user.id.clone())
            .or_else(|| sticker["user_id"].as_str().map(str::to_owned)),
        user,
    })
}

/// A role in full, as Roles in a server's settings shows and edits it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub permissions: u64,
    /// Higher roles list first; @everyone is 0.
    pub position: i64,
    /// Its colour, 0 for none, and a gradient's second and third, which
    /// Yapfast keeps as they are.
    pub color: u32,
    pub secondary_color: Option<u32>,
    pub tertiary_color: Option<u32>,
    /// Listed apart from online members.
    pub hoist: bool,
    pub mentionable: bool,
    /// What gives the role, when not people: a bot, an integration or
    /// boosting. Nobody grants it by hand.
    pub managed: Option<Managed>,
    /// Its icon's address, or a Unicode emoji in its place.
    pub icon: Option<String>,
    pub unicode_emoji: Option<String>,
}

/// What manages a role, from its `tags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Managed {
    Bot,
    Booster,
    Integration,
}

/// A role object from `GET /guilds/{id}/roles` or a GUILD_ROLE_* event.
pub fn parse_role(role: &Value) -> Option<Role> {
    let id = role["id"].as_str()?.to_owned();
    let tags = &role["tags"];
    let managed = role["managed"].as_bool().unwrap_or_default().then(|| {
        if tags.get("bot_id").is_some() {
            Managed::Bot
        } else if tags.get("premium_subscriber").is_some() {
            Managed::Booster
        } else {
            Managed::Integration
        }
    });
    let colors = &role["colors"];
    let color = |key: &str| colors[key].as_u64().map(|color| color as u32);
    Some(Role {
        name: role["name"].as_str().unwrap_or_default().to_owned(),
        permissions: role["permissions"]
            .as_str()
            .and_then(|bits| bits.parse().ok())
            .unwrap_or_default(),
        position: role["position"].as_i64().unwrap_or_default(),
        color: color("primary_color")
            .or_else(|| role["color"].as_u64().map(|color| color as u32))
            .unwrap_or_default(),
        secondary_color: color("secondary_color"),
        tertiary_color: color("tertiary_color"),
        hoist: role["hoist"].as_bool().unwrap_or_default(),
        mentionable: role["mentionable"].as_bool().unwrap_or_default(),
        managed,
        icon: role["icon"]
            .as_str()
            .map(|hash| ready::role_icon(&id, hash)),
        unicode_emoji: role["unicode_emoji"]
            .as_str()
            .filter(|emoji| !emoji.is_empty())
            .map(str::to_owned),
        id,
    })
}

/// What Roles shows: a server's roles, how many members hold each, and
/// the boost level and features that decide what a role may have.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roles {
    /// Highest first, @everyone last.
    pub roles: Vec<Role>,
    /// By role id; @everyone isn't counted.
    pub counts: std::collections::HashMap<String, u64>,
    pub premium_tier: u8,
    pub features: Vec<String>,
}

/// Roles from a server's guild object and Discord's member counts.
pub fn parse_roles_page(guild: &Value, counts: &Value) -> Roles {
    let settings = parse_guild(guild);
    Roles {
        roles: parse_roles(&guild["roles"]),
        counts: counts
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(role, count)| Some((role.clone(), count.as_u64()?)))
            .collect(),
        premium_tier: settings.premium_tier,
        features: settings.features,
    }
}

/// A server's roles, highest first, @everyone last, as Roles lists them.
pub fn parse_roles(roles: &Value) -> Vec<Role> {
    let mut roles: Vec<Role> = roles
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_role)
        .collect();
    roles.sort_by(|a, b| b.position.cmp(&a.position).then(a.id.cmp(&b.id)));
    roles
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roles_read_highest_first_with_what_manages_them() {
        let roles = parse_roles(&json!([
            {"id": "1", "name": "@everyone", "permissions": "1024", "position": 0, "color": 0},
            {"id": "3", "name": "Bot", "position": 2, "managed": true, "tags": {"bot_id": "9"},
             "colors": {"primary_color": 255, "secondary_color": 65280, "tertiary_color": null}},
            {"id": "2", "name": "Mods", "permissions": "8192", "position": 1, "color": 3447003,
             "hoist": true, "icon": "abc", "unicode_emoji": ""},
        ]));
        let ids: Vec<&str> = roles.iter().map(|role| role.id.as_str()).collect();
        assert_eq!(ids, ["3", "2", "1"]);
        assert_eq!(roles[0].managed, Some(Managed::Bot));
        assert_eq!(
            (
                roles[0].color,
                roles[0].secondary_color,
                roles[0].tertiary_color
            ),
            (255, Some(65280), None)
        );
        assert_eq!((roles[1].permissions, roles[1].color), (8192, 3447003));
        assert!(roles[1].hoist && roles[1].unicode_emoji.is_none());
        assert!(
            roles[1]
                .icon
                .as_deref()
                .unwrap()
                .contains("/role-icons/2/abc.png")
        );
    }

    #[test]
    fn a_profile_reads_as_server_profile_shows_it() {
        let profile = parse_profile(
            "1",
            &json!({"id": "1", "name": "Crew", "icon_hash": "abc", "description": null}),
        );
        assert_eq!(profile.name, "Crew");
        assert_eq!(profile.description, "");
        assert!(profile.icon.unwrap().ends_with("/icons/1/abc.png?size=128"));
    }

    #[test]
    fn a_guild_reads_from_its_top_or_its_properties() {
        let settings = parse_guild(&json!({"id": "1", "properties": {
            "system_channel_id": "11", "system_channel_flags": 5,
            "default_message_notifications": 1, "afk_channel_id": null,
            "afk_timeout": 900, "banner": "b", "splash": null,
            "premium_progress_bar_enabled": true, "premium_tier": 2,
            "features": ["BANNER", "INVITE_SPLASH"],
        }}));
        assert_eq!(settings.system_channel.as_deref(), Some("11"));
        assert_eq!(
            (settings.system_flags, settings.default_notifications),
            (5, 1)
        );
        assert_eq!(
            (settings.afk_channel.as_deref(), settings.afk_timeout),
            (None, 900)
        );
        assert!(
            settings
                .banner
                .as_deref()
                .unwrap()
                .ends_with("/banners/1/b.png?size=600")
        );
        assert!(settings.progress_bar && settings.has("BANNER"));
        let update = parse_guild(&json!({"id": "1", "premium_tier": 1, "afk_timeout": 60}));
        assert_eq!((update.premium_tier, update.afk_timeout), (1, 60));
    }
}
