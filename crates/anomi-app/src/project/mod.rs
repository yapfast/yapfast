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

mod account;
mod chat;
mod editor;
mod header;
mod inbox;
mod menus;
mod people;
mod places;
mod rail;
pub mod reach;
pub mod rows;
mod server_settings;
mod settings;
mod shell;
mod sidebar;
mod sign_in;
#[cfg(test)]
mod tests;
mod voice;

pub use reach::Reach;

use yapfast_core::application::AppState;

use anomi_vm::Vm;

use crate::layout::Layout;

/// What each area showed last.
#[derive(Default)]
pub struct Shown {
    rail: Option<rail::Key>,
    sidebar: Option<sidebar::Key>,
    account: Option<account::Key>,
    header: Option<header::Key>,
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
pub fn refresh(state: &AppState, layout: &Layout, vm: &mut Vm, shown: &mut Shown, reach: Reach) {
    use Reach as R;
    if reach.touches(R::STATE) {
        shell::show(state, layout, vm);
    }
    if reach.touches(R::STATE | R::PICTURES | R::VOICE) {
        rail::show(state, &mut vm.rail, &mut shown.rail);
        sidebar::show(state, &mut vm.sidebar, &mut shown.sidebar);
        account::show(state, layout, &mut vm.account, &mut shown.account);
    }
    if reach.touches(R::STATE | R::PICTURES) {
        header::show(state, &mut vm.header, &mut shown.header);
    }
}
