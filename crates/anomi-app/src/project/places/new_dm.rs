//! The new DM and group DM dialog from the core's `new_dm`
//! (`src/ui/refresh.rs:3584`-`:3612`): who the search found, Discord's
//! first, built again only when they or the pictures change, and the line
//! under them.

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{AppState, NewDm as Dialog};
use yapfast_lang::tr;

use anomi_vm::new_dm::{NewDm, Person};
use anomi_vm::shell::{Layer, Shell};

use super::{follow, set};
use crate::pictures::Requests;
use crate::project::rows;

/// Matrix ids can be typed, and the rows mark who is on Discord.
const MATRIX: bool = cfg!(feature = "matrix");

/// Who the dialog listed last, as a hash of them, and the pictures then.
#[derive(Default)]
pub(super) struct Shown {
    key: Option<(u64, u64)>,
}

pub(super) fn show(
    state: &AppState,
    shell: &mut Shell,
    new_dm: &mut NewDm,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let dialog = state.new_dm.as_ref();
    if follow(shell, Layer::NewDm, dialog.is_some()) {
        *focus = Some(anomi_screens::new_dm::QUERY);
    }
    let Some(dialog) = dialog else {
        shown.key = None;
        return;
    };
    set(&mut new_dm.query, &dialog.query);
    new_dm.matrix = MATRIX;
    new_dm.starting = dialog.starting;
    let key = (found(dialog), state.picture_changes);
    if shown.key != Some(key) {
        requests.want(state, dialog.people().filter_map(|c| c.avatar.as_deref()));
        new_dm.people = dialog
            .people()
            .map(|contact| Person {
                id: contact.id.clone(),
                name: contact.name.clone(),
                detail: contact.detail.clone(),
                avatar: rows::avatar(
                    state,
                    &contact.name,
                    &contact.name,
                    contact.avatar.as_deref(),
                ),
                discord: MATRIX && contact.discord,
            })
            .collect();
        shown.key = Some(key);
    }
    set(&mut new_dm.note, note(dialog, new_dm.people.is_empty()));
}

/// Who was found, as a hash: a search's answer lists them anew.
fn found(dialog: &Dialog) -> u64 {
    let mut hasher = DefaultHasher::new();
    for contact in dialog.people() {
        (&contact.id, &contact.name, &contact.detail, &contact.avatar).hash(&mut hasher);
    }
    hasher.finish()
}

/// What the line under the rows says: why starting failed, that it's
/// starting, what to type, or that no one was found.
fn note(dialog: &Dialog, nobody: bool) -> &str {
    if let Some(error) = &dialog.error {
        return error;
    }
    if dialog.starting {
        return tr!("Starting the conversation...");
    }
    match nobody {
        false => "",
        true if dialog.query.trim().is_empty() && MATRIX => tr!(
            "Type a name to find people you know on Discord, or a full Matrix id such as @name:example.org."
        ),
        true if dialog.query.trim().is_empty() => {
            tr!("Type a name to find people you know on Discord.")
        }
        true if MATRIX => {
            tr!("No one found. For Matrix, type a full id such as @name:example.org.")
        }
        true => tr!("No one found."),
    }
}
