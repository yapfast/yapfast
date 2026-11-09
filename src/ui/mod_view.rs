//! Mod View, beside the channel in place of its member list; see
//! `ui/mod_view.slint` and the core's `application/mod_view.rs`.

use yapfast_core::{
    application::{
        AuditInput, ELEVATED_PERMISSIONS, MOD_PERMISSIONS, ModPanel, ModView, ModViewInput,
        ModViewMember, NEW_ACCOUNT_MS, PersonAsk, ROLE_PERMISSIONS,
    },
    chat::MemberChange,
    clock,
};

use super::server_settings::{ago, audit_log_prompt, join_method, role_color};
use super::*;

impl Adapter {
    pub(super) fn show_mod_view(&mut self, window: &YapfastWindow) {
        let (Some(view), Some(member)) =
            (self.app.state.mod_view.as_ref(), self.app.mod_view_member())
        else {
            if window.get_mod_view_open() {
                window.set_mod_view_open(false);
                window.set_mod_view(ModViewPrompt::default());
            }
            return;
        };
        let prompt = mod_view_prompt(view, &member, self.layout.twelve_hour_clock);
        window.set_mod_view(prompt);
        window.set_mod_view_open(true);
    }
}

/// A count of Mod View's: "..." while it comes, "?" when Discord didn't
/// send it.
fn count(counts: Option<&Result<[u64; 3], String>>, index: usize) -> (SharedString, bool) {
    match counts {
        None => ("...".into(), false),
        Some(Err(_)) => ("?".into(), false),
        Some(Ok(counts)) => (counts[index].to_string().into(), counts[index] > 0),
    }
}

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}

/// What Mod View shows of its member.
fn mod_view_prompt(view: &ModView, member: &ModViewMember, twelve_hour: bool) -> ModViewPrompt {
    let now = clock::now_ms();
    let found = view
        .member
        .as_ref()
        .and_then(|found| found.as_ref().ok())
        .and_then(Option::as_ref);
    let activity = view.activity.as_ref();
    let (messages, messages_open) = count(activity, 0);
    let (links, links_open) = count(activity, 1);
    let (media, _) = count(activity, 2);
    let log = &view.log;
    let audit: SharedString = if log.fetching() {
        "...".into()
    } else if log.failed {
        "This is Awkward".into()
    } else {
        log.rows.len().to_string().into()
    };
    // The permissions they hold as Discord's spec lists them, with their
    // group's heading and description.
    let infos: Vec<(&str, &yapfast_core::application::PermissionInfo)> = ROLE_PERMISSIONS
        .iter()
        .flat_map(|group| {
            group
                .permissions
                .iter()
                .map(move |info| (group.heading, info))
        })
        .collect();
    let info = |bit: u32| infos.iter().find(|(_, info)| info.bit == bit);
    let mod_permissions = member
        .granted
        .iter()
        .filter(|granted| MOD_PERMISSIONS.contains(&granted.bit))
        .filter_map(|granted| info(granted.bit))
        .map(|(_, info)| ModViewChip {
            text: info.title.into(),
            elevated: ELEVATED_PERMISSIONS.contains(&info.bit),
            ..Default::default()
        })
        .collect();
    // Only the panel shown is made.
    let permissions = if view.panel == ModPanel::Permissions {
        member
            .granted
            .iter()
            .filter_map(|granted| Some((granted, info(granted.bit)?)))
            .map(|(granted, (heading, info))| {
                let mut roles: Vec<SharedString> = granted
                    .roles
                    .iter()
                    .map(|role| role.as_str().into())
                    .collect();
                if granted.owner {
                    roles.insert(0, "Server Owner".into());
                }
                ModPermissionRow {
                    title: info.title.into(),
                    description: info.description.into(),
                    category: (*heading).into(),
                    elevated: ELEVATED_PERMISSIONS.contains(&info.bit),
                    moderator: MOD_PERMISSIONS.contains(&info.bit),
                    count: if roles.len() == 1 {
                        "1 Role".into()
                    } else {
                        format!("{} Roles", roles.len()).into()
                    },
                    roles: model(roles),
                }
            })
            .collect()
    } else {
        vec![]
    };
    let chip = |role: &yapfast_core::chat::ServerRole, removable| ModViewChip {
        id: role.id.as_str().into(),
        text: role.name.as_str().into(),
        color: role
            .color
            .filter(|color| *color != 0)
            .map_or(slint::Color::default(), role_color),
        elevated: false,
        removable,
    };
    let (join, join_unknown) = match &view.member {
        None => ("...".to_owned(), true),
        Some(_) => join_method(
            &found
                .map(|found| found.joined_by.clone())
                .unwrap_or_default(),
        ),
    };
    let created = member.created;
    let day = |ms: u64| SharedString::from(clock::day(ms / 1000));
    ModViewPrompt {
        name: member.name.as_str().into(),
        username: member.username.as_deref().unwrap_or_default().into(),
        initial: initials(&member.name),
        avatar_url: member.avatar.as_deref().unwrap_or_default().into(),
        panel: match view.panel {
            ModPanel::Info => 0,
            ModPanel::Permissions => 1,
            ModPanel::ActionLog => 2,
        },
        me: member.me,
        may_kick: member.may_kick,
        may_ban: member.may_ban,
        may_timeout: member.may_timeout,
        timeout_left: member
            .timed_out_until
            .map_or(0, |until| (until.saturating_sub(now)).div_ceil(1000))
            .try_into()
            .unwrap_or(i32::MAX),
        unusual_dms: if found.is_some_and(|found| found.unusual_dms) {
            "In the last 2 days".into()
        } else {
            SharedString::default()
        },
        messages,
        links,
        media,
        audit,
        messages_open,
        links_open,
        audit_open: !log.fetching() && !log.failed && !log.rows.is_empty(),
        audit_failed: log.failed,
        mod_permissions: model(mod_permissions),
        all_count: i32::try_from(member.granted.len()).unwrap_or_default(),
        permissions: model(permissions),
        roles: model(
            member
                .roles
                .iter()
                .map(|(role, removable)| chip(role, *removable))
                .collect(),
        ),
        addable: model(
            member
                .addable
                .iter()
                .map(|role| chip(role, false))
                .collect(),
        ),
        created: day(created),
        created_ago: ago(now.saturating_sub(created) / 1000, true).into(),
        new_account: now.saturating_sub(created) < NEW_ACCOUNT_MS,
        joined: member.joined.map(day).unwrap_or_default(),
        joined_ago: member
            .joined
            .map(|joined| ago(now.saturating_sub(joined) / 1000, false).into())
            .unwrap_or_default(),
        join_method: join.into(),
        join_unknown,
        invited_by: member.inviter.as_deref().unwrap_or_default().into(),
        log: if view.panel == ModPanel::ActionLog {
            audit_log_prompt(log, twelve_hour)
        } else {
            Default::default()
        },
    }
}

