//! Samples of a server's Safety Setup for tests and shots, after how
//! `safety_prompt` words it (`src/ui/server_settings.rs:838`): task X05's.

use iced::Color;
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::settings::{invites, server};
use crate::fixtures::shell;
use crate::settings::safety::{Actions, Offer, Safety};
use crate::settings::server::Section;
use crate::shell::Layer;

fn rgb(rgb: u32) -> Option<Color> {
    Some(Color::from_rgb8(
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
    ))
}

fn offer(value: u8, title: &str, description: &str, color: Option<u32>) -> Offer {
    Offer {
        value,
        title: title.into(),
        description: description.into(),
        color: color.and_then(rgb),
        enabled: true,
    }
}

/// A plain server as the owner without 2FA sees it, nothing but "None"
/// and "Do not filter" chosen, as the golden screenshot has it.
pub fn page() -> Safety {
    Safety {
        verification_text: tr!("Members of the server must meet the following criteria before they can send messages in text channels or initiate a direct message conversation. If a member has an assigned role and server onboarding is not enabled, this does not apply. We recommend setting a verification level for a Community Server.").into(),
        verification_options: vec![
            offer(0, tr!("None"), tr!("Unrestricted"), None),
            offer(1, tr!("Low"), tr!("Must have a verified email on their Discord account."), Some(0x23a55a)),
            offer(2, tr!("Medium"), tr!("Must also be registered on Discord for longer than 5 minutes."), Some(0xf0b232)),
            offer(3, tr!("High"), tr!("Must also be a member of this server for longer than 10 minutes."), Some(0xf57731)),
            offer(4, tr!("Highest"), tr!("Must have a verified phone on their Discord account."), Some(0xf23f43)),
        ],
        verification: 0,
        filter_options: vec![
            offer(2, tr!("Filter messages from all members"), tr!("All messages will be filtered for sensitive image-based media."), Some(0xf23f43)),
            offer(1, tr!("Filter messages from server members without roles"), tr!("Messages from server members without a role will be filtered for sensitive image-based media."), Some(0xf0b232)),
            offer(0, tr!("Do not filter"), tr!("Messages will not be filtered for sensitive image-based media."), None),
        ],
        filter: 0,
        mfa_text: format!(
            "{} {}",
            tr!("Moderators must have two-factor authentication enabled to ban, kick, or timeout members and delete messages."),
            tr!("Only the server owner can change this setting if they have 2FA enabled.")
        ),
        mfa_lock: tr!("To change this setting, first enable 2FA on your account.").into(),
        ..Safety::default()
    }
}

/// The owner without 2FA, with Discord's note under the switch.
pub fn noted() -> Safety {
    Safety {
        mfa_note: tr!(
            "In order to change this setting, you must have 2FA enabled on your account."
        )
        .into(),
        ..page()
    }
}

/// A Community server: it can't have "None", and the filter below "all
/// members" is off.
pub fn community() -> Safety {
    let mut safety = page();
    safety.verification = 1;
    safety.verification_options[0].enabled = false;
    safety.verification_options[0].description =
        tr!("This option is not available for Community Servers.").into();
    safety.mfa_lock = String::new();
    safety
}

/// Security Actions as opened: an hour of nothing paused yet.
pub fn actions() -> Actions {
    Actions::default()
}

/// Security Actions open over the Invites, whose button opens it.
pub fn actions_open() -> Vm {
    let mut vm = invites::page();
    vm.settings.safety.actions = Some(actions());
    shell::open(vm, Layer::SecurityActions)
}

/// The server's settings open on Safety Setup.
pub fn safety() -> Vm {
    let mut vm = server::profile();
    vm.settings.server.section = Section::Safety;
    vm.settings.safety = page();
    vm
}

/// Safety Setup of a Community server whose owner has no 2FA: "None" not
/// offered, the switch locked, Discord's note under it.
pub fn safety_community() -> Vm {
    let mut vm = safety();
    let noted = noted();
    let mut safety = community();
    safety.mfa_lock = noted.mfa_lock;
    safety.mfa_note = noted.mfa_note;
    vm.settings.safety = safety;
    vm
}
