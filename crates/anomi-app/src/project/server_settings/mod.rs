//! A server's and a channel's settings from the core's state (task I20),
//! ported from `show_server_settings` (`src/ui/server_settings.rs:116`),
//! `show_channel_settings` (`src/ui/channel_settings.rs:41`) and
//! `show_channel_dialog` (`src/ui/refresh.rs:2939`).
//!
//! `Layer::ServerSettings` is open while the core holds a server's
//! settings, `Layer::ChannelSettings` a channel's and `Layer::CreateChannel`
//! the dialog that makes, copies or deletes a channel. Only the page that
//! shows is built, from the core's draft, while the reaches that can change
//! it come: a keystroke in a field (`Reach::SETTINGS`) builds that page and
//! nothing else in the window. A dialog over the settings has a layer of its
//! own, open while its page holds what it asks; its model stays as the layer
//! plays its way out and goes on its `Gone` ([`gone`]). The pages hold
//! nothing once the settings went: the core let go of what they loaded.

mod channel;
mod create;
mod expressions;
mod members;
mod moderation;
mod ownership;
mod pages;
mod roles;
#[cfg(test)]
pub(crate) mod tests;

use yapfast_core::application::{Application, ServerSettings, SettingsPage};
use yapfast_core::chat::SettingsSection;
use yapfast_lang::tr;

use anomi_vm::Vm;
use anomi_vm::settings::frame::Unsaved;
use anomi_vm::settings::server::{Section, Server};
use anomi_vm::shell::{Layer, Shell};

use super::Reach;
use crate::layout::Layout;

/// What the settings showed last, and what only the window keeps beside the
/// core's draft.
#[derive(Default)]
pub(crate) struct Shown {
    /// The server whose settings show.
    server: Option<String>,
    /// The role open, and the colour its hex field was last written from.
    pub(crate) role: Option<(String, u32)>,
    /// The members' search waiting for the typing to stop, by generation.
    pub(crate) search: u64,
    pub(crate) channel: channel::Shown,
    create: bool,
    /// The window's size, which Members' table fits.
    pub(crate) window: iced::Size,
    /// Security Actions' and the ban's dialogs, which only the window
    /// opens over their pages, by what they are about.
    pub(crate) ban: Option<String>,
}

/// The core's section as the window names it.
pub(crate) fn section(section: SettingsSection) -> Section {
    match section {
        SettingsSection::Profile => Section::Profile,
        SettingsSection::Engagement => Section::Engagement,
        SettingsSection::BoostPerks => Section::BoostPerks,
        SettingsSection::Emoji => Section::Emoji,
        SettingsSection::Stickers => Section::Stickers,
        SettingsSection::Members => Section::Members,
        SettingsSection::Roles => Section::Roles,
        SettingsSection::Invites => Section::Invites,
        SettingsSection::Safety => Section::Safety,
        SettingsSection::AuditLog => Section::AuditLog,
        SettingsSection::Bans => Section::Bans,
    }
}

/// The window's section as the core names it.
pub(crate) fn core_section(section: Section) -> SettingsSection {
    match section {
        Section::Profile => SettingsSection::Profile,
        Section::Engagement => SettingsSection::Engagement,
        Section::BoostPerks => SettingsSection::BoostPerks,
        Section::Emoji => SettingsSection::Emoji,
        Section::Stickers => SettingsSection::Stickers,
        Section::Members => SettingsSection::Members,
        Section::Roles => SettingsSection::Roles,
        Section::Invites => SettingsSection::Invites,
        Section::Safety => SettingsSection::Safety,
        Section::AuditLog => SettingsSection::AuditLog,
        Section::Bans => SettingsSection::Bans,
    }
}

/// Shows the server's and the channel's settings and the channel dialog,
/// as the core has them.
pub fn show(
    core: &Application,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut super::Shown,
    reach: Reach,
) {
    if !reach.touches(Reach::STATE | Reach::SETTINGS | Reach::PICTURES | Reach::EMOJI) {
        return;
    }
    let (focus, shown) = (&mut shown.focus, &mut shown.server_settings);
    server(core, layout, vm, shown, focus);
    channel::show(core, vm, &mut shown.channel, focus);
    create::show(core, vm, &mut shown.create, focus);
}

