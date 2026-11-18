//! The dialog that makes, copies or deletes a Discord channel or category,
//! from the core's (`show_channel_dialog`, `src/ui/refresh.rs:2939`):
//! `Layer::CreateChannel` is open while the core holds one. Its name and
//! type are the window's, set from the channel's as it opens, when its
//! field takes the keys.

use yapfast_core::application::{Application, ChannelAsk};
use yapfast_core::chat::ChannelKind;
use yapfast_lang::tr;

use anomi_vm::Vm;
use anomi_vm::settings::create_channel::ChannelType;
use anomi_vm::shell::Layer;

use super::set;

/// `opened`: the dialog showed last time, so its fields are the window's.
pub(super) fn show(
    core: &Application,
    vm: &mut Vm,
    opened: &mut bool,
    focus: &mut Option<&'static str>,
) {
    let Some(dialog) = core.state.channel_dialog.as_ref() else {
        if vm.shell.is_open(Layer::CreateChannel) {
            vm.shell.close(Layer::CreateChannel);
        }
        *opened = false;
        return;
    };
    let form = &mut vm.settings.create_channel;
    let category = dialog.kind == ChannelKind::Category;
    let deleting = matches!(dialog.ask, ChannelAsk::Delete(_));
    // Its fields start from the channel's, as it opens.
    if !*opened || !vm.shell.is_open(Layer::CreateChannel) {
        *opened = true;
        form.name = dialog
            .settings
            .as_ref()
            .map(|settings| settings.name.clone())
            .unwrap_or_default();
        form.kind = if dialog.kind == ChannelKind::Voice {
            ChannelType::Voice
        } else {
            ChannelType::Text
        };
        vm.shell.open(Layer::CreateChannel);
        if !deleting {
            *focus = Some(anomi_settings::channel::create::NAME);
        }
    }
    let named = |name: &str| {
        if dialog.kind == ChannelKind::Text {
            format!("#{name}")
        } else {
            name.to_owned()
        }
    };
    let make = if category {
        tr!("Create category")
    } else {
        tr!("Create channel")
    };
    let (title, detail, confirm) = match &dialog.ask {
        ChannelAsk::Create { .. } => (
            make,
            tr!(
                "In {}",
                if dialog.name.is_empty() {
                    &dialog.server_name
                } else {
                    &dialog.name
                }
            ),
            make,
        ),
        ChannelAsk::Duplicate(_) => (
            tr!("Duplicate channel"),
            tr!(
                "A copy of {}, with its settings and permissions, in the same category.",
                named(&dialog.name)
            ),
            tr!("Create channel"),
        ),
        ChannelAsk::Delete(_) => {
            let delete = if category {
                tr!("Delete category")
            } else {
                tr!("Delete channel")
            };
            (
                delete,
                tr!(
                    "Are you sure you want to delete {}? This cannot be undone.",
                    named(&dialog.name)
                ),
                delete,
            )
        }
    };
    set(&mut form.title, title);
    set(&mut form.detail, &detail);
    set(&mut form.confirm, confirm);
    form.danger = deleting;
    let kinds = matches!(dialog.ask, ChannelAsk::Create { .. }) && !category;
    if kinds != !form.kinds.is_empty() {
        form.kinds = if kinds {
            vec![tr!("Text").to_owned(), tr!("Voice").to_owned()]
        } else {
            Vec::new()
        };
    }
    form.named = !deleting;
    set(
        &mut form.field,
        if category {
            tr!("CATEGORY NAME")
        } else {
            tr!("CHANNEL NAME")
        },
    );
    set(
        &mut form.placeholder,
        if category {
            tr!("New Category")
        } else {
            tr!("new-channel")
        },
    );
    form.saving = dialog.saving;
    set(&mut form.error, dialog.error.as_deref().unwrap_or_default());
}
