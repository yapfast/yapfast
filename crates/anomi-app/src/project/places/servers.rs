//! The dialogs about a server: joining one (`show_join`,
//! `src/ui/refresh.rs:2775`), and leaving a server or group DM or syncing a
//! moved channel's permissions (`show_leave`, `show_move_sync`, `:3320`,
//! `:3356`), which share one dialog. A handful of fields each, built while
//! the dialog shows and set when they differ. The invite is `invite`'s.

use yapfast_core::application::AppState;
use yapfast_lang::{tr, trn};

use anomi_vm::places::join::{Found, Join};
use anomi_vm::places::leave::Leave;
use anomi_vm::shell::{Layer, Shell};

use super::follow;
use crate::pictures::Requests;
use crate::project::rows;

/// A channel as the dialogs name it: a voice channel by its name, any
/// other with its `#`.
fn place(voice: bool, channel: &str) -> String {
    if voice {
        channel.to_owned()
    } else {
        format!("#{channel}")
    }
}

/// Join a Server, or the dialog a clicked invite link opens: its field
/// starts empty and takes the keys, unless a link opened it.
pub(super) fn join(
    state: &AppState,
    shell: &mut Shell,
    join: &mut Join,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let dialog = state.join.as_ref();
    let opened = follow(shell, Layer::Join, dialog.is_some());
    let Some(dialog) = dialog else {
        return;
    };
    if opened {
        join.link.clear();
        if !dialog.from_link {
            *focus = Some(anomi_screens::places::join::LINK);
        }
    }
    let preview = dialog.preview.as_ref();
    if let Some(icon) = preview.and_then(|preview| preview.icon.as_deref()) {
        requests.want(state, [icon]);
    }
    let found = preview.map(|preview| Found {
        name: preview.name.clone(),
        avatar: rows::avatar(
            state,
            &preview.name,
            &preview.server,
            preview.icon.as_deref(),
        ),
        detail: match (preview.members, preview.online) {
            (Some(members), Some(online)) if members >= 5 || online > 0 => trn!(
                "{} online, {n} member",
                "{} online, {n} members",
                members,
                online
            ),
            _ => place(preview.voice, &preview.channel),
        },
    });
    let confirm = if dialog.form.is_some() {
        tr!("Open in browser")
    } else if dialog.member {
        tr!("Open Server")
    } else if preview.is_some() {
        tr!("Join Server")
    } else if dialog.from_link {
        ""
    } else {
        tr!("Find Invite")
    };
    let shown = Join {
        title: if dialog.from_link {
            tr!("You've been invited to join a server")
        } else {
            tr!("Join a Server")
        }
        .to_owned(),
        field: !dialog.from_link,
        // The field's text is the app's.
        link: join.link.clone(),
        found,
        confirm: confirm.to_owned(),
        busy: dialog.resolving || dialog.joining,
        status: join_status(state).to_owned(),
        captcha: dialog.captcha.is_some(),
        form: dialog.form.is_some(),
        risk: preview.is_some() && !dialog.member && dialog.form.is_none() && !dialog.joining,
        error: dialog.error.clone().unwrap_or_default(),
    };
    if *join != shown {
        *join = shown;
    }
}

/// How joining goes, in a line.
fn join_status(state: &AppState) -> &'static str {
    let Some(dialog) = &state.join else {
        return "";
    };
    if dialog.form.is_some() {
        tr!(
            "You joined. This server asks new members to fill in a form before they can talk, which Yapfast can't show yet. Finish it in Discord's web app."
        )
    } else if dialog.browser {
        tr!("Finish joining in your browser. The server appears here once you're in.")
    } else if dialog.captcha.is_some() {
        tr!("Discord asks for a captcha first. Solve it in your browser.")
    } else if dialog.joining {
        tr!("Joining...")
    } else if dialog.resolving {
        tr!("Looking up the invite...")
    } else if dialog.member {
        tr!("You're already in this server.")
    } else {
        ""
    }
}

/// Leaving a server or group DM, or the move-sync question: one dialog,
/// open while the core asks either.
pub(super) fn leave(state: &AppState, shell: &mut Shell, leave: &mut Leave) {
    let shown = if let Some(dialog) = &state.leave {
        let name = &dialog.name;
        Leave {
            title: tr!("Leave '{}'", name),
            detail: if dialog.group {
                tr!(
                    "Are you sure you want to leave {}? You won't be able to rejoin this group unless you are re-invited.",
                    name
                )
            } else {
                tr!(
                    "Are you sure you want to leave {}? You won't be able to rejoin this server unless you are re-invited.",
                    name
                )
            },
            confirm: if dialog.group {
                tr!("Leave group")
            } else {
                tr!("Leave server")
            }
            .to_owned(),
            option: if dialog.group {
                tr!("Leave without notifying other members").to_owned()
            } else {
                String::new()
            },
            ticked: dialog.silent,
            danger: true,
            saving: dialog.leaving,
            error: dialog.error.clone().unwrap_or_default(),
            ..Leave::default()
        }
    } else if let Some(asked) = &state.move_sync {
        Leave {
            title: tr!("Sync permissions?").to_owned(),
            detail: tr!(
                "Would you like to sync {} permissions with {}?",
                asked.channel,
                asked.category
            ),
            confirm: tr!("Sync Permissions").to_owned(),
            alternative: tr!("Keep Current Permissions").to_owned(),
            ..Leave::default()
        }
    } else {
        follow(shell, Layer::Leave, false);
        return;
    };
    follow(shell, Layer::Leave, true);
    if *leave != shown {
        *leave = shown;
    }
}