/// The dialogs over a server's settings, each open while its model holds
/// what it asks.
const DIALOGS: [Layer; 7] = [
    Layer::SecurityActions,
    Layer::Ban,
    Layer::RoleConfirm,
    Layer::RoleAdding,
    Layer::EmojiUpload,
    Layer::StickerForm,
    Layer::Ownership,
];

fn server(
    core: &Application,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    let Some(settings) = core.state.server_settings.as_ref() else {
        if vm.shell.is_open(Layer::ServerSettings) {
            vm.shell.close(Layer::ServerSettings);
            for layer in DIALOGS {
                vm.shell.close(layer);
            }
        }
        shown.server = None;
        shown.role = None;
        shown.ban = None;
        return;
    };
    if !vm.shell.is_open(Layer::ServerSettings) {
        vm.shell.open(Layer::ServerSettings);
    }
    if shown.server.as_deref() != Some(settings.server.as_str()) {
        shown.server = Some(settings.server.clone());
        shown.role = None;
    }
    frame(core, settings, &mut vm.settings.server);
    let state = &core.state;
    let server = settings.server.as_str();
    let (shell, all) = (&mut vm.shell, &mut vm.settings);
    match &settings.page {
        Some(SettingsPage::Profile(page)) => pages::profile(state, settings, page, &mut all.server),
        Some(SettingsPage::Engagement(page)) => pages::engagement(core, server, page, all),
        Some(SettingsPage::BoostPerks(page)) => pages::boosts(state, page, all),
        Some(SettingsPage::Safety(page)) => pages::safety(core, server, page, all),
        Some(SettingsPage::Roles(page)) => {
            roles::show(core, settings, page, all, shell, shown, focus);
        }
        Some(SettingsPage::Members(page)) => members::show(core, server, page, all, shown.window),
        Some(SettingsPage::Bans(page)) => moderation::bans(core, server, page, all, shell, shown),
        Some(SettingsPage::AuditLog(page)) => moderation::audit(state, layout, page, all),
        Some(SettingsPage::Invites(page)) => moderation::invites(core, server, page, all, shell),
        Some(SettingsPage::Emoji(page)) => {
            expressions::emoji(core, server, page, all, shell, focus)
        }
        Some(SettingsPage::Stickers(page)) => {
            expressions::stickers(core, server, page, all, shell, focus);
        }
        None => {}
    }
    // A dialog whose page went with it closes.
    let page = settings.page.as_ref();
    if !matches!(page, Some(SettingsPage::Roles(_))) {
        dialog(
            shell,
            Layer::RoleConfirm,
            &mut all.role_editor.confirm,
            None,
        );
        dialog(shell, Layer::RoleAdding, &mut all.role_editor.adding, None);
    }
    if !matches!(page, Some(SettingsPage::Bans(_))) {
        dialog(shell, Layer::Ban, &mut all.bans.open, None);
    }
    if !matches!(page, Some(SettingsPage::Invites(_))) {
        dialog(shell, Layer::SecurityActions, &mut all.safety.actions, None);
    }
    if !matches!(page, Some(SettingsPage::Emoji(_))) {
        dialog(shell, Layer::EmojiUpload, &mut all.emoji.upload, None);
    }
    if !matches!(page, Some(SettingsPage::Stickers(_))) {
        dialog(shell, Layer::StickerForm, &mut all.stickers.form, None);
    }
    ownership::show(core, settings, all, shell, focus);
}

