//! Members: a page of the server's members from the core's page
//! (`members_prompt`, `src/ui/server_settings.rs:1068`). The search field
//! is the window's: what is typed goes to Discord once the typing stops
//! (`intent::server_settings::members`).

use yapfast_core::application::{Application, MEMBER_PAGE_SIZES, MembersPage};
use yapfast_core::chat::{JoinMethod, ServerRole};
use yapfast_core::clock;
use yapfast_lang::{tr, trn};

use anomi_vm::settings::Settings;
use anomi_vm::settings::members::{Member, Members};
use yapfast_core::chat::Permission;

use super::roles::color;
use crate::project::rows;

/// How long ago, as Discord's Members words it: `account` for Joined
/// Discord, which counts in days at least.
pub(super) fn ago(seconds: u64, account: bool) -> String {
    const DAY: u64 = 86_400;
    match seconds {
        0..DAY if account => tr!("<1 day ago").to_owned(),
        0..60 => tr!("just now").to_owned(),
        60..3_600 => trn!("{n} min ago", "{n} mins ago", seconds / 60),
        3_600..DAY => trn!("{n} hr ago", "{n} hrs ago", seconds / 3_600),
        // Discord's months are 30 days, and its years 360.
        DAY..2_592_000 => trn!("{n} day ago", "{n} days ago", seconds / DAY),
        2_592_000..31_104_000 => trn!("{n} month ago", "{n} months ago", seconds / 2_592_000),
        _ => trn!("{n} year ago", "{n} years ago", seconds / 31_104_000),
    }
}

/// The time left, as Discord's timeout tooltip counts it: "2h 3m 10s".
fn time_left(seconds: u64) -> String {
    let parts = [
        (seconds / 86_400, tr!("d")),
        (seconds / 3_600 % 24, tr!("h")),
        (seconds / 60 % 60, tr!("m")),
        (seconds % 60, tr!("s")),
    ];
    let first = parts.iter().position(|(count, _)| *count > 0).unwrap_or(3);
    parts[first..]
        .iter()
        .map(|(count, unit)| format!("{count}{unit}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// How a member joined, as Join Method names it, and whether Discord
/// doesn't say.
fn join_method(joined: &JoinMethod) -> (String, bool) {
    let label = match joined {
        JoinMethod::Unknown => return (tr!("Unknown").to_owned(), true),
        JoinMethod::Bot => tr!("Bot Invite").to_owned(),
        JoinMethod::Integration(kind) => match kind.as_deref() {
            Some("twitch") => tr!("Twitch (Integration)").to_owned(),
            Some("youtube") => tr!("YouTube (Integration)").to_owned(),
            _ => tr!("Integration").to_owned(),
        },
        JoinMethod::Discovery => tr!("Server Discovery").to_owned(),
        JoinMethod::Hub => tr!("Student Hub").to_owned(),
        JoinMethod::Invite(code) | JoinMethod::Vanity(code) => code.clone(),
        JoinMethod::Manual(Some(code)) => tr!("Manual Verification ({})", code),
        JoinMethod::Manual(None) => tr!("Manual Verification").to_owned(),
        JoinMethod::LinkedChannel => tr!("Linked Channel").to_owned(),
    };
    (label, false)
}

/// A member's first role's chip: its name, colour and "+2" for more.
fn chip(held: &[String], roles: &[ServerRole]) -> (String, iced::Color, String) {
    let mut known = roles.iter().filter(|role| held.contains(&role.id));
    let Some(first) = known.next() else {
        return (String::new(), color(0), String::new());
    };
    let more = known.count();
    let more = if more > 0 {
        format!("+{more}")
    } else {
        String::new()
    };
    (
        first.name.clone(),
        color(first.color.unwrap_or_default()),
        more,
    )
}

/// Members, a page of them; the server's roles, highest first, colour them.
pub(super) fn show(
    core: &Application,
    server: &str,
    page: &MembersPage,
    all: &mut Settings,
    window: iced::Size,
) {
    let state = &core.state;
    let roles = core.server_roles(server);
    let now = clock::now_ms();
    let rows: Vec<Member> = page
        .on_page()
        .iter()
        .map(|member| {
            let person = &member.person;
            let (role, role_color, more_roles) = chip(&member.roles, &roles);
            // Discord colours a name by its highest coloured role.
            let name_color = roles
                .iter()
                .filter(|role| member.roles.contains(&role.id))
                .find_map(|role| role.color.filter(|color| *color != 0))
                .map(rows::rgb);
            let created = person
                .id
                .parse::<u64>()
                .map_or(now, |id| (id >> 22) + 1_420_070_400_000);
            let (join, join_unknown) = join_method(&member.joined_by);
            Member {
                id: person.id.clone(),
                name: person.name.clone(),
                username: person.username.clone().unwrap_or_default(),
                avatar: rows::avatar(state, &person.name, &person.id, Some(&person.avatar)),
                name_color,
                since: ago(now.saturating_sub(member.joined) / 1000, false),
                age: ago(now.saturating_sub(created) / 1000, true),
                join,
                join_unknown,
                role,
                role_color,
                more_roles,
                timed_out: if member.timed_out_until > now {
                    let left = (member.timed_out_until - now).div_ceil(1000);
                    tr!("Timed out for {}", time_left(left))
                } else {
                    String::new()
                },
                unusual_dms: member.unusual_dms,
                spammer: member.spammer,
                quarantined: member.quarantined,
            }
        })
        .collect();
    let vm = &mut all.members;
    if vm.rows != rows {
        vm.rows = rows;
    }
    let empty = page.members.is_empty() && page.fetching();
    let count = if empty {
        "...".to_owned()
    } else {
        page.count().to_string()
    };
    let members = Members {
        rows: std::mem::take(&mut vm.rows),
        search: std::mem::take(&mut vm.search),
        searched: page.searched(),
        sort: usize::from(page.sort.saturating_sub(1)),
        page: page.page,
        pages: page.pages(),
        page_size: MEMBER_PAGE_SIZES
            .iter()
            .position(|size| *size == page.page_size)
            .unwrap_or(0),
        total: usize::try_from(page.count()).unwrap_or(usize::MAX),
        count,
        fetching: page.fetching(),
        indexing: page.indexing,
        join_method: core.may(server, Permission::ManageServer),
        narrow: anomi_settings::server::members::narrow(window),
    };
    *vm = members;
}
