//! Bans, Audit Log and Invites from the core's pages (`bans_prompt`,
//! `audit_log_prompt`, `invites_prompt`, `src/ui/server_settings.rs:963`,
//! `:1461`, `:1153`), with the ban's dialog and Security Actions over them.

use std::time::Instant;

use yapfast_core::application::{
    AUDIT_ACTIONS, AppState, Application, AuditKind, AuditLogPage, AuditSpan, AuditWords, BansPage,
    InvitesPage, PAUSE_HOURS,
};
use yapfast_core::chat::Permission;
use yapfast_core::clock;
use yapfast_lang::tr;

use anomi_vm::kind::Avatar;
use anomi_vm::settings::Settings;
use anomi_vm::settings::audit::{Kind, Line, Row as AuditRow};
use anomi_vm::settings::bans::{Open, Row as BanRow};
use anomi_vm::settings::invites::Row as InviteRow;
use anomi_vm::settings::safety::{Actions, HOURS};
use anomi_vm::shell::{Layer, Shell};
use anomi_vm::text::{Builder, Style, Text};

use super::{Shown, dialog, set};
use crate::layout::Layout;
use crate::project::rows;

/// Bans, a page of them, and the one open in its dialog.
pub(super) fn bans(
    core: &Application,
    server: &str,
    page: &BansPage,
    all: &mut Settings,
    shell: &mut Shell,
    shown: &mut Shown,
) {
    let state = &core.state;
    let vm = &mut all.bans;
    let rows: Vec<BanRow> = page
        .on_page()
        .into_iter()
        .map(|ban| {
            let user = &ban.user;
            BanRow {
                id: user.id.clone(),
                avatar: rows::avatar(state, &user.name, &user.id, Some(&user.avatar)),
                name: user.name.clone(),
                username: user.username.clone().unwrap_or_default(),
            }
        })
        .collect();
    if vm.rows != rows {
        vm.rows = rows;
    }
    vm.page = page.page;
    vm.pages = page.pages();
    vm.fetching = page.fetching;
    set(&mut vm.query, &page.query);
    let open = page.open_ban().map(|ban| {
        let user = &ban.user;
        Open {
            // Discord's title is the username alone.
            name: user.username.clone().unwrap_or_else(|| user.name.clone()),
            reason: ban
                .reason
                .clone()
                .unwrap_or_else(|| tr!("No reason provided").to_owned()),
            error: page.open_error.clone().unwrap_or_default(),
            unbanning: page.unbanning,
            may_unban: core.may(server, Permission::Ban),
        }
    });
    shown.ban.clone_from(&page.open);
    dialog(shell, Layer::Ban, &mut vm.open, open);
}

/// A sentence of Audit Log's, its bold parts in bold.
fn sentence(words: &AuditWords, twelve_hour: bool, now: u64) -> Text {
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let mut builder = Builder::new();
    for span in words.spans() {
        match span {
            AuditSpan::Text(text) => builder.text(text, Style::default()),
            AuditSpan::Bold("") => &mut builder,
            AuditSpan::Bold(text) => builder.text(text, bold),
            AuditSpan::Time(seconds) => {
                builder.text(&clock::calendar(seconds, now, twelve_hour), bold)
            }
        };
    }
    builder.finish()
}

