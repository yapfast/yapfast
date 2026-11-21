//! My Account's tabs: the authorized apps, the devices and the connected
//! accounts are read when asked for and answered once; revoking, signing out
//! and unlinking go one at a time and change the list only when Discord
//! agrees; and the username, email and password change only with the current
//! password. Nothing is asked of Discord while it isn't connected.

use yapfast_discord::service::{Snapshot, Status};

use super::*;
use crate::account_tools::{AccountField, AuthorizedApp, Connection, Device};

fn online() -> Application {
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));
    app
}

fn asks(effects: &[Effect], wanted: fn(&DiscordCommand) -> bool) -> bool {
    effects.iter().any(
        |effect| matches!(effect, Effect::Backend(Command::Discord(command)) if wanted(command)),
    )
}

fn connection(service: &str, id: &str) -> Connection {
    Connection {
        service: service.into(),
        label: service.into(),
        id: id.into(),
        name: "name".into(),
        visible: true,
        ..Default::default()
    }
}

#[test]
fn nothing_is_asked_while_discord_is_not_connected() {
    let mut app = Application::default();
    for action in [
        Action::LoadAuthorizedApps,
        Action::LoadDevices,
        Action::LoadConnections,
        Action::RevokeAuthorizedApp("a".into()),
        Action::LogOutDevices(vec!["h".into()]),
        Action::DisconnectAccount {
            service: "github".into(),
            id: "1".into(),
        },
        Action::LinkConnection("github".into()),
        Action::ChangeAccount {
            field: AccountField::Username,
            value: "name".into(),
            password: "pw".into(),
        },
    ] {
        assert!(app.dispatch(action).is_empty());
    }
    assert_eq!(app.state.account_tools, Default::default());
}

#[test]
fn each_list_is_read_once_at_a_time_and_keeps_why_it_failed() {
    let mut app = online();
    let effects = app.dispatch(Action::LoadDevices);
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadDevices)));
    assert!(app.state.account_tools.devices.loading);
    assert!(
        app.dispatch(Action::LoadDevices).is_empty(),
        "it is already being read"
    );
    app.apply(Event::DevicesLoaded {
        result: Err("busy".into()),
    });
    let devices = &app.state.account_tools.devices;
    assert!(!devices.loading && !devices.loaded);
    assert_eq!(devices.error.as_deref(), Some("busy"));

    // Reading again clears the error, and an answer keeps the list.
    assert!(!app.dispatch(Action::LoadDevices).is_empty());
    assert!(app.state.account_tools.devices.error.is_none());
    app.apply(Event::DevicesLoaded {
        result: Ok(vec![Device {
            id: "h1".into(),
            current: true,
            ..Default::default()
        }]),
    });
    assert!(app.state.account_tools.devices.loaded);
    assert_eq!(app.state.account_tools.devices.items.len(), 1);
    // A later failure keeps the last list.
    app.dispatch(Action::LoadDevices);
    app.apply(Event::DevicesLoaded {
        result: Err("busy".into()),
    });
    assert_eq!(app.state.account_tools.devices.items.len(), 1);
}

#[test]
fn an_authorization_is_taken_back_only_when_discord_agrees() {
    let mut app = online();
    app.dispatch(Action::LoadAuthorizedApps);
    app.apply(Event::AuthorizedAppsLoaded {
        result: Ok(vec![
            AuthorizedApp {
                id: "a1".into(),
                ..Default::default()
            },
            AuthorizedApp {
                id: "a2".into(),
                ..Default::default()
            },
        ]),
    });
    let effects = app.dispatch(Action::RevokeAuthorizedApp("a1".into()));
    assert!(asks(
        &effects,
        |c| matches!(c, DiscordCommand::RevokeAuthorizedApp { id } if id == "a1")
    ));
    assert!(
        app.dispatch(Action::RevokeAuthorizedApp("a1".into()))
            .is_empty(),
        "one at a time"
    );
    app.apply(Event::AuthorizedAppRevoked {
        id: "a1".into(),
        result: Err("no".into()),
    });
    assert_eq!(app.state.account_tools.apps.items.len(), 2);
    assert_eq!(app.state.account_tools.error.as_deref(), Some("no"));
    assert!(app.state.account_tools.busy.is_empty());

    // Asking again clears the old reason; success removes only that app.
    assert!(
        !app.dispatch(Action::RevokeAuthorizedApp("a1".into()))
            .is_empty()
    );
    assert!(app.state.account_tools.error.is_none());
    app.apply(Event::AuthorizedAppRevoked {
        id: "a1".into(),
        result: Ok(()),
    });
    let left: Vec<_> = app
        .state
        .account_tools
        .apps
        .items
        .iter()
        .map(|app| app.id.as_str())
        .collect();
    assert_eq!(left, ["a2"]);
}

