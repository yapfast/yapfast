//! The core's `AppState` as the screens' view-models: what
//! `src/ui/refresh.rs` does for the Slint window.
//!
//! [`refresh`] runs each area's `show`, behind the [`Reach`]es that can
//! change what the area shows: a backend event shows again only the areas
//! it can reach (who's typing reaches the composer alone), and an action
//! reaches everything. An area's `show` then decides whether to rebuild:
//!
//! - an area that builds rows keeps a key of what it showed last (the
//!   core's change counters such as `list_changes` and `picture_changes`,
//!   the selection, the settings it reads, its own UI state) in [`Shown`],
//!   and returns at once when the key is the same. A key must cover
//!   everything the rows read, or the area goes stale;
//! - an area of a few fields sets each, comparing before it copies text.
//!
//! Every area's module is here, its I task's (docs/iced/tasks/integration.md).
//! A task fills its module, adds its `show` line to [`refresh`] under the
//! reaches that can change it, and its key to [`Shown`].

pub mod account;
pub(crate) mod chat;
mod editor;
pub(crate) mod header;
mod inbox;
pub(crate) mod menus;
mod people;
mod places;
mod rail;
pub mod reach;
pub mod rows;
mod server_settings;
mod settings;
mod shell;
pub mod sidebar;
mod sign_in;
#[cfg(test)]
mod tests;
mod voice;

pub use reach::Reach;
pub(crate) use sign_in::{forget, mfa_method};

use yapfast_core::application::Application;

use anomi_vm::Vm;

use crate::layout::Layout;
use crate::pictures::Requests;

/// What each area showed last.
#[derive(Default)]
pub struct Shown {
    rail: rail::Shown,
    pub sidebar: sidebar::Shown,
    browse: sidebar::BrowseShown,
    account: account::Shown,
    header: header::Shown,
    inbox: inbox::Shown,
    sign_in: sign_in::Shown,
    places: places::Shown,
    pub(crate) timeline: chat::timeline::Shown,
    reactions: chat::reactions::Shown,
    /// The window's menu and the message menu (I10).
    pub(crate) menus: menus::Shown,
    pub(crate) edit: chat::edit::Shown,
    forward: chat::forward::Shown,
    /// The field a projection asked to take the keys as it showed, which
    /// `App::update` focuses.
    pub(crate) focus: Option<&'static str>,
}

/// The rail's rows in view, by the id of each server, folder and DM: their
/// pictures are asked for, now and as the rail is built again.
pub fn rail_seen(
    state: &yapfast_core::application::AppState,
    shown: &mut Shown,
    ids: Vec<String>,
    requests: &mut Requests,
) {
    rail::seen_now(state, &mut shown.rail, ids, requests);
}

/// The DM list's rows in view, by room id, as [`rail_seen`].
pub fn dms_seen(
    state: &yapfast_core::application::AppState,
    shown: &mut Shown,
    ids: Vec<String>,
    requests: &mut Requests,
) {
    sidebar::seen(state, &mut shown.sidebar, ids, requests);
}

/// The inbox's rows in view, by room id, as [`rail_seen`].
pub fn inbox_seen(
    state: &yapfast_core::application::AppState,
    shown: &mut Shown,
    ids: Vec<String>,
    requests: &mut Requests,
) {
    inbox::seen(state, &mut shown.inbox, ids, requests);
}

/// A dialog about places played its way out: the rows it held go.
pub fn places_gone(layer: anomi_vm::shell::Layer, vm: &mut Vm, shown: &mut Shown) {
    places::gone(layer, vm, &mut shown.places);
}

/// The Reactions window played its way out: its rows go.
pub fn reactions_gone(vm: &mut Vm, shown: &mut Shown) {
    chat::reactions::gone(&mut vm.chat.reactions, &mut shown.reactions);
}

/// The forward dialog played its way out: its places go.
pub fn forward_gone(vm: &mut Vm, shown: &mut Shown) {
    chat::forward::gone(&mut vm.chat.forward, &mut shown.forward);
}

