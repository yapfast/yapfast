//! Account settings use the existing profile editor and explicit browser
//! handoffs. Visiting a tab or opening help never sends an account command.

use iced_test::Simulator;
use yapfast_core::chat::{Command, DiscordCommand};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    settings::{
        Event, Section,
        account::{Event as AccountEvent, Tab, Workflow},
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

#[test]
fn account_tabs_stay_in_the_account_section_without_fetching_data() {
    let (mut app, _folder) = open(true);
    for (label, tab, heading) in [
        ("Profiles", Tab::Profiles, "Your profiles"),
        ("Connections", Tab::Connections, "Connected accounts"),
        ("Authorized Apps", Tab::AuthorizedApps, "App access"),
        ("Devices", Tab::Devices, "Signed-in devices"),
        ("Nitro & Billing", Tab::Billing, "Nitro and payments"),
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
            "navigation fetches no account data"
        );
    }
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
fn the_existing_editor_labels_the_connections_article_as_help() {
    let (mut app, _folder) = open(true);
    let _ = app.update(Message::Settings(Event::EditProfile));
    app.client.apply(yapfast_core::chat::Event::OwnProfile {
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
    assert!(!shows(&app, "+  Add Connection"));
    let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    ui.click("Connections help")
        .expect("the labelled help link");
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Link(url, _)] if url == Workflow::ConnectionsHelp.url())
    );
}

#[test]
fn account_workflow_buttons_have_honest_labels_and_safe_destinations() {
    let (mut app, _folder) = open(true);
    for (tab, workflow) in [
        (Tab::Account, Workflow::Discord),
        (Tab::Account, Workflow::UsernameHelp),
        (Tab::Account, Workflow::EmailHelp),
        (Tab::Account, Workflow::PhoneHelp),
        (Tab::Account, Workflow::PasswordHelp),
        (Tab::Account, Workflow::MfaHelp),
        (Tab::Connections, Workflow::Discord),
        (Tab::Connections, Workflow::ConnectionsHelp),
        (Tab::AuthorizedApps, Workflow::Discord),
        (Tab::AuthorizedApps, Workflow::AuthorizedAppsHelp),
        (Tab::Devices, Workflow::Discord),
        (Tab::Devices, Workflow::LoginHelp),
        (Tab::Billing, Workflow::Discord),
        (Tab::Billing, Workflow::Nitro),
        (Tab::Billing, Workflow::BillingHelp),
    ] {
        let _ = app.update(Message::Settings(Event::AccountPage(AccountEvent::Show(
            tab,
        ))));
        let messages = {
            let mut ui =
                Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
            ui.click(workflow.label())
                .expect("a working browser button");
            ui.into_messages().collect::<Vec<_>>()
        };
        assert!(
            matches!(messages.as_slice(), [Message::Settings(Event::AccountPage(AccountEvent::Open(actual)))] if *actual == workflow)
        );
        let url = workflow.url();
        if workflow == Workflow::Discord {
            assert_eq!(url, "https://discord.com/app");
            assert_eq!(workflow.label(), "Open Discord in browser");
        } else if workflow == Workflow::Nitro {
            assert_eq!(url, "https://discord.com/nitro");
        } else {
            assert!(url.starts_with("https://support.discord.com/hc/en-us/articles/"));
            assert!(workflow.label().ends_with("help"));
        }
        for message in messages {
            let _ = app.update(message);
        }
        assert!(app.win.settings.get().is_some());
        assert!(
            app.client.sent.is_empty(),
            "a browser handoff sends no account command"
        );
    }
}

#[test]
fn external_settings_explain_the_manual_navigation() {
    let (mut app, _folder) = open(true);
    for (tab, path) in [
        (Tab::Account, "User Settings > My Account"),
        (Tab::Connections, "User Settings > Connections"),
        (Tab::AuthorizedApps, "User Settings > Authorized Apps"),
        (Tab::Devices, "User Settings > Devices"),
        (Tab::Billing, "User Settings > Subscriptions"),
    ] {
        let _ = app.update(Message::Settings(Event::AccountPage(AccountEvent::Show(
            tab,
        ))));
        assert!(shows(&app, path));
        assert!(shows(
            &app,
            "Opens Discord's web app. Choose the settings page there."
        ));
    }
    assert!(shows(&app, "User Settings > Billing"));
}
