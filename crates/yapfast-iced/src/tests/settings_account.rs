//! Account settings: the profile editors, and the lists of the apps the
//! account authorized, the devices it is signed in on and the services it
//! linked, with the username, email and password forms. Everything is done
//! in Yapfast, and a tab reads its list only when Discord is connected.

use iced_test::Simulator;
use yapfast_core::{
    account_tools::{AccountField, AuthorizedApp, Connection, Device},
    chat::{Command, DiscordCommand, Event as ChatEvent},
};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    settings::{
        Event, Section,
        account::{Event as AccountEvent, Tab},
    },
};

fn open(linked: bool) -> (App, tempfile::TempDir) {
    let (mut app, folder) = preview();
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    if linked {
        change(&mut app, |state| {
            state.discord_linked = true;
            state.discord_user = Some("42".into());
        });
    }
    let _ = app.update(Message::Settings(Event::Open(Section::Account)));
    app.client.sent.clear();
    (app, folder)
}

/// The same, with Discord connected, so a tab's list is asked for.
fn open_online() -> (App, tempfile::TempDir) {
    let (mut app, folder) = open(true);
    change(&mut app, |state| {
        state.account = Some("42".into());
        state.preview = false;
        state.discord_online = true;
    });
    app.client.sent.clear();
    (app, folder)
}

fn show(app: &mut App, tab: Tab) {
    let _ = app.update(Message::Settings(Event::AccountPage(AccountEvent::Show(
        tab,
    ))));
}

fn account(app: &mut App, event: AccountEvent) {
    let _ = app.update(Message::Settings(Event::AccountPage(event)));
}

#[test]
fn account_tabs_stay_in_the_account_section_and_read_nothing_in_preview() {
    let (mut app, _folder) = open(true);
    for (label, tab, heading) in [
        ("Profiles", Tab::Profiles, "Your profiles"),
        ("Connections", Tab::Connections, "Connected accounts"),
        ("Authorized Apps", Tab::AuthorizedApps, "App access"),
        ("Devices", Tab::Devices, "Signed-in devices"),
        ("Overview", Tab::Account, "My account"),
    ] {
        interact(&mut app, |ui| {
            ui.click(label).expect("the account tab");
        });
        let settings = app.win.settings.get().expect("settings stay open");
        assert_eq!(settings.section(), Section::Account);
        assert_eq!(settings.account.tab, tab);
        assert!(shows(&app, heading), "{heading}");
        assert!(
            app.client.sent.is_empty(),
            "the preview has no account to read"
        );
    }
}

#[test]
fn opening_a_tab_reads_its_list_when_discord_is_connected() {
    let (mut app, _folder) = open_online();
    for tab in [Tab::AuthorizedApps, Tab::Devices, Tab::Connections] {
        show(&mut app, tab);
        let read = match app.client.sent.as_slice() {
            [Command::Discord(DiscordCommand::LoadAuthorizedApps)] => Tab::AuthorizedApps,
            [Command::Discord(DiscordCommand::LoadDevices)] => Tab::Devices,
            [Command::Discord(DiscordCommand::LoadConnections)] => Tab::Connections,
            _ => panic!("{tab:?} read nothing, or more than its list"),
        };
        assert_eq!(read, tab);
        app.client.sent.clear();
    }
    show(&mut app, Tab::Profiles);
    show(&mut app, Tab::Account);
    assert!(app.client.sent.is_empty(), "those tabs list nothing");
}

#[test]
fn a_list_that_could_not_be_read_says_why_and_reads_again() {
    let (mut app, _folder) = open_online();
    show(&mut app, Tab::Devices);
    assert!(shows(&app, "Loading..."));
    app.client.apply(ChatEvent::DevicesLoaded {
        result: Err("Discord is busy".into()),
    });
    let _ = app.sync();
    assert!(shows(&app, "Discord is busy"));
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Try again").expect("the retry");
    });
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::LoadDevices)]
    ));
}

