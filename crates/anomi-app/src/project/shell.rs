//! The window's frame and the conversation column from the core's state
//! and the layout: whether the account's screens show, the notice, what
//! fills the column and what is docked beside it (`show_workspace`,
//! `src/ui/refresh.rs:960`, and the column's conditions,
//! `ui/yapfast.slint:3441`-`:4094`). A handful of fields, so each is set
//! rather than keyed. The panels' widths are the app's own (`App::new`
//! takes them from the layout, `intent::shell` as their edges are dragged),
//! the zoom note `crate::zoom`'s.

use yapfast_core::application::{AppState, DmScreen, Navigation};

use anomi_vm::Vm;
use anomi_vm::conversation::{Aside, Beside, Body};

use crate::layout::Layout;

pub(super) fn show(state: &AppState, layout: &Layout, vm: &mut Vm) {
    let shell = &mut vm.shell;
    shell.signed_out = !(state.preview || state.account.is_some());
    shell.connecting = state.busy && state.rooms.is_empty();
    if shell.notice != state.notice {
        shell.notice.clone_from(&state.notice);
    }
    let conversation = &mut vm.conversation;
    conversation.body = body(state);
    conversation.beside = beside(state, layout);
    conversation.preview = state.preview;
}

/// What fills the column: a screen of the DM home or a server's Browse
/// Channels over any room, else the room's stage, posts or messages.
fn body(state: &AppState) -> Body {
    let home = state.navigation == Navigation::DirectMessages;
    match state.dm_screen {
        Some(DmScreen::Requests) if home => return Body::Requests,
        Some(DmScreen::Friends) if home => return Body::Friends,
        _ => {}
    }
    if state.browse.is_some() {
        return Body::Browse;
    }
    match state.selected_room() {
        None => Body::Nothing,
        // `voice-panel`: a voice room shows its call, not its chat.
        Some(room) if room.voice && !state.voice_chat => Body::Voice,
        Some(room) if room.forum => Body::Forum,
        Some(_) => Body::Messages,
    }
}

/// What the column wants docked at its right (`search-open`, `aside-open`,
/// `new-thread-open`, `members-available && member-list`,
/// `profile-available && profile-panel && panel-profile-shown`).
fn beside(state: &AppState, layout: &Layout) -> Beside {
    let thread = if state.new_thread().is_some() {
        Some(Aside::NewThread)
    } else {
        state.aside.as_ref().map(|_| Aside::Thread)
    };
    Beside {
        search: state.search_shown().is_some(),
        thread,
        members: state.lists_members() && !layout.members_hidden,
        profile: state.one_to_one().is_some()
            && state.profile_panel
            && state.panel_profile.is_some(),
    }
}