/// The frame: the sections this account may open, the title, the bar for
/// changes not saved.
fn frame(core: &Application, settings: &ServerSettings, vm: &mut Server) {
    let name = core
        .state
        .servers
        .iter()
        .find(|known| known.id == settings.server)
        .map_or("", |known| known.name.as_str());
    if vm.name != name {
        name.clone_into(&mut vm.name);
    }
    let sections: Vec<Section> = core
        .settings_sections(&settings.server)
        .into_iter()
        .map(section)
        .collect();
    if vm.sections != sections {
        vm.sections = sections;
    }
    vm.may_delete = core.may_give_up(&settings.server);
    vm.section = section(settings.section);
    vm.loading = settings.loading();
    vm.loaded = settings.page.is_some();
    vm.saving = settings.saving;
    set(
        &mut vm.problem,
        settings.problem().as_deref().unwrap_or_default(),
    );
    set(&mut vm.error, settings.error.as_deref().unwrap_or_default());
    vm.unsaved =
        ((settings.changed() || settings.saving) && !settings.immediate()).then(|| Unsaved {
            saving: settings.saving,
            may_reset: !settings.saving,
            may_save: settings.ready(),
            nudge: settings.nudge,
        });
    if vm.close.is_empty() {
        vm.close = tr!("Close {}", tr!("Server settings"));
    }
    // An open role titles the bar; Boost Perks says the level beside.
    let role = match &settings.page {
        Some(SettingsPage::Roles(page)) => page.open_role().map(|role| {
            if page.is_everyone(&role.id) {
                tr!("Default Permissions").to_owned()
            } else {
                tr!("Edit Role — {}", role.name)
            }
        }),
        _ => None,
    };
    if vm.title != role {
        vm.title = role;
    }
    let detail = match &settings.page {
        Some(SettingsPage::BoostPerks(page)) => match page.original.premium_tier {
            0 => tr!("No Level").to_owned(),
            tier => tr!("Level {}", tier),
        },
        _ => String::new(),
    };
    set(&mut vm.detail, &detail);
    vm.fill = match &settings.page {
        Some(SettingsPage::Roles(page)) => page
            .open
            .as_ref()
            .is_some_and(|draft| draft.tab != yapfast_core::application::RoleTab::Display),
        Some(
            SettingsPage::Bans(_)
            | SettingsPage::AuditLog(_)
            | SettingsPage::Members(_)
            | SettingsPage::Invites(_)
            | SettingsPage::Emoji(_),
        ) => true,
        _ => false,
    };
}

/// Whether Invites shows something counting down: the clocks tick each
/// second for it while the window is in front.
pub(crate) fn ticking(vm: &Vm) -> bool {
    vm.shell.is_open(Layer::ServerSettings)
        && vm.settings.server.section == Section::Invites
        && vm.settings.invites.wants_ticks()
}

/// A second passed, or the window came to the front: Invites' countdowns
/// read the clock.
pub(crate) fn tick(vm: &mut Vm) {
    if vm.shell.is_open(Layer::ServerSettings) && vm.settings.server.section == Section::Invites {
        vm.settings.invites.tick(std::time::Instant::now());
    }
}

/// A text the view-model holds, changed only when it differs.
pub(super) fn set(slot: &mut String, text: &str) {
    if slot != text {
        text.clone_into(slot);
    }
}

/// A dialog's layer, open while `now` holds what it asks: its model takes
/// it; once it is none the layer plays its way out with what it showed,
/// and its `Gone` clears the model.
pub(super) fn dialog<T: PartialEq>(
    shell: &mut Shell,
    layer: Layer,
    model: &mut Option<T>,
    now: Option<T>,
) -> bool {
    match now {
        Some(now) => {
            if model.as_ref() != Some(&now) {
                *model = Some(now);
            }
            let opened = !shell.is_open(layer);
            if opened {
                shell.open(layer);
            }
            opened
        }
        None => {
            if shell.is_open(layer) {
                shell.close(layer);
            }
            false
        }
    }
}

/// A layer of these settings played its way out: what it showed goes.
pub fn gone(vm: &mut Vm, shown: &mut Shown, layer: Layer) {
    let settings = &mut vm.settings;
    match layer {
        Layer::ServerSettings => {
            settings.server = Server::default();
            settings.engagement = Default::default();
            settings.boosts = Default::default();
            settings.members = Default::default();
            settings.invites = Default::default();
            settings.safety = Default::default();
            settings.bans = Default::default();
            settings.audit = Default::default();
            settings.roles = Default::default();
            settings.role_editor = Default::default();
            settings.emoji = Default::default();
            settings.stickers = Default::default();
            settings.ownership = Default::default();
            shown.role = None;
            shown.ban = None;
        }
        Layer::SecurityActions => settings.safety.actions = None,
        Layer::Ban => {
            settings.bans.open = None;
            shown.ban = None;
        }
        Layer::RoleConfirm => settings.role_editor.confirm = None,
        Layer::RoleAdding => settings.role_editor.adding = None,
        Layer::EmojiUpload => settings.emoji.upload = None,
        Layer::StickerForm => settings.stickers.form = None,
        Layer::Ownership => settings.ownership = Default::default(),
        Layer::ChannelSettings => {
            settings.channel = Default::default();
            shown.channel = channel::Shown::default();
        }
        Layer::PermissionsConfirm => settings.channel.permissions.confirm = None,
        Layer::PermissionsAdd => settings.channel.permissions.adding = None,
        Layer::CreateChannel => settings.create_channel = Default::default(),
        _ => {}
    }
}
