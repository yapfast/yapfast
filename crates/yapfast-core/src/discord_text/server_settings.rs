//! A Discord server's settings: fetching a page as it opens, reading a
//! picture chosen for the server, and saving what changed; and the
//! two-factor prompts Discord asks of moderation and of the settings. See
//! "Server settings" in docs/discord/server-settings.md.

use serde_json::{Map, Value};
use yapfast_discord::{
    mfa,
    server_settings::{
        Ban as DiscordBan, GuildSettings as DiscordSettings, ServerProfile as DiscordProfile,
    },
};

use super::{
    commands::NOT_DISCORD,
    own_profile::{picture, read_shaped},
    *,
};
use crate::chat::{
    Ban, BansAction, GuildSettings, MfaMethod, RoleMember, ServerPicture, ServerProfile,
    ServerSettingsChange, ServerSettingsPage, SettingsSection,
};

/// A server icon as Discord's client crops it, and its preview at the
/// size Server Profile shows it.
const ICON: (u32, u32) = (1024, 1024);
const ICON_PREVIEW: (u32, u32) = (128, 128);
/// A banner and an invite background, 16 by 9 as Discord recommends them,
/// and their previews at the size Boost Perks shows them.
const BACKGROUND: (u32, u32) = (1920, 1080);
const BACKGROUND_PREVIEW: (u32, u32) = (480, 270);
/// A role icon, square, well over the 64 pixels Discord recommends and
/// under its 256 KB, and its preview at the size the editor shows it.
const ROLE_ICON: (u32, u32) = (128, 128);
const ROLE_ICON_PREVIEW: (u32, u32) = (64, 64);