/// Audit Log's rows with their lines, and its filters.
pub(super) fn audit(state: &AppState, layout: &Layout, page: &AuditLogPage, all: &mut Settings) {
    let twelve_hour = layout.twelve_hour_clock;
    let now = clock::now_ms() / 1000;
    let when = |seconds: u64| clock::calendar(seconds, now, twelve_hour);
    let faces: Vec<Avatar> = page
        .actors
        .iter()
        .map(|(id, user)| rows::avatar(state, &user.name, id, Some(&user.avatar)))
        .collect();
    let rows: Vec<AuditRow> = page
        .rows
        .iter()
        .map(|row| {
            let (start, end) = (when(row.start), when(row.end));
            let mut lines = vec![];
            for (index, line) in row.lines.iter().enumerate() {
                lines.push(Line {
                    number: format!("{:02}", index + 1),
                    text: sentence(&line.words, twelve_hour, now),
                });
                lines.extend(line.items.iter().map(|item| Line {
                    number: String::new(),
                    text: Text::styled(item, Style::default()),
                }));
            }
            AuditRow {
                actor: row
                    .actor
                    .and_then(|index| faces.get(index as usize))
                    .cloned()
                    .unwrap_or_default(),
                title: sentence(&row.title, twelve_hour, now),
                // A row of several entries gives when the first and the
                // last were, as Discord's does.
                time: if start == end {
                    start
                } else {
                    format!("{start}—{end}")
                },
                kind: match row.kind {
                    AuditKind::Create => Kind::Create,
                    AuditKind::Update => Kind::Update,
                    AuditKind::Delete => Kind::Delete,
                },
                lines,
            }
        })
        .collect();
    let vm = &mut all.audit;
    if vm.rows != rows {
        vm.rows = rows;
    }
    vm.open = page.open;
    let users: Vec<String> = std::iter::once(tr!("All Users").to_owned())
        .chain(page.actors.iter().map(|(_, user)| user.name.clone()))
        .collect();
    if vm.users != users {
        vm.users = users;
    }
    vm.user = page
        .user
        .as_ref()
        .and_then(|chosen| page.actors.iter().position(|(id, _)| id == chosen))
        .map_or(0, |index| index + 1);
    if vm.actions.is_empty() {
        vm.actions = std::iter::once(tr!("All Actions").to_owned())
            .chain(
                AUDIT_ACTIONS
                    .iter()
                    .map(|(_, name)| yapfast_lang::text(name).to_owned()),
            )
            .collect();
    }
    vm.action = page
        .action
        .and_then(|chosen| {
            AUDIT_ACTIONS
                .iter()
                .position(|(number, _)| *number == chosen)
        })
        .map_or(0, |index| index + 1);
    vm.fetching = page.fetching();
    vm.older = page.older;
    vm.failed = page.failed;
}

/// Invites, each counting down from when it was read, and Security Actions
/// while it is open.
pub(super) fn invites(
    core: &Application,
    server: &str,
    page: &InvitesPage,
    all: &mut Settings,
    shell: &mut Shell,
) {
    let state = &core.state;
    let roles = core.server_roles(server);
    let now = clock::now_ms() / 1000;
    let rows: Vec<InviteRow> = page
        .invites
        .iter()
        .map(|invite| {
            // Discord's list names the inviter by their username.
            let inviter = invite.inviter.as_ref();
            let name = inviter
                .map(|inviter| inviter.username.as_deref().unwrap_or(&inviter.name))
                .unwrap_or_default();
            let mut held = roles.iter().filter(|role| invite.roles.contains(&role.id));
            let first = held.next();
            let more = held.count();
            InviteRow {
                code: invite.code.clone(),
                inviter: rows::avatar(
                    state,
                    name,
                    inviter.map_or(name, |inviter| inviter.id.as_str()),
                    inviter.map(|inviter| inviter.avatar.as_str()),
                ),
                name: name.to_owned(),
                channel: invite.channel.clone(),
                uses: if invite.max_uses > 0 {
                    format!("{}/{}", invite.uses, invite.max_uses)
                } else {
                    invite.uses.to_string()
                },
                expires_in: invite.expires.map_or(-1, |expires| {
                    i64::try_from(expires.saturating_sub(now)).unwrap_or(i64::MAX)
                }),
                role: first.map(|role| role.name.clone()).unwrap_or_default(),
                role_color: super::roles::color(first.and_then(|role| role.color).unwrap_or(0)),
                more_roles: if more > 0 {
                    format!("+{more}")
                } else {
                    String::new()
                },
            }
        })
        .collect();
    let vm = &mut all.invites;
    // A page read anew starts the countdowns over from now.
    if vm.rows != rows {
        vm.set_rows(rows, Instant::now());
    }
    vm.paused = page.paused(now);
    vm.may_create = core.may(server, Permission::CreateInvite);
    let actions = page.security.as_ref().map(|draft| Actions {
        hours: PAUSE_HOURS
            .iter()
            .position(|hours| *hours == draft.hours)
            .unwrap_or(1)
            .min(HOURS.len() - 1),
        pause_invites: draft.invites,
        pause_dms: draft.dms,
        may_save: draft.changed(),
        saving: draft.saving,
        error: draft.error.clone().unwrap_or_default(),
    });
    dialog(
        shell,
        Layer::SecurityActions,
        &mut all.safety.actions,
        actions,
    );
}