#[test]
fn authorized_apps_list_their_access_and_can_be_revoked() {
    let (mut app, _folder) = open_online();
    show(&mut app, Tab::AuthorizedApps);
    app.client.apply(ChatEvent::AuthorizedAppsLoaded {
        result: Ok(vec![AuthorizedApp {
            id: "a1".into(),
            name: "Cool Bot".into(),
            description: "Does things".into(),
            icon: None,
            scopes: vec!["identify".into(), "bot".into()],
        }]),
    });
    let _ = app.sync();
    assert!(shows(&app, "Cool Bot"));
    assert!(shows(&app, "Does things"));
    assert!(shows(
        &app,
        "Can: See your username and avatar, Add a bot to servers"
    ));
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Revoke").expect("the revoke button");
    });
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::RevokeAuthorizedApp { id })] if id == "a1"
    ));
    // A second press while it goes sends nothing.
    app.client.sent.clear();
    account(&mut app, AccountEvent::Revoke("a1".into()));
    assert!(app.client.sent.is_empty());
    app.client.apply(ChatEvent::AuthorizedAppRevoked {
        id: "a1".into(),
        result: Ok(()),
    });
    let _ = app.sync();
    assert!(!shows(&app, "Cool Bot"));
    assert!(shows(&app, "No apps are authorized."));
}

#[test]
fn a_refused_revoke_keeps_the_app_and_says_why() {
    let (mut app, _folder) = open_online();
    show(&mut app, Tab::AuthorizedApps);
    app.client.apply(ChatEvent::AuthorizedAppsLoaded {
        result: Ok(vec![AuthorizedApp {
            id: "a1".into(),
            name: "Cool Bot".into(),
            ..Default::default()
        }]),
    });
    account(&mut app, AccountEvent::Revoke("a1".into()));
    app.client.apply(ChatEvent::AuthorizedAppRevoked {
        id: "a1".into(),
        result: Err("Not allowed".into()),
    });
    let _ = app.sync();
    assert!(shows(&app, "Cool Bot"));
    assert!(shows(&app, "Not allowed"));
}

#[test]
fn devices_list_where_the_account_is_signed_in_and_sign_the_others_out() {
    let (mut app, _folder) = open_online();
    show(&mut app, Tab::Devices);
    app.client.apply(ChatEvent::DevicesLoaded {
        result: Ok(vec![
            Device {
                id: "h1".into(),
                os: "Linux".into(),
                platform: "Yapfast".into(),
                location: "Berlin".into(),
                current: true,
                ..Default::default()
            },
            Device {
                id: "h2".into(),
                os: "Windows".into(),
                platform: "Chrome".into(),
                ..Default::default()
            },
        ]),
    });
    let _ = app.sync();
    assert!(shows(&app, "Yapfast, Linux"));
    assert!(shows(&app, "This device - Berlin"));
    assert!(shows(&app, "Chrome, Windows"));
    assert!(
        !shows(&app, "Sign out all other devices"),
        "one other device needs no button for all"
    );
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Sign out").expect("the other device's button");
    });
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::LogOutDevices { ids })] if ids == &["h2".to_owned()]
    ));
    app.client.apply(ChatEvent::DevicesLoggedOut {
        ids: vec!["h2".into()],
        result: Ok(()),
    });
    let _ = app.sync();
    assert!(!shows(&app, "Chrome, Windows"));
    assert!(shows(&app, "Yapfast, Linux"));
}

#[test]
fn connections_list_their_visibility_and_can_be_changed_or_unlinked() {
    let (mut app, _folder) = open_online();
    show(&mut app, Tab::Connections);
    app.client.apply(ChatEvent::ConnectionsLoaded {
        result: Ok(vec![Connection {
            service: "github".into(),
            label: "GitHub".into(),
            id: "7".into(),
            name: "octo".into(),
            verified: true,
            visible: true,
            show_activity: false,
            revoked: false,
        }]),
    });
    let _ = app.sync();
    assert!(shows(&app, "GitHub"));
    assert!(shows(&app, "octo (verified)"));
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Display as activity status")
            .expect("the activity switch");
    });
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::ShowConnection {
            service,
            id,
            visible: None,
            activity: Some(true),
        })] if service == "github" && id == "7"
    ));
    app.client.apply(ChatEvent::ConnectionShown {
        service: "github".into(),
        id: "7".into(),
        visible: None,
        activity: Some(true),
        result: Ok(()),
    });
    let _ = app.sync();
    assert!(app.client.state().account_tools.connections.items[0].show_activity);
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Disconnect").expect("the disconnect button");
    });
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::DisconnectAccount { service, id })]
            if service == "github" && id == "7"
    ));
    app.client.apply(ChatEvent::AccountDisconnected {
        service: "github".into(),
        id: "7".into(),
        result: Ok(()),
    });
    let _ = app.sync();
    assert!(!shows(&app, "octo (verified)"));
    assert!(shows(&app, "No accounts are linked."));
}

