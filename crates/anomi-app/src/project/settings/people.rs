//! The frame and the pages about the account: who is signed in above the
//! sections, My Account (`show_account`, `src/ui/refresh.rs:1627`), Discord
//! with those it blocked (`:1241`), and Game activity (`src/ui/games.rs`).

use yapfast_core::application::AppState;
use yapfast_core::new_tags;

use anomi_vm::Vm;
use anomi_vm::settings::{account, discord, frame::Page, games};

use crate::layout::Layout;
use crate::pictures::Requests;
use crate::project::rows;

/// A Discord account is linked, as the workspace shows it.
pub(crate) fn linked(state: &AppState) -> bool {
    state.discord_linked || state.servers.iter().any(|server| server.discord)
}

/// Who is signed in and the pages with something new: compared before
/// anything is copied, on each refresh.
pub(super) fn frame(state: &AppState, layout: &Layout, vm: &mut Vm) {
    let (card, frame) = (&vm.account, &mut vm.settings.frame);
    if frame.name != card.name {
        frame.name.clone_from(&card.name);
    }
    if frame.avatar != card.avatar {
        frame.avatar = card.avatar.clone();
    }
    frame.may_edit_profile = linked(state);
    frame.with_discord = cfg!(feature = "discord");
    let about = new_tags::Page::About;
    let fresh = new_tags::page_new(layout.first_version.as_deref(), &layout.seen_new, about);
    if frame.fresh.contains(&Page::About) != fresh {
        frame.fresh = if fresh { vec![Page::About] } else { Vec::new() };
    }
}

pub(super) fn account(
    state: &AppState,
    card: &anomi_vm::account::Account,
    page: &mut account::Account,
) {
    let built = account::Account {
        name: card.name.clone(),
        avatar: card.avatar.clone(),
        decoration: card.decoration.clone(),
        status: card.status.clone(),
        storage: state
            .storage
            .map(yapfast_lang::text)
            .unwrap_or_default()
            .to_owned(),
        preview: state.preview,
        online: state.online,
        busy: state.busy,
        sending: state.sending.is_some(),
        linked: linked(state),
        has_custom_status: state.custom_status.is_some(),
    };
    if *page != built {
        *page = built;
    }
}

pub(super) fn discord(
    state: &AppState,
    layout: &Layout,
    page: &mut discord::Discord,
    requests: &mut Requests,
) {
    let blocked = &state.discord_blocked;
    requests.want(
        state,
        blocked.iter().filter_map(|person| person.avatar.as_deref()),
    );
    let built = discord::Discord {
        linked: linked(state),
        matrix_build: cfg!(feature = "matrix"),
        show_hidden_channels: layout.show_hidden_channels,
        sync_reads: layout.sync_discord_reads.unwrap_or(true),
        show_idle: layout.show_idle.unwrap_or(true),
        hide_blocked: layout.hide_blocked,
        blocked: (blocked.iter())
            .map(|person| discord::Blocked {
                id: person.id.clone(),
                name: person.name.clone(),
                username: person.username.clone().unwrap_or_default(),
                avatar: rows::avatar(state, &person.name, &person.id, person.avatar.as_deref()),
            })
            .collect(),
        blocked_shown: page.blocked_shown,
    };
    if *page != built {
        *page = built;
    }
}

/// The games named on this device, and the switch; the form's drafts are
/// the window's own and stay.
pub(super) fn games(state: &AppState, layout: &Layout, page: &mut games::Games) {
    page.shown = state.discord_show_game;
    page.linked = linked(state);
    let same = page.games.len() == layout.game_executables.len()
        && (page.games.iter().zip(&layout.game_executables)).all(|(shown, (executable, text))| {
            shown.executable == *executable && shown.text == *text
        });
    if !same {
        page.games = (layout.game_executables.iter())
            .map(|(executable, text)| games::Executable {
                executable: executable.clone(),
                text: text.clone(),
            })
            .collect();
    }
}
