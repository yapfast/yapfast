//! A Discord channel's or category's settings, as their screen shows them;
//! see `ui/channel_settings.slint`.

use yapfast_core::{
    application::{
        ChannelInput, ChannelSection, OverwriteConfirm, OverwriteState, Overwrites, SLOWMODES,
        channel_groups, channel_permission,
    },
    chat::{ChannelKind, ChannelOverwrite},
};

use super::{
    server_settings::{member_row, role_color},
    *,
};

/// Discord's words for a permission this account can't change.
const NOT_HELD: &str = "You cannot modify this permission because none of your roles have it.";

/// Delete Channel's id in the sidebar, which closes the settings and asks.
const DELETE_CHANNEL: i32 = 99;

fn section_id(section: ChannelSection) -> i32 {
    match section {
        ChannelSection::Overview => 0,
        ChannelSection::Permissions => 1,
    }
}

fn state_id(state: OverwriteState) -> i32 {
    match state {
        OverwriteState::Deny => 0,
        OverwriteState::Neutral => 1,
        OverwriteState::Allow => 2,
    }
}

impl Adapter {
    /// The open channel's settings, as their screen shows them.
    pub(super) fn show_channel_settings(&mut self, window: &YapfastWindow) {
        let app = &self.app;
        let Some(screen) = app.state.channel_settings.as_ref() else {
            if window.get_channel_settings_open() {
                window.set_channel_settings_open(false);
                window.set_channel_settings_prompt(ChannelSettingsPrompt::default());
            }
            return;
        };
        window.set_channel_settings_open(true);
        let room = &screen.room;
        let title = app.channel_title(room);
        let channel = if screen.kind == ChannelKind::Text {
            format!("#{title}")
        } else {
            title
        };
        window.set_channel_settings_channel(channel.as_str().into());
        let mut rows = vec![SettingsRow {
            heading: true,
            text: channel.to_uppercase().into(),
            ..Default::default()
        }];
        for section in app.channel_sections(room) {
            let (text, icon) = match section {
                ChannelSection::Overview => ("Overview", &self.icons.edit),
                ChannelSection::Permissions => ("Permissions", &self.icons.roles),
            };
            rows.push(SettingsRow {
                heading: false,
                text: text.into(),
                icon: icon.clone(),
                id: section_id(section),
                danger: false,
            });
        }
        // Delete Channel last, after a line, as Discord's sidebar has it.
        if app.may_delete_channel(room) {
            rows.push(SettingsRow {
                heading: true,
                ..Default::default()
            });
            rows.push(SettingsRow {
                text: if screen.kind == ChannelKind::Category {
                    tr("Delete Category")
                } else {
                    tr("Delete Channel")
                }
                .into(),
                icon: self.icons.delete.clone(),
                id: DELETE_CHANNEL,
                danger: true,
                ..Default::default()
            });
        }
        let loading = screen.section == ChannelSection::Overview
            && screen.overview.is_none()
            && screen.error.is_none();
        let mut prompt = ChannelSettingsPrompt {
            rows: ModelRc::new(VecModel::from(rows)),
            section: section_id(screen.section),
            loading,
            loaded: !loading,
            saving: screen.saving,
            error: screen.error.as_deref().unwrap_or_default().into(),
            problem: screen.problem().unwrap_or_default().into(),
            changed: screen.changed(),
            may_save: screen.ready() && app.state.discord_online,
            nudge: i32::try_from(screen.nudge % 1_000_000).unwrap_or_default(),
            kind: match screen.kind {
                ChannelKind::Text => 0,
                ChannelKind::Voice => 1,
                ChannelKind::Category => 2,
            },
            ..Default::default()
        };
        if let Some((_, draft)) = &screen.overview {
            if window.get_channel_settings_name() != draft.name.as_str() {
                window.set_channel_settings_name(draft.name.as_str().into());
            }
            if window.get_channel_settings_topic() != draft.topic.as_str() {
                window.set_channel_settings_topic(draft.topic.as_str().into());
            }
            prompt.slowmode = SLOWMODES
                .iter()
                .position(|seconds| *seconds == draft.slowmode)
                .map_or(0, |index| i32::try_from(index).unwrap_or_default());
            prompt.nsfw = draft.nsfw;
        }
        if screen.section == ChannelSection::Permissions {
            self.permissions_prompt(&mut prompt);
        }
        // Lists already shown stay, so their rows keep a click under way.
        let old = window.get_channel_settings_prompt();
        // While Discord is away its sections show none; the sidebar waits.
        prompt.rows = if app.state.discord_online {
            same_list(&old.rows, prompt.rows)
        } else {
            old.rows.clone()
        };
        prompt.overwrites = same_list(&old.overwrites, prompt.overwrites);
        prompt.permissions = same_list(&old.permissions, prompt.permissions);
        prompt.roles_to_add = same_list(&old.roles_to_add, prompt.roles_to_add);
        prompt.members_found = same_list(&old.members_found, prompt.members_found);
        window.set_channel_settings_prompt(prompt);
    }

