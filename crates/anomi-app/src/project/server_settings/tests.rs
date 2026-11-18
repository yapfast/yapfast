//! A server's and a channel's settings on an app without a window, on a
//! Discord server this account owns ("Test": the categories Talk and
//! Later, #general, the roles Mods and @everyone).

use std::sync::Arc;

use yapfast_core::application::{Action, RoleInput, RoleTab};
use yapfast_core::chat::{
    ChannelKind, ChannelSettings, Event, RoleDetail, ServerProfile, ServerRoles,
    ServerSettingsPage, SettingsSection,
};

use anomi_vm::settings::role_editor::{Confirm, Tab};
use anomi_vm::settings::server::Section;
use anomi_vm::shell::{Intent as Shell, Layer};

use crate::app::App;

/// An app whose account owns the server "Test", which shows.
pub(crate) fn app(name: &str) -> (App, std::path::PathBuf) {
    use yapfast_core::yapfast_discord::marked::MarkedServer;
    use yapfast_core::yapfast_discord::ready;
    use yapfast_core::yapfast_discord::service::{Snapshot, Status};

    let folder =
        std::env::temp_dir().join(format!("anomi-ssettings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    let server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "1", "properties": {"name": "Test"},
        "roles": [
            {"id": "20", "name": "Mods", "position": 1, "color": 0x3498db, "permissions": "0"},
            {"id": "10", "name": "@everyone", "position": 0, "permissions": "1024"},
        ],
        "channels": [
            {"id": "4", "type": 4, "name": "Talk", "position": 0},
            {"id": "5", "type": 4, "name": "Later", "position": 1},
            {"id": "11", "type": 0, "name": "general", "position": 0, "parent_id": "4"},
        ],
    }))
    .expect("the server");
    drop(app.core.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer {
            server: Arc::new(server),
            marks: Arc::default(),
        }],
        ..Default::default()
    }))));
    app.core.state.account = Some("1".into());
    app.refresh();
    (app, folder)
}

fn loaded(app: &mut App, section: SettingsSection, page: ServerSettingsPage) {
    drop(app.core.apply(Event::ServerSettingsLoaded {
        server: "discord:10".into(),
        section,
        result: Ok(page),
    }));
    app.refresh();
}

fn roles() -> ServerRoles {
    let role = |id: &str, name: &str, position| RoleDetail {
        id: id.into(),
        name: name.into(),
        position,
        members: 2,
        ..RoleDetail::default()
    };
    ServerRoles {
        roles: vec![role("20", "Mods", 1), role("10", "@everyone", 0)],
        ..ServerRoles::default()
    }
}

