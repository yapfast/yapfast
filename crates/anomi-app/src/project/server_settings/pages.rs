//! A server's Profile, Engagement, Boost Perks and Safety Setup from the
//! core's pages (`show_server_settings` and `safety_prompt`,
//! `src/ui/server_settings.rs:212`, `:839`).

use std::sync::Arc;

use yapfast_core::application::{
    AFK_TIMEOUTS, AppState, Application, BOOSTED, BoostPage, DESCRIPTION_LIMIT, EngagementPage,
    MfaRule, ProfilePage, SETUP_TIPS, SafetyPage, ServerSettings, WELCOME, WELCOME_STICKER,
};
use yapfast_core::chat::{Picture, ServerPicture};
use yapfast_lang::tr;

use anomi_vm::kind::Avatar;
use anomi_vm::settings::Settings;
use anomi_vm::settings::boosts::{Boosts, Picture as Background};
use anomi_vm::settings::engagement::Engagement;
use anomi_vm::settings::safety::Offer;
use anomi_vm::settings::server::Server;

use super::set;
use crate::project::rows;

/// Discord's colours for the safety levels.
const GREEN: u32 = 0x23a55a;
const YELLOW: u32 = 0xf0b232;
const ORANGE: u32 = 0xf57731;
const RED: u32 = 0xf23f43;

/// A picture as it will be: a new one's preview, else Discord's at `url`.
fn picture(
    state: &AppState,
    url: Option<&str>,
    new: Option<&Arc<Picture>>,
) -> Option<iced::widget::image::Handle> {
    new.and_then(crate::pictures::shown)
        .or_else(|| crate::pictures::find(state, url))
}

/// Server Profile: the name, the description and the icon as they will be.
pub(super) fn profile(
    state: &AppState,
    settings: &ServerSettings,
    page: &ProfilePage,
    vm: &mut Server,
) {
    let profile = &mut vm.profile;
    set(&mut profile.name, &page.name);
    set(&mut profile.description, &page.description);
    let (icon, new) = page.icon();
    let avatar = Avatar {
        initial: rows::initials(&page.name),
        tint: rows::tint(&page.original.name),
        picture: picture(state, icon, new),
    };
    if profile.icon != avatar {
        profile.icon = avatar;
    }
    profile.has_icon = icon.is_some() || new.is_some();
    profile.reading = settings.reading.is_some();
    let used = page.description.trim().chars().count();
    profile.description_left = i32::try_from(DESCRIPTION_LIMIT)
        .unwrap_or(i32::MAX)
        .saturating_sub(i32::try_from(used).unwrap_or(i32::MAX));
}

/// A list of channels with "none" first, and the place of the one chosen.
fn channels(
    none: &str,
    channels: &[(String, String)],
    chosen: Option<&str>,
) -> (Vec<String>, usize) {
    let names = std::iter::once(none.to_owned())
        .chain(channels.iter().map(|(_, name)| name.clone()))
        .collect();
    let index = chosen
        .and_then(|chosen| channels.iter().position(|(id, _)| id == chosen))
        .map_or(0, |index| index + 1);
    (names, index)
}

/// Engagement: system messages, default notifications, the inactive
/// channel.
pub(super) fn engagement(
    core: &Application,
    server: &str,
    page: &EngagementPage,
    all: &mut Settings,
) {
    let text = core.settings_channels(server, false);
    let voice = core.settings_channels(server, true);
    let (system_channels, system_channel) = channels(
        tr!("No System Messages"),
        &text,
        page.system_channel.as_deref(),
    );
    let (afk_channels, afk_channel) = channels(
        tr!("No Inactive Channel"),
        &voice,
        page.afk_channel.as_deref(),
    );
    let engagement = Engagement {
        system_channels,
        system_channel,
        welcome: page.sends(WELCOME),
        welcome_sticker: page.sends(WELCOME_STICKER),
        boosted: page.sends(BOOSTED),
        setup_tips: page.sends(SETUP_TIPS),
        notifications: usize::from(page.default_notifications.min(1)),
        afk_channels,
        afk_channel,
        // Discord's default, five minutes, for a timeout it doesn't offer.
        afk_timeout: AFK_TIMEOUTS
            .iter()
            .position(|seconds| *seconds == page.afk_timeout)
            .unwrap_or(1),
        saving: all.server.saving,
    };
    if all.engagement != engagement {
        all.engagement = engagement;
    }
}