    /// Permissions' part of the prompt.
    fn permissions_prompt(&self, prompt: &mut ChannelSettingsPrompt) {
        let app = &self.app;
        let Some(screen) = app.state.channel_settings.as_ref() else {
            return;
        };
        let room = &screen.room;
        let page = &screen.permissions;
        let rules = app.overwrite_rules(room);
        let category = screen.kind == ChannelKind::Category;
        let voice = screen.kind == ChannelKind::Voice;
        prompt.title = if category {
            tr("Category Settings")
        } else {
            tr("Channel Permissions")
        }
        .into();
        prompt.subtitle = if category {
            tr("Use permissions to customize who can do what in this category.")
        } else {
            tr("Use permissions to customize who can do what in this channel.")
        }
        .into();
        if !category && let Some((_, name)) = app.sync_category(room) {
            prompt.sync = if app.synced(room) { 1 } else { 2 };
            prompt.category = name.into();
        }
        let access = if voice {
            (1 << 10) | (1 << 20)
        } else {
            1 << 10
        };
        prompt.private = page.private(access);
        prompt.private_title = if category {
            tr("Private Category")
        } else {
            tr("Private Channel")
        }
        .into();
        prompt.private_text = if category {
            tr("By making a category private, only select members and roles will be able to view this category. Linked channels in this category will automatically match to this setting.")
        } else if voice {
            tr("By making a channel private, only select members and roles will have access to view or connect to this channel.")
        } else {
            tr("By making a channel private, only select members and roles will be able to view this channel.")
        }
        .into();
        // Locked as the core decides: any of the access bits.
        if (0..64).any(|bit| access & (1 << bit) != 0 && rules.locked(bit)) {
            prompt.private_lock = NOT_HELD.into();
        }
        prompt.may_edit = app
            .channel_sections(room)
            .contains(&ChannelSection::Permissions);
        let roles = app.server_roles(room);
        let color = |id: &str| {
            roles
                .iter()
                .find(|role| role.id == id)
                .and_then(|role| role.color)
                .map_or(role_color(0), role_color)
        };
        let listed = app.listed_overwrites();
        prompt.overwrites = ModelRc::new(VecModel::from(
            listed
                .iter()
                .map(|overwrite| OverwriteRow {
                    id: overwrite.id.as_str().into(),
                    name: app.overwrite_name(room, overwrite).into(),
                    member: overwrite.member,
                    color: if page.is_everyone(overwrite) {
                        role_color(0)
                    } else {
                        color(&overwrite.id)
                    },
                    selected: overwrite.id == page.selected,
                })
                .collect::<Vec<_>>(),
        ));
        let shown = page.shown();
        let everyone = page.is_everyone(&shown);
        prompt.shown_everyone = everyone;
        prompt.shown_name = app.overwrite_name(room, &shown).into();
        let mut permissions = vec![];
        for (heading, bits) in channel_groups(screen.kind) {
            permissions.push(ThreeStateRow {
                heading: true,
                title: heading.to_uppercase().into(),
                ..Default::default()
            });
            for &bit in *bits {
                let (title, description) = channel_permission(bit, screen.kind, everyone);
                permissions.push(ThreeStateRow {
                    heading: false,
                    title: title.into(),
                    description: description.into(),
                    bit: i32::try_from(bit).unwrap_or_default(),
                    state: state_id(Overwrites::state(&shown, bit)),
                    lock_text: if rules.locked(bit) {
                        NOT_HELD.into()
                    } else {
                        SharedString::default()
                    },
                });
            }
        }
        prompt.permissions = ModelRc::new(VecModel::from(permissions));
        if let Some(adding) = &page.adding {
            let query = adding.query.trim().to_lowercase();
            let free = |id: &str| !listed.iter().any(|known| known.id == id);
            prompt.adding = true;
            prompt.roles_to_add = ModelRc::new(VecModel::from(
                roles
                    .iter()
                    .filter(|role| free(&role.id))
                    .filter(|role| query.is_empty() || role.name.to_lowercase().contains(&query))
                    .map(|role| OverwriteRow {
                        id: role.id.as_str().into(),
                        name: role.name.as_str().into(),
                        member: false,
                        color: role.color.map_or(role_color(0), role_color),
                        selected: false,
                    })
                    .collect::<Vec<_>>(),
            ));
            prompt.members_found = ModelRc::new(VecModel::from(
                adding
                    .found
                    .iter()
                    .filter(|member| free(&member.id))
                    .map(|member| member_row(member, false))
                    .collect::<Vec<_>>(),
            ));
        }
        let name_of = |id: &str| {
            let overwrite = listed
                .iter()
                .find(|known| known.id == id)
                .cloned()
                .unwrap_or_else(|| ChannelOverwrite {
                    id: id.to_owned(),
                    ..Default::default()
                });
            app.overwrite_name(room, &overwrite)
        };
        match &page.confirm {
            Some(OverwriteConfirm::Remove(id)) => {
                prompt.confirm = 1;
                prompt.confirm_name = name_of(id).into();
            }
            Some(OverwriteConfirm::Sync) => prompt.confirm = 2,
            Some(OverwriteConfirm::DenySelf(name)) => {
                prompt.confirm = 3;
                prompt.confirm_name = name.as_str().into();
            }
            None => {}
        }
    }
}