/// What Mod View's buttons do; see `ModViewPanel` in `ui/mod_view.slint`.
pub(super) fn mod_view_act(
    adapter: &mut Adapter,
    window: &YapfastWindow,
    what: i32,
    text: SharedString,
    number: i32,
) {
    let Some(view) = adapter.app.state.mod_view.as_ref() else {
        return;
    };
    let (place, user) = (view.place.clone(), view.user.clone());
    let index = usize::try_from(number).unwrap_or_default();
    let ask = |ask| Action::AskMember {
        place: place.clone(),
        user: user.clone(),
        ask,
    };
    let change = |change| Action::ChangeMember {
        place: place.clone(),
        user: user.clone(),
        change,
    };
    let action = match what {
        0 => Action::ModView(ModViewInput::Close),
        1 => Action::ModView(ModViewInput::Refresh),
        2 => Action::ModView(ModViewInput::Panel(match number {
            1 => ModPanel::Permissions,
            2 => ModPanel::ActionLog,
            _ => ModPanel::Info,
        })),
        3 => Action::MessagePerson(user.clone()),
        4 => ask(PersonAsk::Kick),
        5 => ask(PersonAsk::Ban),
        6 => ask(PersonAsk::Timeout),
        7 => change(MemberChange::Timeout {
            until_ms: None,
            reason: String::new(),
        }),
        8 => {
            window.invoke_copy_text(user.into());
            return;
        }
        9 => {
            window.invoke_copy_text(format!("<@{user}>").into());
            return;
        }
        10 => Action::ModView(ModViewInput::Search { links: number == 1 }),
        11 | 12 => change(MemberChange::Role {
            role: text.into(),
            add: what == 12,
        }),
        13 => Action::ModView(ModViewInput::QuickTimeout(index)),
        22 => Action::ModView(ModViewInput::Log(AuditInput::More)),
        23 => Action::ModView(ModViewInput::Log(AuditInput::Toggle(index))),
        _ => return,
    };
    adapter.action(window, action);
}