/// Boost Perks: the banner and the invite background, each locked below its
/// level, and the progress bar.
pub(super) fn boosts(state: &AppState, page: &BoostPage, all: &mut Settings) {
    let shown = |kind: ServerPicture, level: u8| {
        let (url, new) = page.picture(kind);
        Background {
            picture: picture(state, url, new),
            shown: url.is_some() || new.is_some(),
            locked: if page.unlocked(kind) {
                String::new()
            } else {
                tr!("Unlocks at Level {}", level)
            },
        }
    };
    let boosts = Boosts {
        banner: shown(ServerPicture::Banner, 2),
        invite: shown(ServerPicture::Splash, 1),
        progress_bar: page.progress_bar,
        saving: all.server.saving,
        reading: all.server.profile.reading,
    };
    if all.boosts != boosts {
        all.boosts = boosts;
    }
}

/// One of Safety Setup's choices in Discord's words.
fn offer(value: u8, title: &str, description: &str, color: Option<u32>, offered: bool) -> Offer {
    Offer {
        value,
        title: title.to_owned(),
        description: if offered {
            description
        } else {
            tr!("This option is not available for Community Servers.")
        }
        .to_owned(),
        color: color.map(rows::rgb),
        enabled: offered,
    }
}

/// Safety Setup: the verification level, the content filter and two-factor
/// for moderators, as Discord words them for the kind of server.
pub(super) fn safety(core: &Application, server: &str, page: &SafetyPage, all: &mut Settings) {
    let community = page.community();
    let levels = [
        (0, tr!("None"), tr!("Unrestricted"), None),
        (
            1,
            tr!("Low"),
            tr!("Must have a verified email on their Discord account."),
            Some(GREEN),
        ),
        (
            2,
            tr!("Medium"),
            tr!("Must also be registered on Discord for longer than 5 minutes."),
            Some(YELLOW),
        ),
        (
            3,
            tr!("High"),
            tr!("Must also be a member of this server for longer than 10 minutes."),
            Some(ORANGE),
        ),
        (
            4,
            tr!("Highest"),
            tr!("Must have a verified phone on their Discord account."),
            Some(RED),
        ),
    ];
    let filters = [
        (
            2,
            tr!("Filter messages from all members"),
            tr!("All messages will be filtered for sensitive image-based media."),
            Some(RED),
        ),
        (
            1,
            tr!("Filter messages from server members without roles"),
            tr!(
                "Messages from server members without a role will be filtered for sensitive image-based media."
            ),
            Some(YELLOW),
        ),
        (
            0,
            tr!("Do not filter"),
            tr!("Messages will not be filtered for sensitive image-based media."),
            None,
        ),
    ];
    let rule = core.mfa_rule(server);
    let moderators = tr!(
        "Moderators must have two-factor authentication enabled to ban, kick, or timeout members and delete messages."
    );
    let owner_only = tr!("Only the server owner can change this setting if they have 2FA enabled.");
    let (mfa_text, mfa_note) = match (community, rule) {
        (false, MfaRule::OwnerWithoutMfa) => (
            format!("{moderators} {owner_only}"),
            tr!("In order to change this setting, you must have 2FA enabled on your account."),
        ),
        (false, _) | (true, MfaRule::NotOwner) => (format!("{moderators} {owner_only}"), ""),
        (true, MfaRule::May) => (moderators.to_owned(), ""),
        (true, MfaRule::OwnerWithoutMfa) => (
            format!(
                "{moderators} {}",
                tr!("To change this setting, first enable 2FA on your account.")
            ),
            "",
        ),
    };
    let safety = &mut all.safety;
    let verification_options: Vec<Offer> = levels
        .iter()
        .map(|&(value, title, text, color)| {
            offer(value, title, text, color, page.offers_verification(value))
        })
        .collect();
    if safety.verification_options != verification_options {
        safety.verification_options = verification_options;
    }
    let filter_options: Vec<Offer> = filters
        .iter()
        .map(|&(value, title, text, color)| {
            offer(value, title, text, color, page.offers_filter(value))
        })
        .collect();
    if safety.filter_options != filter_options {
        safety.filter_options = filter_options;
    }
    set(
        &mut safety.verification_text,
        if community {
            tr!(
                "Members of the server must meet the following criteria before they can send messages. If a member has an assigned role and server onboarding is not enabled, this does not apply."
            )
        } else {
            tr!(
                "Members of the server must meet the following criteria before they can send messages in text channels or initiate a direct message conversation. If a member has an assigned role and server onboarding is not enabled, this does not apply. We recommend setting a verification level for a Community Server."
            )
        },
    );
    safety.verification = page.verification();
    safety.filter = page.content_filter();
    safety.mfa_on = page.requires_mfa();
    set(&mut safety.mfa_text, &mfa_text);
    set(
        &mut safety.mfa_lock,
        page.mfa_lock(rule).unwrap_or_default(),
    );
    set(&mut safety.mfa_note, mfa_note);
    safety.saving = all.server.saving;
}