/// Carries out a command about a server's settings or a two-factor prompt,
/// or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(command) = command else {
        return Some(command);
    };
    let events = events.clone();
    match command {
        DiscordCommand::FinishMfa { id, method, code } => {
            let discord = match discord {
                Ok(discord) => discord.clone(),
                Err(why) => {
                    let _ = events.send(Event::MfaFinished {
                        id,
                        result: Err(why.into()),
                    });
                    return None;
                }
            };
            tokio::spawn(async move {
                let result = discord
                    .finish_mfa(id, discord_method(method), &code)
                    .await
                    .map_err(|error| {
                        tracing::info!(
                            error = format!("{error:#}"),
                            "Discord refused a 2FA confirmation"
                        );
                        error.to_string()
                    });
                let _ = events.send(Event::MfaFinished { id, result });
            });
        }
        DiscordCommand::TextMfa(id) => {
            let discord = match discord {
                Ok(discord) => discord.clone(),
                Err(why) => {
                    let _ = events.send(Event::MfaTexted {
                        id,
                        result: Err(why.into()),
                    });
                    return None;
                }
            };
            tokio::spawn(async move {
                let result = discord
                    .send_mfa_sms(id)
                    .await
                    .map_err(|error| format!("Discord didn't send a code: {error}"));
                let _ = events.send(Event::MfaTexted { id, result });
            });
        }
        DiscordCommand::CancelMfa(id) => {
            if let Ok(discord) = discord {
                discord.cancel_mfa(id);
            }
        }
        DiscordCommand::ServerSettings { server, section } => {
            let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
                let why = discord.err().unwrap_or(NOT_DISCORD);
                let _ = events.send(Event::ServerSettingsLoaded {
                    server,
                    section,
                    result: Err(why.into()),
                });
                return None;
            };
            let (discord, guild) = (discord.clone(), guild.to_owned());
            tokio::spawn(async move {
                let result = match section {
                    SettingsSection::Profile => discord
                        .guild_profile(&guild)
                        .await
                        .map(|profile| ServerSettingsPage::Profile(server_profile(profile))),
                    SettingsSection::Engagement
                    | SettingsSection::BoostPerks
                    | SettingsSection::Safety => discord
                        .guild_settings(&guild)
                        .await
                        .map(|settings| ServerSettingsPage::Guild(guild_settings(settings))),
                    SettingsSection::Roles => discord
                        .roles_page(&guild)
                        .await
                        .map(|roles| ServerSettingsPage::Roles(super::roles::page(roles))),
                    SettingsSection::AuditLog => discord
                        .audit_log(&guild, None, None, None)
                        .await
                        .map(|log| ServerSettingsPage::AuditLog(super::audit_log::parse(&log))),
                    SettingsSection::Members => {
                        let query = Default::default();
                        super::people::members(&discord, &guild, &server, &query, &events)
                            .await
                            .map(|(members, total)| ServerSettingsPage::Members { members, total })
                            .map_err(anyhow::Error::msg)
                    }
                    SettingsSection::Emoji => {
                        let (emojis, settings) = tokio::join!(
                            super::expressions::emojis(&discord, &guild),
                            discord.guild_settings(&guild)
                        );
                        emojis.and_then(|emojis| {
                            Ok(ServerSettingsPage::Emoji {
                                emojis,
                                settings: guild_settings(settings?),
                            })
                        })
                    }
                    SettingsSection::Stickers => {
                        let (stickers, settings) = tokio::join!(
                            super::expressions::stickers(&discord, &guild),
                            discord.guild_settings(&guild)
                        );
                        stickers.and_then(|stickers| {
                            Ok(ServerSettingsPage::Stickers {
                                stickers,
                                settings: guild_settings(settings?),
                            })
                        })
                    }
                    SettingsSection::Invites => {
                        let (invites, settings) = tokio::join!(
                            super::people::list(&discord, &guild),
                            discord.guild_settings(&guild)
                        );
                        match (invites, settings) {
                            (Ok(invites), Ok(settings)) => Ok(ServerSettingsPage::Invites {
                                invites,
                                settings: guild_settings(settings),
                            }),
                            (Err(error), _) => Err(anyhow::Error::msg(error)),
                            (_, Err(error)) => Err(error),
                        }
                    }
                    SettingsSection::Bans => discord
                        .bans(&guild, None)
                        .await
                        .map(|bans| ServerSettingsPage::Bans(bans.into_iter().map(ban).collect())),
                };
                let result = result.map_err(|error| {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "could not fetch server settings"
                    );
                    format!("Discord didn't send the server's settings: {error}")
                });
                let _ = events.send(Event::ServerSettingsLoaded {
                    server,
                    section,
                    result,
                });
            });
        }
        DiscordCommand::ServerPicture { kind, path } => {
            tokio::task::spawn_blocking(move || {
                let result = match kind {
                    ServerPicture::Icon => read_shaped(&path, ICON, ICON_PREVIEW, "a server icon"),
                    ServerPicture::Banner => {
                        read_shaped(&path, BACKGROUND, BACKGROUND_PREVIEW, "a server banner")
                    }
                    ServerPicture::Splash => read_shaped(
                        &path,
                        BACKGROUND,
                        BACKGROUND_PREVIEW,
                        "an invite background",
                    ),
                    ServerPicture::RoleIcon => {
                        read_shaped(&path, ROLE_ICON, ROLE_ICON_PREVIEW, "a role icon")
                    }
                    ServerPicture::Emoji => super::expressions::read_emoji(&path),
                    ServerPicture::Sticker => super::expressions::read_sticker(&path),
                };
                let _ = events.send(Event::ServerPicture { kind, path, result });
            });
        }
        DiscordCommand::SaveServerSettings { server, change } => {
            let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
                let why = discord.err().unwrap_or(NOT_DISCORD);
                let _ = events.send(Event::ServerSettingsSaved {
                    server,
                    result: Err(why.into()),
                });
                return None;
            };
            let (discord, guild) = (discord.clone(), guild.to_owned());
            tokio::spawn(async move {
                let result = match *change {
                    ServerSettingsChange::Profile { .. } => {
                        let body = profile_body(&change);
                        drop(change);
                        discord
                            .stepping_up(|| discord.save_guild_profile(&guild, &body))
                            .await
                            .map(|profile| ServerSettingsPage::Profile(server_profile(profile)))
                    }
                    ServerSettingsChange::Guild { .. } => {
                        let body = guild_body(&change);
                        drop(change);
                        discord
                            .stepping_up(|| discord.save_guild_settings(&guild, &body))
                            .await
                            .map(|settings| ServerSettingsPage::Guild(guild_settings(settings)))
                    }
                    ServerSettingsChange::Roles { positions, role } => {
                        super::roles::save(&discord, &guild, positions, role).await
                    }
                    ServerSettingsChange::MfaLevel(level) => discord
                        .stepping_up(|| discord.set_mfa_level(&guild, level))
                        .await
                        .map(|settings| ServerSettingsPage::Guild(guild_settings(settings))),
                };
                let result = result.map_err(|error| {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "could not save server settings"
                    );
                    format!("Couldn't save the changes: {error}")
                });
                let _ = events.send(Event::ServerSettingsSaved { server, result });
            });
        }
        DiscordCommand::Bans { server, action } => {
            let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
                let why = discord.err().unwrap_or(NOT_DISCORD).to_owned();
                let _ = events.send(match action {
                    BansAction::Unban(user) => Event::Unbanned {
                        server,
                        user,
                        result: Err(why),
                    },
                    BansAction::More(_) | BansAction::Search(_) => Event::BansLoaded {
                        server,
                        searched: matches!(action, BansAction::Search(_)),
                        result: Err(why),
                    },
                });
                return None;
            };
            let (discord, guild) = (discord.clone(), guild.to_owned());
            tokio::spawn(async move { bans(&discord, &guild, server, action, &events).await });
        }
        other => return Some(Command::Discord(other)),
    }
    None
}

