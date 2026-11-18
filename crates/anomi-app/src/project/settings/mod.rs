//! The person's settings from the core's state and the layout: the frame
//! and the one page that shows, filled as Settings opens and as what the
//! page reads changes, and nothing while Settings is closed (task I19). The
//! Slint side: `show_account` (`src/ui/refresh.rs:1627`), the layout's
//! `apply` (`src/ui.rs:1728`), `src/ui/appearance.rs`, `src/ui/updates.rs`
//! and the soundboard's settings (`refresh.rs:4838`).
//!
//! A page is built again only when its [`Key`] changed: the page, the
//! layout's changes, what the intents keep beside it ([`Kept::version`])
//! and a stamp of the core's state the page reads, made without copying it.
//! Typing in a field (the sections' search, a game's, the theme editor's)
//! changes the view-model alone and builds nothing.

pub(crate) mod appearance;
pub(crate) mod pages;
mod people;
pub(crate) mod voice;

use std::hash::{Hash, Hasher};

use yapfast_core::application::Application;

use anomi_vm::Vm;
use anomi_vm::settings::frame::Page;
use anomi_vm::shell::Layer;

use super::Reach;
use crate::layout::Layout;
use crate::pictures::Requests;

pub use appearance::Kept;

/// What Settings showed last, and what the intents keep beside it.
#[derive(Default)]
pub struct Shown {
    key: Option<Key>,
    pub(crate) kept: Kept,
    /// The voice page's devices' names, by id (`voice::show`).
    device_names: std::collections::HashMap<String, String>,
}

impl Shown {
    /// As the language changes: the pages are made again, what's kept stays.
    pub fn forgotten(self) -> Self {
        Self {
            kept: self.kept,
            ..Self::default()
        }
    }
}

/// Everything the page showing reads.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    page: Page,
    edits: u64,
    kept: u64,
    core: u64,
}

/// Shows the frame and its page while Settings is open; on its way out it
/// stays as it was until [`gone`].
pub fn show(
    core: &Application,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
    reach: Reach,
) {
    let reaches = Reach::STATE | Reach::SETTINGS | Reach::PICTURES | Reach::VOICE;
    if !vm.shell.is_open(Layer::Settings) || !reach.touches(reaches) {
        return;
    }
    let state = &core.state;
    people::frame(state, layout, vm);
    let page = vm.settings.frame.page;
    let key = Key {
        page,
        edits: layout.edits(),
        kept: shown.kept.version,
        core: stamp(core, page),
    };
    if shown.key == Some(key) {
        return;
    }
    if shown.key.is_none_or(|before| before.page != page) {
        let frame = &mut vm.settings.frame;
        frame.build = Some(crate::build_info::current());
        frame.close = yapfast_lang::tr!("Close {}", yapfast_lang::tr!("Settings"));
    }
    shown.key = Some(key);
    let (settings, kept, names) = (&mut vm.settings, &shown.kept, &mut shown.device_names);
    match page {
        Page::Account => people::account(state, &vm.account, &mut settings.account),
        Page::Voice => voice::show(core, layout, kept, names, &mut settings.voice),
        Page::Messages => pages::messages(layout, &mut settings.messages),
        Page::Notifications => pages::notifications(layout, &mut settings.notifications),
        Page::Sounds => pages::sounds(layout, &mut settings.sounds),
        Page::Appearance => appearance::show(layout, kept, &mut settings.appearance),
        Page::Discord => people::discord(state, layout, &mut settings.discord, requests),
        Page::Games => people::games(state, layout, &mut settings.games),
        Page::Keybinds => {}
        Page::Advanced => pages::advanced(layout, kept, &mut settings.advanced),
        Page::About => pages::about(layout, &mut settings.about),
    }
}

/// A stamp of what the page reads of the core's state, without copying it.
fn stamp(core: &Application, page: Page) -> u64 {
    let state = &core.state;
    let mut hasher = std::hash::DefaultHasher::new();
    match page {
        Page::Account => {
            (
                state.preview,
                state.online,
                state.busy,
                state.sending.is_some(),
            )
                .hash(&mut hasher);
            (state.custom_status.is_some(), state.discord_linked).hash(&mut hasher);
            state.storage.hash(&mut hasher);
        }
        Page::Voice => voice::stamp(core, &mut hasher),
        Page::Discord => {
            (
                state.discord_linked,
                state.servers.iter().any(|s| s.discord),
            )
                .hash(&mut hasher);
            (state.people_changes, state.picture_changes).hash(&mut hasher);
        }
        Page::Games => {
            (
                state.discord_linked,
                state.servers.iter().any(|s| s.discord),
            )
                .hash(&mut hasher);
            state.discord_show_game.hash(&mut hasher);
        }
        _ => {}
    }
    hasher.finish()
}

/// Settings played its way out: the person's pages hold nothing until they
/// open again. The page chosen stays, which the ways in set.
pub fn gone(vm: &mut Vm, shown: &mut Shown) {
    shown.key = None;
    let settings = &mut vm.settings;
    let page = settings.frame.page;
    settings.frame = anomi_vm::settings::frame::Frame {
        page,
        ..Default::default()
    };
    settings.account = Default::default();
    settings.voice = Default::default();
    settings.messages = Default::default();
    settings.notifications = Default::default();
    settings.sounds = Default::default();
    settings.appearance = Default::default();
    settings.theme_editor = Default::default();
    settings.discord = Default::default();
    settings.games = Default::default();
    settings.keybinds = Default::default();
    settings.advanced = Default::default();
    settings.about = Default::default();
}
