//! Samples of a server's Members settings for tests and shots, after the fake
//! Discord's data: task X03's.

use iced::Color;

use crate::Vm;
use crate::fixtures::pictures::pictured;
use crate::fixtures::settings::server::server;
use crate::fixtures::{avatar, shell};
use crate::settings::members::{Member, Members};
use crate::settings::server::Section;
use crate::shell::Layer;

const STAFF: Color = Color::from_rgb(0.58, 0.60, 0.63);
const MODS: Color = Color::from_rgb(0.20, 0.60, 0.95);

fn member(name: &str, since: &str, join: &str) -> Member {
    let initial: String = name.chars().take(1).collect();
    Member {
        id: format!(
            "{}",
            100 + name.len() as u64 * 7 + u64::from(initial.as_bytes()[0])
        ),
        name: name.into(),
        username: name.to_lowercase(),
        avatar: pictured(avatar(&initial, 0x7a9ee0), name),
        since: since.into(),
        age: "1 year ago".into(),
        join: join.into(),
        ..Member::default()
    }
}

fn chip(mut member: Member, role: &str, color: Color, more: &str) -> Member {
    member.role = role.into();
    member.role_color = color;
    member.more_roles = more.into();
    member
}

/// Twelve members as the golden screenshot's page lists them: roles with
/// more behind them, an unknown way of joining and the signals.
pub fn members() -> Members {
    let mut rows = vec![
        member("Dave", "1 month ago", "Server Discovery"),
        chip(
            member("Carol", "2 months ago", "carol1"),
            "Leads",
            STAFF,
            "",
        ),
        chip(
            member("Alice", "2 months ago", "server"),
            "Staff",
            STAFF,
            "+1",
        ),
        chip(member("Bob", "10 months ago", ""), "Mods", MODS, ""),
        member("Erin", "10 months ago", "server"),
        member("Frank", "11 months ago", "server"),
    ];
    rows[1].unusual_dms = true;
    rows[3].join_unknown = true;
    rows[3].join = "Unknown".into();
    rows[3].name_color = Some(MODS);
    rows[4].timed_out = "Timed out for 3 hours".into();
    rows[4].spammer = true;
    rows[5].quarantined = true;
    for number in 1..=6 {
        rows.push(member(&format!("Member {number}"), "1 year ago", "server"));
    }
    Members {
        rows,
        page: 0,
        pages: 3,
        page_size: 0,
        total: 30,
        count: "30".into(),
        join_method: true,
        ..Members::default()
    }
}

/// The golden screenshot's page: its four members on one page, so the
/// footer only counts them.
pub fn few() -> Members {
    let mut page = members();
    page.rows.truncate(4);
    Members {
        pages: 1,
        total: 4,
        count: "4".into(),
        ..page
    }
}

fn with(members: Members) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = crate::settings::server::Server {
        section: Section::Members,
        fill: true,
        ..server()
    };
    vm.settings.members = members;
    vm
}

/// The window with the server's settings open on Members, as the golden
/// screenshot has it.
pub fn open() -> Vm {
    with(few())
}

/// The same with a page of twelve of thirty members: the pager, the page
/// size and every signal.
pub fn open_pages() -> Vm {
    with(members())
}
