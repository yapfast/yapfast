//! Roles' list and the role editor's intents (`role_input`,
//! `src/ui/server_settings.rs:1571`). A role dragged to a new place moves
//! in the draft, which Save sends as one change with the rest; the hex and
//! emoji fields are the window's until they're used.

use iced::Task;
use yapfast_core::application::{
    Action, DEFAULT_ROLE_COLOR, ROLE_COLORS, RoleInput, RoleTab, parse_colour,
};
use yapfast_core::chat::ServerPicture;

use anomi_vm::settings::role_editor::{self, Tab};
use anomi_vm::settings::roles;

use super::pick;
use crate::app::App;
use crate::message::Message;

pub(super) fn list(app: &mut App, intent: roles::Intent) -> Task<Message> {
    use roles::Intent;
    let input = match intent {
        Intent::Search(text) => RoleInput::Search(text),
        Intent::Open(id) => RoleInput::Open(Some(id)),
        Intent::Create => RoleInput::Create,
        Intent::Copy(id) => RoleInput::Duplicate(id),
        Intent::Delete(id) => RoleInput::Delete(id),
        Intent::Move { id, to } => {
            app.vm.settings.roles.dragging = None;
            RoleInput::Move { role: id, to }
        }
        Intent::Dragging(id) => {
            app.vm.settings.roles.dragging = id;
            return Task::none();
        }
        Intent::More(id, at) => {
            let serial = 0;
            let roles = &app.vm.settings.roles;
            let menu = anomi_settings::server::roles::list::menu::menu(roles, &id, at, serial);
            crate::intent::menus::show(app, menu);
            return Task::none();
        }
        intent => {
            tracing::debug!(?intent, "not handled by Roles");
            return Task::none();
        }
    };
    app.dispatch(Action::Roles(input))
}

pub(super) fn editor(app: &mut App, intent: role_editor::Intent) -> Task<Message> {
    use role_editor::Intent;
    let editor = &mut app.vm.settings.role_editor;
    let input = match intent {
        Intent::Back => RoleInput::Open(None),
        Intent::Tab(tab) => RoleInput::Tab(match tab {
            Tab::Display => RoleTab::Display,
            Tab::Permissions => RoleTab::Permissions,
            Tab::Members => RoleTab::Members,
        }),
        Intent::Name(text) => RoleInput::Name(text),
        Intent::Color(None) => RoleInput::Color(DEFAULT_ROLE_COLOR),
        Intent::Color(Some(index)) => match ROLE_COLORS.get(index) {
            Some(color) => RoleInput::Color(*color),
            None => return Task::none(),
        },
        Intent::HexText(text) => {
            editor.hex = text;
            return Task::none();
        }
        // A colour written as hex, "#rrggbb" or without its "#"; what
        // isn't one stays in the field.
        Intent::Hex(text) => match parse_colour(&text) {
            Ok(Some(color)) => RoleInput::Color(color),
            _ => return Task::none(),
        },
        Intent::Hoist => RoleInput::Hoist,
        Intent::Mentionable => RoleInput::Mentionable,
        Intent::PickIcon => return pick(app, ServerPicture::RoleIcon),
        Intent::RemoveIcon => {
            return app.dispatch(Action::RemoveServerPicture(ServerPicture::RoleIcon));
        }
        Intent::EmojiText(text) => {
            editor.emoji_text = text;
            return Task::none();
        }
        Intent::Emoji(text) => {
            editor.emoji_text.clear();
            RoleInput::Emoji(text)
        }
        Intent::PermissionSearch(text) => RoleInput::PermissionSearch(text),
        Intent::ClearPermissions => RoleInput::ClearPermissions,
        Intent::Permission(bit) => match 1u64.checked_shl(bit) {
            Some(bit) => RoleInput::Permission(bit),
            None => return Task::none(),
        },
        Intent::MemberSearch(text) => RoleInput::MemberSearch(text),
        Intent::AddMembers => RoleInput::AddMembers,
        Intent::RemoveMember(id) => RoleInput::RemoveMember(id),
        Intent::Confirm => RoleInput::Confirm,
        Intent::Cancel => RoleInput::Cancel,
        Intent::AddingSearch(text) => RoleInput::FindCandidates(text),
        Intent::AddingPick(id) => RoleInput::Pick(id),
        Intent::AddingAdd => RoleInput::AddPicked,
        Intent::AddingClose => RoleInput::CloseAdding,
        intent => {
            tracing::debug!(?intent, "not handled by the role editor");
            return Task::none();
        }
    };
    app.dispatch(Action::Roles(input))
}