#[test]
fn a_service_is_linked_through_its_own_sign_in_page() {
    let (mut app, _folder) = open_online();
    show(&mut app, Tab::Connections);
    app.client
        .apply(ChatEvent::ConnectionsLoaded { result: Ok(vec![]) });
    let _ = app.sync();
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Spotify").expect("a service");
    });
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::LinkConnection { service })] if service == "spotify"
    ));
}

#[test]
fn the_username_email_and_password_forms_need_what_discord_asks_for() {
    let (mut app, _folder) = open_online();
    account(&mut app, AccountEvent::Form(AccountField::Username));
    account(&mut app, AccountEvent::Value("newname".into()));
    account(&mut app, AccountEvent::Save);
    assert!(
        app.client.sent.is_empty(),
        "nothing goes without the current password"
    );
    account(&mut app, AccountEvent::Password("hunter2".into()));
    account(&mut app, AccountEvent::Save);
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::ChangeAccount { field: AccountField::Username, value, password })]
            if value == "newname" && password == "hunter2"
    ));
    app.client.apply(ChatEvent::AccountChanged {
        field: AccountField::Username,
        result: Ok(()),
    });
    let _ = app.sync();
    assert!(shows(&app, "Your username was changed."));
}

#[test]
fn a_new_password_is_typed_twice_and_a_refused_change_says_why() {
    let (mut app, _folder) = open_online();
    account(&mut app, AccountEvent::Form(AccountField::Password));
    account(&mut app, AccountEvent::Value("new secret".into()));
    account(&mut app, AccountEvent::Confirm("new secre".into()));
    account(&mut app, AccountEvent::Password("old secret".into()));
    account(&mut app, AccountEvent::Save);
    assert!(app.client.sent.is_empty(), "the two do not match");
    let _ = app.sync();
    assert!(shows(&app, "The new passwords are not the same."));
    account(&mut app, AccountEvent::Confirm("new secret".into()));
    account(&mut app, AccountEvent::Save);
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::ChangeAccount {
            field: AccountField::Password,
            ..
        })]
    ));
    app.client.apply(ChatEvent::AccountChanged {
        field: AccountField::Password,
        result: Err("Password does not match".into()),
    });
    let _ = app.sync();
    assert!(shows(&app, "Password does not match"));
    // The password is asked for again after a try.
    app.client.sent.clear();
    account(&mut app, AccountEvent::Save);
    assert!(app.client.sent.is_empty());
}

#[test]
fn the_forms_send_nothing_in_the_preview() {
    let (mut app, _folder) = open(true);
    account(&mut app, AccountEvent::Form(AccountField::Email));
    account(&mut app, AccountEvent::Value("a@b.c".into()));
    account(&mut app, AccountEvent::Password("pw".into()));
    account(&mut app, AccountEvent::Save);
    assert!(app.client.sent.is_empty());
}

#[test]
fn profiles_open_the_existing_main_profile_editor() {
    let (mut app, _folder) = open(true);
    interact(&mut app, |ui| {
        ui.click("Profiles").expect("Profiles");
    });
    interact(&mut app, |ui| {
        ui.click("Edit main profile").expect("the native editor");
    });
    assert!(app.client.state().profile_editor.is_some());
    assert!(app.win.settings.get().is_none());
    assert!(matches!(
        app.client.sent.as_slice(),
        [Command::Discord(DiscordCommand::OwnProfile {
            server: None
        })]
    ));
}

