//! A channel's settings' and the channel dialog's intents (`channel_input`,
//! `src/ui/channel_settings.rs:310`; `on_save_channel`,
//! `src/ui/callbacks.rs:1584`).

use iced::Task;
use yapfast_core::application::{Action, ChannelInput, ChannelSection, OverwriteState, SLOWMODES};
use yapfast_core::chat::{ChannelKind, ChannelSettings};

use anomi_vm::settings::channel::{self, Page};
use anomi_vm::settings::create_channel::{self, ChannelType};
use anomi_vm::settings::permissions;

use crate::app::App;
use crate::message::Message;

pub(super) fn overview(app: &mut App, intent: channel::Intent) -> Task<Message> {
    use channel::Intent;
    let input = match intent {
        Intent::Choose(Page::Overview) => ChannelInput::Section(ChannelSection::Overview),
        Intent::Choose(Page::Permissions) => ChannelInput::Section(ChannelSection::Permissions),
        Intent::Choose(Page::Delete) => ChannelInput::Delete,
        Intent::Close => ChannelInput::Close,
        Intent::Name(text) => ChannelInput::Name(text),
        Intent::Topic(text) => ChannelInput::Topic(text),
        Intent::Slowmode(index) => match SLOWMODES.get(index) {
            Some(seconds) => ChannelInput::Slowmode(*seconds),
            None => return Task::none(),
        },
        Intent::Nsfw(on) => ChannelInput::Nsfw(on),
        Intent::Undo => ChannelInput::Undo,
        Intent::Save => ChannelInput::Save,
        intent => {
            tracing::debug!(?intent, "not handled by the channel's settings");
            return Task::none();
        }
    };
    app.dispatch(Action::ChannelSettings(input))
}

pub(super) fn permissions(app: &mut App, intent: permissions::Intent) -> Task<Message> {
    use permissions::Intent;
    let input = match intent {
        Intent::Show(id) => ChannelInput::Select(id),
        Intent::Set(bit, state) => ChannelInput::Set(
            bit,
            match state {
                0 => OverwriteState::Deny,
                1 => OverwriteState::Neutral,
                _ => OverwriteState::Allow,
            },
        ),
        Intent::Private(on) => ChannelInput::Private(on),
        Intent::Add => ChannelInput::Adding(true),
        Intent::Search(text) => ChannelInput::FindMembers(text),
        Intent::CloseAdd => ChannelInput::Adding(false),
        Intent::AddRole(id) => ChannelInput::Add(id, None),
        // A member is added with the name the search found them by.
        Intent::AddMember(id) => {
            let found = app
                .core
                .state
                .channel_settings
                .as_ref()
                .and_then(|screen| screen.permissions.adding.as_ref())
                .and_then(|adding| adding.found.iter().find(|known| known.id == id))
                .cloned();
            match found {
                Some(member) => ChannelInput::Add(id, Some(member)),
                None => return Task::none(),
            }
        }
        Intent::Remove => ChannelInput::Remove,
        Intent::SyncNow => ChannelInput::Sync,
        Intent::Confirm => ChannelInput::Confirm,
        Intent::Cancel => ChannelInput::Cancel,
        intent => {
            tracing::debug!(?intent, "not handled by the channel's permissions");
            return Task::none();
        }
    };
    app.dispatch(Action::ChannelSettings(input))
}

/// The channel dialog: its type and name are the window's until it's
/// carried out.
pub(super) fn create(app: &mut App, intent: create_channel::Intent) -> Task<Message> {
    use create_channel::Intent;
    let form = &mut app.vm.settings.create_channel;
    match intent {
        Intent::Kind(kind) => {
            form.kind = kind;
            Task::none()
        }
        Intent::Name(name) => {
            form.name = name;
            Task::none()
        }
        Intent::Confirm => {
            if form.saving {
                return Task::none();
            }
            let kind = match form.kind {
                ChannelType::Voice => ChannelKind::Voice,
                ChannelType::Text => ChannelKind::Text,
            };
            let settings = ChannelSettings {
                name: form.name.clone(),
                ..ChannelSettings::default()
            };
            app.dispatch(Action::SaveChannel { kind, settings })
        }
        Intent::Close => app.dispatch(Action::CloseChannelDialog),
        intent => {
            tracing::debug!(?intent, "not handled by the channel dialog");
            Task::none()
        }
    }
}