#[cfg(feature = "discord")]
#[test]
fn the_profile_fills_as_it_comes_and_the_bar_holds_until_reset() {
    let (mut app, folder) = app("profile");
    drop(app.dispatch(Action::OpenServerSettings("discord:10".into())));
    assert!(app.vm.shell.is_open(Layer::ServerSettings));
    let server = &app.vm.settings.server;
    assert_eq!(server.name, "Test");
    assert!(server.may_delete, "its owner may delete it");
    assert_eq!(server.sections.first(), Some(&Section::Profile));
    assert!(server.loading && !server.loaded);
    let profile = ServerProfile {
        name: "Test".into(),
        description: String::new(),
        icon: None,
    };
    loaded(
        &mut app,
        SettingsSection::Profile,
        ServerSettingsPage::Profile(profile),
    );
    let server = &app.vm.settings.server;
    assert!(server.loaded && server.unsaved.is_none());
    assert_eq!(server.profile.name, "Test");
    assert_eq!(server.profile.icon.initial, "T");
    drop(crate::intent::server_settings::handle(
        &mut app,
        anomi_vm::settings::server::Intent::Name("Renamed".into()).into(),
    ));
    let server = &app.vm.settings.server;
    assert_eq!(server.profile.name, "Renamed");
    let bar = server.unsaved.as_ref().expect("the bar for changes");
    assert!(bar.may_save && bar.nudge == 0);
    // Leaving is refused: the bar shakes, and the settings stay.
    drop(app.dispatch(Action::ServerSettingsSection(SettingsSection::Roles)));
    drop(app.dispatch(Action::CloseServerSettings));
    assert_eq!(
        app.vm.settings.server.unsaved.as_ref().map(|bar| bar.nudge),
        Some(2)
    );
    assert_eq!(app.vm.settings.server.section, Section::Profile);
    drop(app.dispatch(Action::UndoServerSettings));
    drop(app.dispatch(Action::CloseServerSettings));
    assert!(!app.vm.shell.is_open(Layer::ServerSettings));
    assert_eq!(
        app.vm.settings.server.name, "Test",
        "it plays out as it was"
    );
    drop(crate::intent::handle(
        &mut app,
        Shell::Gone(Layer::ServerSettings).into(),
    ));
    assert!(
        app.vm.settings.server.name.is_empty(),
        "nothing held once gone"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[cfg(feature = "discord")]
#[test]
fn a_role_opens_in_its_editor_and_its_delete_asks_over_it() {
    let (mut app, folder) = app("roles");
    drop(app.dispatch(Action::OpenServerSettings("discord:10".into())));
    drop(app.dispatch(Action::ServerSettingsSection(SettingsSection::Roles)));
    loaded(
        &mut app,
        SettingsSection::Roles,
        ServerSettingsPage::Roles(roles()),
    );
    let names: Vec<&str> = app
        .vm
        .settings
        .roles
        .rows
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["Mods"], "@everyone stands apart");
    assert_eq!(app.vm.settings.roles.everyone, "10");
    assert_eq!(app.vm.settings.server.title, None);
    drop(app.dispatch(Action::Roles(RoleInput::Open(Some("20".into())))));
    let (server, editor) = (&app.vm.settings.server, &app.vm.settings.role_editor);
    assert!(
        server
            .title
            .as_deref()
            .is_some_and(|title| title.contains("Mods"))
    );
    assert_eq!((editor.name.as_str(), editor.tab), ("Mods", Tab::Display));
    assert!(editor.default_color && !editor.locked);
    assert_eq!(editor.hex, "#99aab5");
    // A colour chosen previews at once, and the hex field follows it.
    drop(app.dispatch(Action::Roles(RoleInput::Color(0x3498db))));
    let editor = &app.vm.settings.role_editor;
    assert_eq!(editor.hex, "#3498db");
    assert!(editor.bright[2].chosen && !editor.custom_color);
    assert!(app.vm.settings.server.unsaved.is_some());
    drop(app.dispatch(Action::Roles(RoleInput::Tab(RoleTab::Permissions))));
    assert!(
        app.vm.settings.server.fill,
        "a role's permissions take the height"
    );
    assert!(
        app.vm
            .settings
            .role_editor
            .permissions
            .iter()
            .any(|line| line.heading)
    );
    drop(app.dispatch(Action::UndoServerSettings));
    drop(app.dispatch(Action::Roles(RoleInput::Delete("20".into()))));
    assert!(app.vm.shell.is_open(Layer::RoleConfirm));
    let asked = Confirm::Delete {
        role: "Mods".into(),
    };
    assert_eq!(app.vm.settings.role_editor.confirm, Some(asked.clone()));
    drop(crate::intent::handle(
        &mut app,
        Shell::Close(Layer::RoleConfirm).into(),
    ));
    assert!(!app.vm.shell.is_open(Layer::RoleConfirm));
    assert_eq!(
        app.vm.settings.role_editor.confirm,
        Some(asked),
        "it plays out"
    );
    drop(crate::intent::handle(
        &mut app,
        Shell::Gone(Layer::RoleConfirm).into(),
    ));
    assert_eq!(app.vm.settings.role_editor.confirm, None);
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[cfg(feature = "discord")]
#[test]
fn a_channels_settings_and_the_channel_dialog_open_from_the_core() {
    let (mut app, folder) = app("channel");
    drop(app.dispatch(Action::OpenChannelSettings("discord:10:11".into())));
    assert!(app.vm.shell.is_open(Layer::ChannelSettings));
    assert!(app.vm.settings.channel.loading);
    drop(app.core.apply(Event::ChannelLoaded {
        room: "discord:10:11".into(),
        result: Ok(ChannelSettings {
            name: "general".into(),
            topic: "Hi".into(),
            ..ChannelSettings::default()
        }),
    }));
    app.refresh();
    let channel = &app.vm.settings.channel;
    assert_eq!(
        (channel.name.as_str(), channel.topic.as_str()),
        ("general", "Hi")
    );
    assert!(!channel.loading && channel.unsaved.is_none());
    assert!(
        matches!(&channel.rows[0], anomi_vm::settings::channel::Row::Heading(name) if name == "#GENERAL")
    );
    drop(app.dispatch(Action::ChannelSettings(
        yapfast_core::application::ChannelInput::Close,
    )));
    assert!(!app.vm.shell.is_open(Layer::ChannelSettings));
    drop(app.dispatch(Action::NewChannel {
        place: "discord:10".into(),
        kind: ChannelKind::Text,
    }));
    assert!(app.vm.shell.is_open(Layer::CreateChannel));
    let form = &app.vm.settings.create_channel;
    assert_eq!(form.kinds.len(), 2, "a channel offers Text and Voice");
    assert!(form.named && !form.danger);
    assert_eq!(app.shown.focus, Some(anomi_settings::channel::create::NAME));
    drop(app.dispatch(Action::CloseChannelDialog));
    assert!(!app.vm.shell.is_open(Layer::CreateChannel));
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[cfg(feature = "discord")]
#[test]
fn delete_server_asks_its_name_over_the_settings_and_stays_while_sent() {
    use anomi_vm::settings::ownership::Kind;
    use yapfast_core::application::OwnershipInput;

    let (mut app, folder) = app("delete");
    drop(app.dispatch(Action::OpenServerSettings("discord:10".into())));
    drop(crate::intent::server_settings::handle(
        &mut app,
        anomi_vm::settings::server::Intent::Delete.into(),
    ));
    assert!(app.vm.shell.is_open(Layer::Ownership));
    let asked = &app.vm.settings.ownership;
    assert_eq!(asked.kind, Kind::Delete);
    assert!(asked.title.contains("Test"));
    if asked.by_name {
        assert_eq!(
            app.shown.focus,
            Some(anomi_settings::server::ownership::NAME)
        );
    }
    drop(app.dispatch(Action::Ownership(OwnershipInput::Type("Test".into()))));
    assert_eq!(app.vm.settings.ownership.typed, "Test");
    drop(app.dispatch(Action::Ownership(OwnershipInput::Confirm)));
    assert!(app.vm.settings.ownership.sending);
    // Escape asks the core, which keeps the dialog while Discord deletes.
    drop(crate::intent::handle(
        &mut app,
        Shell::Close(Layer::Ownership).into(),
    ));
    assert!(app.vm.shell.is_open(Layer::Ownership));
    drop(crate::intent::handle(
        &mut app,
        Shell::Close(Layer::ServerSettings).into(),
    ));
    assert!(
        app.vm.shell.is_open(Layer::ServerSettings),
        "not while it's deleted"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