#[test]
fn profiles_without_a_discord_identity_offer_no_inert_editor() {
    let (mut app, _folder) = open(false);
    interact(&mut app, |ui| {
        ui.click("Profiles").expect("Profiles");
    });
    assert!(shows(&app, "Link a Discord account to edit its profile."));
    assert!(!shows(&app, "Edit main profile"));
    assert!(app.client.sent.is_empty());
}

#[test]
fn account_server_chooser_opens_the_existing_server_profile_editor() {
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    let _ = app.update(Message::Settings(Event::Open(Section::Account)));
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Profiles").expect("Profiles");
    });
    assert!(
        !shows(&app, "Test"),
        "server choices are only built when opened"
    );
    interact(&mut app, |ui| {
        ui.click("Choose a server").expect("the server chooser");
    });
    assert!(shows(&app, "Test"));
    assert!(
        app.client.sent.is_empty(),
        "the list uses the existing snapshot"
    );
    interact(&mut app, |ui| {
        ui.click("Test").expect("the server");
    });
    assert_eq!(
        app.client
            .state()
            .profile_editor
            .as_ref()
            .expect("the existing editor")
            .server
            .as_deref(),
        Some("discord:10")
    );
    assert!(app.win.settings.get().is_none());
    assert!(
        matches!(app.client.sent.as_slice(), [Command::Discord(DiscordCommand::OwnProfile { server: Some(server) })] if server == "discord:10")
    );
}

#[test]
fn account_server_chooser_has_an_empty_state_and_resets_when_leaving_its_tab() {
    let (mut app, _folder) = open(true);
    interact(&mut app, |ui| {
        ui.click("Profiles").expect("Profiles");
    });
    interact(&mut app, |ui| {
        ui.click("Choose a server").expect("the server chooser");
    });
    assert!(shows(&app, "No server profiles are available."));
    interact(&mut app, |ui| {
        ui.click("Devices").expect("Devices");
    });
    interact(&mut app, |ui| {
        ui.click("Profiles").expect("Profiles");
    });
    assert!(shows(&app, "Choose a server"));
    assert!(!shows(&app, "Close server list"));
    let _ = app.update(Message::Settings(Event::AccountPage(
        AccountEvent::EditServer("discord:missing".into()),
    )));
    assert!(app.client.state().profile_editor.is_none());
    assert!(app.client.sent.is_empty());
}

#[test]
fn account_server_chooser_rejects_unavailable_servers_including_stale_choices() {
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    change(&mut app, |state| {
        std::sync::Arc::make_mut(&mut state.servers[0]).unavailable = true;
    });
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    let _ = app.update(Message::Settings(Event::Open(Section::Account)));
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Profiles").expect("Profiles");
    });
    interact(&mut app, |ui| {
        ui.click("Choose a server").expect("the server chooser");
    });
    assert!(shows(&app, "No server profiles are available."));
    let _ = app.update(Message::Settings(Event::AccountPage(
        AccountEvent::EditServer("discord:10".into()),
    )));
    assert!(app.client.state().profile_editor.is_none());
    assert!(app.client.sent.is_empty());
}

#[test]
fn the_profile_editor_manages_connections_in_settings() {
    let (mut app, _folder) = open(true);
    let _ = app.update(Message::Settings(Event::EditProfile));
    app.client.apply(ChatEvent::OwnProfile {
        server: None,
        result: Ok(yapfast_core::chat::OwnProfile {
            name: "Alice".into(),
            fallback: yapfast_core::chat::Profile {
                id: "42".into(),
                name: "Alice".into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    });
    let _ = app.sync();
    assert!(!shows(&app, "Connections help"));
    let messages = {
        let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.click("Manage connections")
            .expect("the link to the Connections tab");
        ui.into_messages().collect::<Vec<_>>()
    };
    assert!(matches!(
        messages.as_slice(),
        [Message::Settings(Event::Connections)]
    ));
    for message in messages {
        let _ = app.update(message);
    }
    let settings = app.win.settings.get().expect("Settings open");
    assert_eq!(settings.section(), Section::Account);
    assert_eq!(settings.account.tab, Tab::Connections);
}