/// Does what Bans asked, and says how it went.
async fn bans(
    discord: &Discord,
    guild: &str,
    server: String,
    action: BansAction,
    events: &UnboundedSender<Event>,
) {
    let (searched, result) = match action {
        BansAction::Unban(user) => {
            let result = discord
                .stepping_up(|| discord.unban(guild, &user))
                .await
                .map_err(|error| {
                    tracing::info!(error = format!("{error:#}"), "Discord kept a ban");
                    format!("Couldn't revoke the ban: {error}")
                });
            let _ = events.send(Event::Unbanned {
                server,
                user,
                result,
            });
            return;
        }
        BansAction::More(after) => (false, discord.bans(guild, Some(&after)).await),
        BansAction::Search(query) => (true, discord.search_bans(guild, &query).await),
    };
    let result = result
        .map(|bans| bans.into_iter().map(ban).collect())
        .map_err(|error| {
            tracing::warn!(error = format!("{error:#}"), "could not fetch bans");
            format!("Discord didn't send the bans: {error}")
        });
    let _ = events.send(Event::BansLoaded {
        server,
        searched,
        result,
    });
}

/// A ban as Bans shows it: the user's display name, with their username
/// under it, or their username alone.
pub(super) fn ban(ban: DiscordBan) -> Ban {
    let user = ban.user;
    let (name, username) = match user.global_name {
        Some(name) => (name, Some(user.username)),
        None => (user.username, None),
    };
    Ban {
        user: RoleMember {
            id: user.id,
            name,
            username,
            avatar: user.avatar,
        },
        reason: ban.reason,
    }
}

/// A prompt's method as the Discord service names it.
fn discord_method(method: MfaMethod) -> mfa::Method {
    match method {
        MfaMethod::Passkey => mfa::Method::Webauthn,
        MfaMethod::App => mfa::Method::Totp,
        MfaMethod::Sms => mfa::Method::Sms,
        MfaMethod::Password => mfa::Method::Password,
        MfaMethod::Backup => mfa::Method::Backup,
    }
}

/// A method of a prompt's as Yapfast names it.
pub(super) fn method(method: mfa::Method) -> MfaMethod {
    match method {
        mfa::Method::Webauthn => MfaMethod::Passkey,
        mfa::Method::Totp => MfaMethod::App,
        mfa::Method::Sms => MfaMethod::Sms,
        mfa::Method::Password => MfaMethod::Password,
        mfa::Method::Backup => MfaMethod::Backup,
    }
}

fn server_profile(profile: DiscordProfile) -> ServerProfile {
    ServerProfile {
        name: profile.name,
        description: profile.description,
        icon: profile.icon,
    }
}

