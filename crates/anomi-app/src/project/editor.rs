//! The profile editor from the core's `profile_editor`
//! (`show_profile_editor`, `src/ui/refresh.rs:3036`): `Layer::ProfileEditor`
//! open while the core has an editor, its card the popout's own card over
//! the profile as written ([`view`]), its words and switches the core's
//! ([`fill`]).
//!
//! The core counts every change to its editor (`profile_editor_changes`):
//! the editor is made again only when that count, or what else its card
//! reads, moved, or one of the pictures it shows came. What only the
//! window knows (which text is being edited, the banner colour's box, the
//! styles panel, a theme colour's picker, the tab shown) stays across
//! that, and goes with the editor once it played its way out ([`gone`]),
//! with the pictures it held.

mod fill;
#[cfg(test)]
pub(crate) mod tests;
mod view;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::{AppState, Application, Cosmetics};

use anomi_vm::Vm;
use anomi_vm::kind::Presence;
use anomi_vm::people::editor::Editor;
use anomi_vm::people::name_style::NameStyle;
use anomi_vm::shell::Layer;

use crate::pictures::{self, Requests, Watch};
use crate::project::people::card;

/// The narrowest window whose editor opens with its styles panel out
/// (`side-open: root.width >= 1100px`).
const SIDE_FROM: f32 = 1100.0;

/// What the editor was made from.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<Key>,
    pictures: Watch<()>,
    /// The window's width, which the styles panel opens out in or folded;
    /// 0 until the window says it.
    pub(crate) width: f32,
}

/// What the editor reads besides the core's editor.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    editor: u64,
    /// The servers, whose tags it may wear.
    lists: u64,
    cosmetics: Cosmetics,
    effects: (usize, usize),
    /// One's own custom status, the card's thought.
    status: u64,
    /// What one's sessions say one does, which one's card shows.
    activities: usize,
}

impl Key {
    fn of(state: &AppState) -> Self {
        let mut hasher = DefaultHasher::new();
        if let Some(status) = &state.custom_status {
            (&status.text, &status.emoji, &status.emoji_id).hash(&mut hasher);
        }
        Self {
            editor: state.profile_editor_changes,
            lists: state.list_changes,
            cosmetics: state.cosmetics,
            effects: (
                state.profile_effects.len(),
                state
                    .profile_effects
                    .last()
                    .map_or(0, |effect| Arc::as_ptr(effect).addr()),
            ),
            status: hasher.finish(),
            activities: Arc::as_ptr(&state.discord_activities).cast::<()>().addr(),
        }
    }
}

/// Opens, shows and closes the editor as the core has it.
pub(super) fn show(core: &Application, vm: &mut Vm, shown: &mut Shown, requests: &mut Requests) {
    let state = &core.state;
    let Some(editor) = &state.profile_editor else {
        if vm.shell.is_open(Layer::ProfileEditor) {
            vm.shell.close(Layer::ProfileEditor);
            vm.people.name_style.open = false;
        }
        shown.key = None;
        return;
    };
    if !vm.shell.is_open(Layer::ProfileEditor) {
        // A new editor: what the window kept of the last one goes.
        vm.people.editor = Editor {
            side_open: shown.width == 0.0 || shown.width >= SIDE_FROM,
            ..Editor::default()
        };
        vm.people.name_style = NameStyle::default();
        shown.key = None;
        vm.shell.open(Layer::ProfileEditor);
    }
    let key = Key::of(state);
    let moved = !shown.pictures.moved(state).is_empty();
    if moved || shown.key != Some(key) {
        let made = pictures::noting(state, &mut shown.pictures, || view::card(core, editor));
        // The effect's tile shows its still, which nothing asks for else.
        let fallback = editor.profile.as_ref().map(|profile| &profile.fallback);
        if let Some(effect) = fallback.and_then(|own| card::effect_of(state, own)) {
            for address in effect.pictures() {
                shown.pictures.note(state, address, ());
            }
        }
        requests.want(state, shown.pictures.addresses());
        fill::fill(state, editor, made, &mut vm.people.editor);
        shown.key = Some(key);
    }
    // The dot and the nameplate the account card shows.
    let (account, edited) = (&vm.account, &mut vm.people.editor);
    if account.presence != Presence::None {
        edited.card.presence = account.presence;
    }
    if edited.nameplate != account.plate {
        edited.nameplate = account.plate.clone();
    }
}

/// The editor played its way out: what it showed goes, its pictures with
/// it, unless the core shows an editor again.
pub(crate) fn gone(state: &AppState, vm: &mut Vm, shown: &mut Shown) {
    if state.profile_editor.is_some() {
        return;
    }
    vm.people.editor = Editor::default();
    vm.people.name_style = NameStyle::default();
    *shown = Shown {
        width: shown.width,
        ..Shown::default()
    };
}

/// A second passed: the preview's activities say their times again, while
/// their tab shows.
pub(crate) fn tick(vm: &mut Vm) {
    if vm.shell.is_open(Layer::ProfileEditor) && seconds(vm) {
        vm.people.editor.preview.now = crate::project::people::lines::now();
    }
}

/// Whether the preview shows an activity's time counting.
pub(crate) fn seconds(vm: &Vm) -> bool {
    use anomi_vm::people::editor_preview::Page;
    let preview = &vm.people.editor.preview;
    vm.shell.is_open(Layer::ProfileEditor)
        && preview.page == Page::Activity
        && anomi_vm::people::activity::card::wants_clock(&preview.activities, preview.now)
}