/// What the screen sent, as core takes it; see ChannelSettingsScreen's
/// `input`.
fn channel_input(
    app: &Application,
    what: i32,
    text: SharedString,
    number: i32,
) -> Option<ChannelInput> {
    let text = || text.to_string();
    Some(match what {
        0 => ChannelInput::Name(text()),
        1 => ChannelInput::Topic(text()),
        2 => ChannelInput::FindMembers(text()),
        10 if number == DELETE_CHANNEL => ChannelInput::Delete,
        10 => ChannelInput::Section(match number {
            0 => ChannelSection::Overview,
            _ => ChannelSection::Permissions,
        }),
        11 => ChannelInput::Slowmode(*SLOWMODES.get(usize::try_from(number).ok()?)?),
        12 => ChannelInput::Nsfw(number != 0),
        13 => ChannelInput::Select(text()),
        14 => ChannelInput::Set(
            u32::try_from(number / 3).ok()?,
            match number % 3 {
                0 => OverwriteState::Deny,
                1 => OverwriteState::Neutral,
                _ => OverwriteState::Allow,
            },
        ),
        15 => ChannelInput::Private(number != 0),
        20 => ChannelInput::Adding(true),
        21 => ChannelInput::Adding(false),
        22 => ChannelInput::Add(text(), None),
        23 => {
            let id = text();
            let member = app
                .state
                .channel_settings
                .as_ref()?
                .permissions
                .adding
                .as_ref()?
                .found
                .iter()
                .find(|known| known.id == id)?
                .clone();
            ChannelInput::Add(id, Some(member))
        }
        24 => ChannelInput::Remove,
        25 => ChannelInput::Sync,
        26 => ChannelInput::Confirm,
        27 => ChannelInput::Cancel,
        30 => ChannelInput::Undo,
        31 => ChannelInput::Save,
        32 => ChannelInput::Close,
        _ => return None,
    })
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_channel_settings_input(move |what, text, number| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let input = channel_input(&state.borrow().app, what, text, number);
        if let Some(input) = input {
            state
                .borrow_mut()
                .action(&window, Action::ChannelSettings(input));
        }
    });
}
