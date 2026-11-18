//! A channel's settings from the core's (`show_channel_settings` and
//! `permissions_prompt`, `src/ui/channel_settings.rs:41`, `:146`):
//! `Layer::ChannelSettings` open while the core holds them, Overview's
//! draft and the Permissions page in `vm.settings.channel`, with the
//! question and Add's picker over it. Its sections stay while Discord is
//! away, so Overview, Permissions and Delete still show.

use yapfast_core::application::{
    Application, ChannelSection, ChannelSettingsScreen, OverwriteConfirm, OverwriteState,
    Overwrites, SLOWMODES, channel_groups, channel_permission,
};
use yapfast_core::chat::{ChannelKind, ChannelOverwrite};
use yapfast_lang::tr;

use anomi_vm::Vm;
use anomi_vm::icon::Icon;
use anomi_vm::settings::channel::{Channel, Kind, Page, Row};
use anomi_vm::settings::frame::Unsaved;
use anomi_vm::settings::permissions::{Adding, Confirm, Found, Line, Overwrite, Permissions, Sync};
use anomi_vm::shell::{Layer, Shell};

use super::{dialog, set};
use crate::project::rows;

const NOT_HELD: &str = "You cannot modify this permission because none of your roles have it.";

/// What the channel's settings showed last.
#[derive(Default)]
pub(crate) struct Shown {
    room: Option<String>,
}

/// The core's section as the window names it.
fn page(section: ChannelSection) -> Page {
    match section {
        ChannelSection::Overview => Page::Overview,
        ChannelSection::Permissions => Page::Permissions,
    }
}

pub(super) fn show(
    core: &Application,
    vm: &mut Vm,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    let Some(screen) = core.state.channel_settings.as_ref() else {
        if vm.shell.is_open(Layer::ChannelSettings) {
            for layer in [
                Layer::ChannelSettings,
                Layer::PermissionsConfirm,
                Layer::PermissionsAdd,
            ] {
                vm.shell.close(layer);
            }
        }
        shown.room = None;
        return;
    };
    if !vm.shell.is_open(Layer::ChannelSettings) {
        vm.shell.open(Layer::ChannelSettings);
    }
    let fresh = shown.room.as_deref() != Some(screen.room.as_str());
    if fresh {
        shown.room = Some(screen.room.clone());
    }
    let channel = &mut vm.settings.channel;
    frame(core, screen, channel, fresh);
    if screen.section == ChannelSection::Permissions {
        permissions(core, screen, &mut channel.permissions, &mut vm.shell, focus);
    } else {
        dialog(
            &mut vm.shell,
            Layer::PermissionsConfirm,
            &mut channel.permissions.confirm,
            None,
        );
        dialog(
            &mut vm.shell,
            Layer::PermissionsAdd,
            &mut channel.permissions.adding,
            None,
        );
    }
}

/// The sections, the title, Overview's draft and the bar.
fn frame(core: &Application, screen: &ChannelSettingsScreen, vm: &mut Channel, fresh: bool) {
    let room = &screen.room;
    let title = core.channel_title(room);
    let named = if screen.kind == ChannelKind::Text {
        format!("#{title}")
    } else {
        title
    };
    if core.state.discord_online || vm.rows.is_empty() || fresh {
        let mut rows = vec![Row::Heading(named.to_uppercase())];
        rows.extend(core.channel_sections(room).into_iter().map(|section| {
            let (text, icon) = match section {
                ChannelSection::Overview => (tr!("Overview"), Icon::Pencil),
                ChannelSection::Permissions => (tr!("Permissions"), Icon::Shield),
            };
            Row::Page {
                text: text.to_owned(),
                icon,
                page: page(section),
                danger: false,
            }
        }));
        if core.may_delete_channel(room) {
            rows.push(Row::Line);
            rows.push(Row::Page {
                text: if screen.kind == ChannelKind::Category {
                    tr!("Delete Category")
                } else {
                    tr!("Delete Channel")
                }
                .to_owned(),
                icon: Icon::Trash,
                page: Page::Delete,
                danger: true,
            });
        }
        if vm.rows != rows {
            vm.rows = rows;
        }
    }
    vm.page = page(screen.section);
    vm.kind = match screen.kind {
        ChannelKind::Text => Kind::Text,
        ChannelKind::Voice => Kind::Voice,
        ChannelKind::Category => Kind::Category,
    };
    let title = match screen.section {
        ChannelSection::Overview => tr!("Overview"),
        ChannelSection::Permissions if screen.kind == ChannelKind::Category => {
            tr!("Category Settings")
        }
        ChannelSection::Permissions => tr!("Channel Permissions"),
    };
    set(&mut vm.title, title);
    if vm.close.is_empty() {
        vm.close = tr!("Close {}", tr!("Channel settings"));
    }
    vm.loading = screen.section == ChannelSection::Overview
        && screen.overview.is_none()
        && screen.error.is_none();
    set(&mut vm.error, screen.error.as_deref().unwrap_or_default());
    set(
        &mut vm.problem,
        screen.problem().as_deref().unwrap_or_default(),
    );
    vm.saving = screen.saving;
    vm.unsaved = (screen.changed() || screen.saving).then(|| Unsaved {
        saving: screen.saving,
        may_reset: !screen.saving,
        may_save: screen.ready() && core.state.discord_online,
        nudge: screen.nudge,
    });
    if let Some((_, draft)) = &screen.overview {
        set(&mut vm.name, &draft.name);
        set(&mut vm.topic, &draft.topic);
        vm.slowmode = SLOWMODES
            .iter()
            .position(|seconds| *seconds == draft.slowmode)
            .unwrap_or(0);
        vm.nsfw = draft.nsfw;
    }
}