#[test]
fn devices_sign_out_together_and_leave_the_list_on_success() {
    let mut app = online();
    app.dispatch(Action::LoadDevices);
    app.apply(Event::DevicesLoaded {
        result: Ok(["h1", "h2", "h3"]
            .map(|id| Device {
                id: id.into(),
                ..Default::default()
            })
            .into()),
    });
    let effects = app.dispatch(Action::LogOutDevices(vec!["h2".into(), "h3".into()]));
    assert!(asks(
        &effects,
        |c| matches!(c, DiscordCommand::LogOutDevices { ids } if ids == &["h2".to_owned(), "h3".to_owned()])
    ));
    // Those already going are not asked for again.
    let again = app.dispatch(Action::LogOutDevices(vec!["h2".into()]));
    assert!(again.is_empty());
    app.apply(Event::DevicesLoggedOut {
        ids: vec!["h2".into(), "h3".into()],
        result: Ok(()),
    });
    let left: Vec<_> = app
        .state
        .account_tools
        .devices
        .items
        .iter()
        .map(|device| device.id.as_str())
        .collect();
    assert_eq!(left, ["h1"]);
    assert!(app.state.account_tools.busy.is_empty());
}

#[test]
fn a_connection_changes_how_it_shows_when_discord_agrees() {
    let mut app = online();
    app.dispatch(Action::LoadConnections);
    app.apply(Event::ConnectionsLoaded {
        result: Ok(vec![connection("github", "7"), connection("github", "8")]),
    });
    // Asking for nothing sends nothing.
    assert!(
        app.dispatch(Action::ShowConnection {
            service: "github".into(),
            id: "7".into(),
            visible: None,
            activity: None,
        })
        .is_empty()
    );
    let effects = app.dispatch(Action::ShowConnection {
        service: "github".into(),
        id: "7".into(),
        visible: None,
        activity: Some(true),
    });
    assert!(!effects.is_empty());
    // The other github account has its own busy key.
    assert!(
        !app.dispatch(Action::ShowConnection {
            service: "github".into(),
            id: "8".into(),
            visible: Some(false),
            activity: None,
        })
        .is_empty()
    );
    app.apply(Event::ConnectionShown {
        service: "github".into(),
        id: "7".into(),
        visible: None,
        activity: Some(true),
        result: Ok(()),
    });
    app.apply(Event::ConnectionShown {
        service: "github".into(),
        id: "8".into(),
        visible: Some(false),
        activity: None,
        result: Err("no".into()),
    });
    let items = &app.state.account_tools.connections.items;
    assert!(items[0].show_activity && items[0].visible);
    assert!(!items[1].show_activity && items[1].visible, "refused");

    // Unlinking removes only that account, and only on success.
    app.dispatch(Action::DisconnectAccount {
        service: "github".into(),
        id: "7".into(),
    });
    app.apply(Event::AccountDisconnected {
        service: "github".into(),
        id: "7".into(),
        result: Ok(()),
    });
    let items = &app.state.account_tools.connections.items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "8");
}

#[test]
fn a_service_is_linked_on_a_secure_page_of_its_own() {
    let mut app = online();
    let effects = app.dispatch(Action::LinkConnection("spotify".into()));
    assert!(asks(
        &effects,
        |c| matches!(c, DiscordCommand::LinkConnection { service } if service == "spotify")
    ));
    assert!(
        app.dispatch(Action::LinkConnection("spotify".into()))
            .is_empty()
    );
    let opened = app.apply(Event::ConnectionLink {
        service: "spotify".into(),
        result: Ok("https://accounts.spotify.com/authorize".into()),
    });
    assert!(opened.iter().any(
        |effect| matches!(effect, Effect::OpenPage(page) if page == "https://accounts.spotify.com/authorize")
    ));
    assert!(app.state.account_tools.busy.is_empty());

    // A page that is not secure is never opened.
    app.dispatch(Action::LinkConnection("spotify".into()));
    let refused = app.apply(Event::ConnectionLink {
        service: "spotify".into(),
        result: Ok("http://example.com".into()),
    });
    assert!(
        !refused
            .iter()
            .any(|effect| matches!(effect, Effect::OpenPage(_)))
    );
    assert!(app.state.account_tools.error.is_some());
}

#[test]
fn the_account_changes_with_its_password_one_change_at_a_time() {
    let mut app = online();
    let change = |field, value: &str, password: &str| Action::ChangeAccount {
        field,
        value: value.into(),
        password: password.into(),
    };
    assert!(
        app.dispatch(change(AccountField::Email, "a@b.c", ""))
            .is_empty(),
        "no password"
    );
    assert!(
        app.dispatch(change(AccountField::Email, "   ", "pw"))
            .is_empty(),
        "no value"
    );
    let effects = app.dispatch(change(AccountField::Username, " newname ", "pw"));
    assert!(asks(&effects, |c| matches!(
        c,
        DiscordCommand::ChangeAccount { field: AccountField::Username, value, password }
            if value == "newname" && password == "pw"
    )));
    assert_eq!(
        app.state.account_tools.change.busy,
        Some(AccountField::Username)
    );
    assert!(
        app.dispatch(change(AccountField::Email, "a@b.c", "pw"))
            .is_empty(),
        "one change at a time"
    );
    app.apply(Event::AccountChanged {
        field: AccountField::Username,
        result: Err("taken".into()),
    });
    let state = &app.state.account_tools.change;
    assert_eq!(state.busy, None);
    assert_eq!(state.error.as_deref(), Some("taken"));
    assert_eq!(state.done, None);

    assert!(
        !app.dispatch(change(AccountField::Username, "other", "pw"))
            .is_empty()
    );
    assert!(app.state.account_tools.change.error.is_none());
    app.apply(Event::AccountChanged {
        field: AccountField::Username,
        result: Ok(()),
    });
    assert_eq!(
        app.state.account_tools.change.done,
        Some(AccountField::Username)
    );
}
