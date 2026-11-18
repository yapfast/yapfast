//! How a menu's target names what it is about (`header:`, `in-voice:`...),
//! and the submenus built as a menu opens: a member's roles, the servers
//! to invite someone to, the apps' user commands, the voice channels to
//! move someone to, and a server's events (`src/ui/menus.rs`).

use yapfast_core::application::{AppState, Application, CommandTarget};
use yapfast_lang::tr;

use anomi_vm::menu::Row;

use super::Make;
use super::notify::until;

/// A Discord server's header, whose dropdown lists some of the server
/// menu's choices: this, then the server's id.
pub(crate) const HEADER: &str = "header:";
/// A server's "N Events" row, whose popout lists its scheduled events.
pub(crate) const EVENTS: &str = "events:";
/// A thread header's More button, before the thread's room id: the header
/// in full view, and the one beside its channel.
pub(crate) const TOOLBAR: &str = "thread-toolbar:";
pub(crate) const SIDEBAR: &str = "thread-sidebar:";
/// A profile's More button, before the person's id.
pub(crate) const PROFILE_MORE: &str = "profile-more:";
/// The header's bell, before the open channel's room id.
pub(crate) const BELL: &str = "notifications:";
/// Someone's menu in a voice channel: this, the channel's room id, `/`,
/// and their id, which has no `/`.
pub(crate) const IN_VOICE: &str = "in-voice:";
/// What the rows of the submenus report, before their role's, server's,
/// command's or channel's id.
pub(crate) const ROLE: &str = "role:";
pub(crate) const INVITE_TO: &str = "invite-to:";
pub(crate) const APP: &str = "app:";
pub(crate) const MOVE_TO: &str = "move-to:";

/// Where Discord's client says a profile's More menu blocked, unblocked or
/// unfriended someone; a right-click menu is "ContextMenu".
const PROFILE_OVERFLOW: &str = "user profile overflow menu";

/// A menu's target without the profile's prefix, and where Discord's client
/// says a change to a relationship from that menu came from.
pub(crate) fn relationship_place(target: &str) -> (&str, &'static str) {
    match target.strip_prefix(PROFILE_MORE) {
        Some(user) => (user, PROFILE_OVERFLOW),
        None => (target, "ContextMenu"),
    }
}

/// The thread a header's More menu is about, and where Discord's client
/// says a choice in it came from.
pub(crate) fn thread_more(id: &str) -> Option<(&str, &'static str)> {
    id.strip_prefix(TOOLBAR)
        .map(|room| (room, "Toolbar Overflow"))
        .or_else(|| {
            id.strip_prefix(SIDEBAR)
                .map(|room| (room, "Sidebar Overflow"))
        })
}

/// The voice channel and the person a voice channel's menu is about.
pub(crate) fn voice_target(target: &str) -> Option<(&str, &str)> {
    target.strip_prefix(IN_VOICE)?.rsplit_once('/')
}

/// The conversation and the person a menu is about: a person's one-to-one
/// DM, or a DM's other person.
pub(crate) fn subjects(
    state: &AppState,
    target: &str,
    person: bool,
) -> (Option<String>, Option<String>) {
    if person {
        let room = state
            .rooms
            .iter()
            .find(|room| room.direct && room.person.as_deref() == Some(target))
            .map(|room| room.id.clone());
        (room, Some(target.to_owned()))
    } else {
        let user = state
            .rooms
            .iter()
            .find(|room| room.id == target)
            .and_then(|room| room.person.clone());
        (Some(target.to_owned()), user)
    }
}

/// A server's scheduled events as the popout under its "N Events" row
/// lists them: each one's name, and when it starts and where. A server
/// without events opens the row all the same, as Discord's does.
pub(super) fn events(state: &AppState, make: &Make<'_>, server: &str, twelve: bool) -> Vec<Row> {
    let rows: Vec<Row> = state
        .servers
        .iter()
        .find(|found| found.id == server)
        .into_iter()
        .flat_map(|server| &server.events)
        .map(|event| {
            let when = match u64::try_from(event.start) {
                Ok(_) if event.active => tr!("Happening now").to_owned(),
                Ok(start) => until(start, twelve),
                Err(_) => String::new(),
            };
            let subtitle = match &event.location {
                Some(place) if !when.is_empty() => format!("{when} · {place}"),
                Some(place) => place.clone(),
                None => when,
            };
            make.row(&event.name, &format!("event:{}", event.id))
                .subtitle(subtitle)
                .plain()
        })
        .collect();
    if rows.is_empty() {
        return vec![
            make.row(tr!("No upcoming events"), "event:none")
                .plain()
                .disabled(true),
        ];
    }
    rows
}

/// A server member's roles: every role of the server, highest first,
/// ticked for those they hold, and greyed for those this account may not
/// give or take, which show only while held, as Discord's list.
pub(super) fn roles(core: &Application, make: &Make<'_>, user: &str) -> Vec<Row> {
    let Some(place) = core.state.selected.as_deref() else {
        return Vec::new();
    };
    core.role_choices(place, user)
        .into_iter()
        .filter(|choice| choice.grantable || choice.held)
        .map(|choice| {
            make.row(&choice.role.name, &format!("{ROLE}{}", choice.role.id))
                .checkbox(choice.held)
                .disabled(!choice.grantable)
        })
        .collect()
}

/// The servers someone may be invited to, by name, in the rail's order.
pub(super) fn invite(core: &Application, make: &Make<'_>, user: Option<&str>) -> Vec<Row> {
    let Some(user) = user else {
        return Vec::new();
    };
    core.invite_servers(user)
        .into_iter()
        .map(|server| make.row(&server.name, &format!("{INVITE_TO}{}", server.id)))
        .collect()
}

/// The user commands `place`'s command index offers, each with its app's
/// name under it.
pub(super) fn apps(core: &Application, make: &Make<'_>, place: Option<&str>) -> Vec<Row> {
    let Some(place) = place else {
        return Vec::new();
    };
    core.commands_for_target(place, CommandTarget::User)
        .into_iter()
        .map(|command| {
            make.row(&command.name, &format!("{APP}{}", command.command))
                .subtitle(command.app_name)
                .plain()
        })
        .collect()
}

/// The voice channels someone may be moved to, by name, in the channel
/// list's order.
pub(super) fn move_to(core: &Application, make: &Make<'_>, room: &str, user: &str) -> Vec<Row> {
    core.move_targets(room, user)
        .into_iter()
        .map(|(room, name)| make.row(&name, &format!("{MOVE_TO}{room}")))
        .collect()
}