/// The inbox played its way out: its rows go until it opens again.
pub fn inbox_gone(vm: &mut Vm, shown: &mut Shown) {
    inbox::gone(&mut vm.inbox, &mut shown.inbox);
}

impl Shown {
    /// Shows every area again on the next refresh, as after the language
    /// changes: their text was made in the old one.
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

/// Shows the state in the areas `reach` reaches whose inputs changed.
///
/// What reaches which area, as the Slint app's `Adapter::show`: the
/// composers and the thread beside a channel follow [`Reach::TYPING`]; the
/// call's views, and the places listing who's in a voice channel,
/// [`Reach::VOICE`]; whatever shows avatars, server icons or emoji as
/// pictures, [`Reach::PICTURES`] (and [`Reach::EMOJI`] for custom emoji);
/// the GIF and sticker pickers [`Reach::PICKER`], the emoji picker
/// [`Reach::EMOJI`]; the video and the voice message playing
/// [`Reach::VIDEO`]. Everything follows [`Reach::STATE`].
pub fn refresh(
    core: &Application,
    root: &std::path::Path,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
    reach: Reach,
) {
    use Reach as R;
    let state = &core.state;
    if reach.touches(R::STATE) {
        shell::show(state, layout, vm);
        sidebar::show_browse(state, &mut vm.browse, &mut shown.browse);
        sign_in::show(core, root, vm, &mut shown.sign_in, &mut shown.focus);
    } else if reach.touches(R::SETTINGS) {
        // A keystroke in the two-factor prompt.
        sign_in::show_mfa(core, vm, &mut shown.sign_in, &mut shown.focus);
    }
    if reach.touches(R::STATE | R::PICTURES | R::VOICE) {
        rail::show(state, layout, &mut vm.rail, &mut shown.rail, requests);
        sidebar::show(core, layout, &mut vm.sidebar, &mut shown.sidebar, requests);
        // The header's chevron, and a menu's Apps that came late.
        menus::show(core, layout, vm, &mut shown.menus);
    }
    if reach.touches(R::STATE | R::VOICE) {
        account::voice(state, layout, &mut vm.account);
    }
    // The call's phase and whether this account is in the open room's
    // call show in the header's call buttons; its key holds who's speaking
    // out.
    if reach.touches(R::STATE | R::PICTURES | R::EMOJI | R::VOICE) {
        header::show(
            state,
            layout,
            vm.people.friends.all,
            &mut vm.shell,
            &mut vm.header,
            &mut shown.header,
            requests,
        );
    }
    // The switcher's and the new DM's keystrokes reach these alone.
    if reach.touches(R::STATE | R::PICTURES | R::PLACES) {
        places::show(
            state,
            layout,
            vm,
            &mut shown.places,
            requests,
            &mut shown.focus,
        );
    }
    // A picture, a custom emoji's too, reaches only the rows that show it.
    if reach.touches(R::STATE | R::PICTURES | R::EMOJI) {
        let open = vm.conversation.body == anomi_vm::conversation::Body::Messages;
        chat::timeline::show(
            core,
            layout,
            open,
            &mut vm.chat.timeline,
            &mut shown.timeline,
            requests,
        );
        chat::reactions::dialog(
            state,
            &mut vm.shell,
            &mut vm.chat.reactions,
            &mut shown.reactions,
            requests,
        );
        chat::forward::show(
            state,
            &mut vm.shell,
            &mut vm.chat.forward,
            &mut shown.forward,
            requests,
            &mut shown.focus,
        );
    }
    // The message edited in place, in its row as the timeline built it; a
    // keystroke in its field reaches it alone.
    if reach.touches(R::STATE | R::PICTURES | R::EMOJI | R::COMPOSER) {
        chat::edit::show(
            state,
            &mut vm.chat.timeline,
            &mut shown.edit,
            &mut shown.focus,
        );
    }
    if reach.touches(R::STATE | R::PICTURES) {
        inbox::show(state, &vm.shell, &mut vm.inbox, &mut shown.inbox, requests);
        account::show(state, &mut vm.account, &mut shown.account, requests);
        account::status::show(state, &mut vm.shell, &mut vm.people.status, requests);
    }
}