/// A server's settings as Yapfast shows them.
pub(super) fn guild_settings(settings: DiscordSettings) -> GuildSettings {
    GuildSettings {
        system_channel: settings.system_channel,
        system_flags: settings.system_flags,
        default_notifications: settings.default_notifications,
        afk_channel: settings.afk_channel,
        afk_timeout: settings.afk_timeout,
        banner: settings.banner,
        splash: settings.splash,
        progress_bar: settings.progress_bar,
        premium_tier: settings.premium_tier,
        features: settings.features,
        verification_level: settings.verification_level,
        explicit_content_filter: settings.explicit_content_filter,
        mfa_level: settings.mfa_level,
        invites_paused_until: settings.invites_paused_until,
        dms_paused_until: settings.dms_paused_until,
        extra_emoji_slots: settings.extra_emoji_slots,
    }
}

/// What Discord's client sends for a change to Engagement or Boost Perks,
/// for `PATCH /guilds/{id}`: only what changed.
fn guild_body(change: &ServerSettingsChange) -> Value {
    let mut body = Map::new();
    let ServerSettingsChange::Guild {
        system_channel,
        system_flags,
        default_notifications,
        afk_channel,
        afk_timeout,
        banner,
        splash,
        progress_bar,
        verification_level,
        explicit_content_filter,
    } = change
    else {
        return Value::Object(body);
    };
    if let Some(channel) = system_channel {
        body.insert("system_channel_id".into(), channel.as_deref().into());
    }
    if let Some(flags) = system_flags {
        body.insert("system_channel_flags".into(), (*flags).into());
    }
    if let Some(level) = default_notifications {
        body.insert("default_message_notifications".into(), (*level).into());
    }
    if let Some(channel) = afk_channel {
        body.insert("afk_channel_id".into(), channel.as_deref().into());
    }
    if let Some(seconds) = afk_timeout {
        body.insert("afk_timeout".into(), (*seconds).into());
    }
    if let Some(banner) = banner {
        body.insert("banner".into(), picture(banner));
    }
    if let Some(splash) = splash {
        body.insert("splash".into(), picture(splash));
    }
    if let Some(shown) = progress_bar {
        body.insert("premium_progress_bar_enabled".into(), (*shown).into());
    }
    if let Some(level) = verification_level {
        body.insert("verification_level".into(), (*level).into());
    }
    if let Some(level) = explicit_content_filter {
        body.insert("explicit_content_filter".into(), (*level).into());
    }
    Value::Object(body)
}

/// What Discord's client sends for a change to a server's profile, for
/// `PATCH /guilds/{id}/profile`: only what changed.
fn profile_body(change: &ServerSettingsChange) -> Value {
    let mut body = Map::new();
    let ServerSettingsChange::Profile {
        name,
        description,
        icon,
    } = change
    else {
        return Value::Object(body);
    };
    if let Some(name) = name {
        body.insert("name".into(), name.as_str().into());
    }
    if let Some(description) = description {
        body.insert("description".into(), description.as_str().into());
    }
    if let Some(icon) = icon {
        body.insert("icon".into(), picture(icon));
    }
    Value::Object(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_profile_change_sends_only_what_changed() {
        let change = ServerSettingsChange::Profile {
            name: Some("Crew".into()),
            description: None,
            icon: Some(None),
        };
        assert_eq!(profile_body(&change), json!({"name": "Crew", "icon": null}));
    }

    #[test]
    fn a_guild_change_sends_only_what_changed() {
        let change = ServerSettingsChange::Guild {
            system_channel: Some(None),
            system_flags: Some(5),
            default_notifications: None,
            afk_channel: Some(Some("14".into())),
            afk_timeout: None,
            banner: None,
            splash: None,
            progress_bar: Some(true),
            verification_level: Some(2),
            explicit_content_filter: None,
        };
        assert_eq!(
            guild_body(&change),
            json!({"system_channel_id": null, "system_channel_flags": 5,
                   "afk_channel_id": "14", "premium_progress_bar_enabled": true,
                   "verification_level": 2})
        );
    }
}