/// The Permissions page: the overwrites, the shown one's permissions, the
/// private switch, and the question or Add's picker over it.
fn permissions(
    core: &Application,
    screen: &ChannelSettingsScreen,
    vm: &mut Permissions,
    shell: &mut Shell,
    focus: &mut Option<&'static str>,
) {
    let room = &screen.room;
    let page = &screen.permissions;
    let rules = core.overwrite_rules(room);
    let category = screen.kind == ChannelKind::Category;
    let voice = screen.kind == ChannelKind::Voice;
    set(
        &mut vm.subtitle,
        if category {
            tr!("Use permissions to customize who can do what in this category.")
        } else {
            tr!("Use permissions to customize who can do what in this channel.")
        },
    );
    (vm.sync, vm.category) = match core.sync_category(room).filter(|_| !category) {
        Some((_, name)) if core.synced(room) => (Sync::Synced, name),
        Some((_, name)) => (Sync::Apart, name),
        None => (Sync::None, String::new()),
    };
    set(&mut vm.channel, &core.channel_title(room));
    let access = if voice {
        (1 << 10) | (1 << 20)
    } else {
        1 << 10
    };
    vm.private = page.private(access);
    set(
        &mut vm.private_title,
        if category {
            tr!("Private Category")
        } else {
            tr!("Private Channel")
        },
    );
    set(
        &mut vm.private_text,
        if category {
            tr!(
                "By making a category private, only select members and roles will be able to view this category. Linked channels in this category will automatically match to this setting."
            )
        } else if voice {
            tr!(
                "By making a channel private, only select members and roles will have access to view or connect to this channel."
            )
        } else {
            tr!(
                "By making a channel private, only select members and roles will be able to view this channel."
            )
        },
    );
    let locked = (0..64).any(|bit| access & (1 << bit) != 0 && rules.locked(bit));
    set(
        &mut vm.private_lock,
        if locked {
            yapfast_lang::text(NOT_HELD)
        } else {
            ""
        },
    );
    vm.may_edit = core
        .channel_sections(room)
        .contains(&ChannelSection::Permissions);
    vm.saving = screen.saving;
    let roles = core.server_roles(room);
    let color = |id: &str| {
        roles
            .iter()
            .find(|role| role.id == id)
            .and_then(|role| role.color)
            .unwrap_or(0)
    };
    let listed = core.listed_overwrites();
    let overwrites: Vec<Overwrite> = listed
        .iter()
        .map(|overwrite| Overwrite {
            id: overwrite.id.clone(),
            name: core.overwrite_name(room, overwrite),
            member: overwrite.member,
            color: if page.is_everyone(overwrite) {
                0
            } else {
                color(&overwrite.id)
            },
            selected: overwrite.id == page.selected,
        })
        .collect();
    if vm.overwrites != overwrites {
        vm.overwrites = overwrites;
    }
    let shown = page.shown();
    let everyone = page.is_everyone(&shown);
    vm.shown_everyone = everyone;
    set(
        &mut vm.remove,
        &tr!("Remove {}", core.overwrite_name(room, &shown)),
    );
    let mut lines = vec![];
    for (heading, bits) in channel_groups(screen.kind) {
        lines.push(Line {
            heading: true,
            title: yapfast_lang::text(heading).to_uppercase(),
            ..Line::default()
        });
        for &bit in *bits {
            let (title, description) = channel_permission(bit, screen.kind, everyone);
            lines.push(Line {
                heading: false,
                title: title.to_owned(),
                description: description.to_owned(),
                bit,
                state: match Overwrites::state(&shown, bit) {
                    OverwriteState::Deny => 0,
                    OverwriteState::Neutral => 1,
                    OverwriteState::Allow => 2,
                },
                lock: if rules.locked(bit) {
                    yapfast_lang::text(NOT_HELD).to_owned()
                } else {
                    String::new()
                },
            });
        }
    }
    if vm.lines != lines {
        vm.lines = lines;
    }
    let state = &core.state;
    let adding = page.adding.as_ref().map(|adding| {
        let query = adding.query.trim().to_lowercase();
        let free = |id: &str| !listed.iter().any(|known| known.id == id);
        Adding {
            query: adding.query.clone(),
            roles: roles
                .iter()
                .filter(|role| free(&role.id))
                .filter(|role| query.is_empty() || role.name.to_lowercase().contains(&query))
                .map(|role| Overwrite {
                    id: role.id.clone(),
                    name: role.name.clone(),
                    member: false,
                    color: role.color.unwrap_or(0),
                    selected: false,
                })
                .collect(),
            members: adding
                .found
                .iter()
                .filter(|member| free(&member.id))
                .map(|member| Found {
                    id: member.id.clone(),
                    name: member.name.clone(),
                    username: member.username.clone().unwrap_or_default(),
                    avatar: rows::avatar(state, &member.name, &member.id, Some(&member.avatar)),
                })
                .collect(),
        }
    });
    if dialog(shell, Layer::PermissionsAdd, &mut vm.adding, adding) {
        *focus = Some(anomi_settings::channel::permissions::SEARCH);
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
        core.overwrite_name(room, &overwrite)
    };
    let confirm = match &page.confirm {
        Some(OverwriteConfirm::Remove(id)) => Some((Confirm::Remove, name_of(id))),
        Some(OverwriteConfirm::Sync) => Some((Confirm::Sync, String::new())),
        Some(OverwriteConfirm::DenySelf(name)) => Some((Confirm::Refused, name.clone())),
        None => None,
    };
    if let Some((_, name)) = &confirm {
        set(&mut vm.confirm_name, name);
    }
    dialog(
        shell,
        Layer::PermissionsConfirm,
        &mut vm.confirm,
        confirm.map(|(asked, _)| asked),
    );
}
