//! Signing in, trust, and what happens when the connection or the session goes.

use super::*;

#[test]
fn stopped_transport_unblocks_retry_and_failed_logout_keeps_session() {
    let mut app = connected();
    app.dispatch(Action::SetDraft("Retry me".into()));
    let first = send_command(app.dispatch(Action::Send));
    app.backend_failed(&first);
    assert!(app.state.sending.is_none());
    assert_eq!(app.state.draft(), "Retry me");
    let retry = send_command(app.dispatch(Action::Send));
    app.backend_failed(&retry);
    assert!(matches!(
        app.dispatch(Action::Logout).as_slice(),
        [Effect::Backend(Command::Logout)]
    ));
    app.backend_failed(&Command::Logout);
    assert!(!app.state.busy);
    assert_eq!(app.state.account.as_deref(), Some("@alice:test"));
    app.dispatch(Action::Logout);
    app.apply(Event::Error(
        "Sign-out failed. Your session is kept.".into(),
    ));
    assert_eq!(app.state.account.as_deref(), Some("@alice:test"));
    app.dispatch(Action::Logout);
    app.apply(Event::LoggedOut);
    assert!(app.state.account.is_none());
    assert!(app.state.rooms.is_empty());
    assert!(app.state.draft().is_empty());
    assert_eq!(app.state.reset_version, 1);
}

#[test]
fn security_gates_are_shared_by_every_ui_adapter() {
    let mut app = connected();
    app.dispatch(Action::SetDraft("Hello".into()));
    app.state.trusted = false;
    assert!(app.dispatch(Action::Send).is_empty());
    assert!(
        app.dispatch(Action::RequestJoinVoice("lounge".into()))
            .is_empty()
    );
    app.state.trusted = true;
    app.state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "dm")
        .unwrap()
        .encrypted = false;
    assert!(app.dispatch(Action::Send).is_empty());
    app.state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "lounge")
        .unwrap()
        .encrypted = false;
    assert!(
        app.dispatch(Action::RequestJoinVoice("lounge".into()))
            .is_empty()
    );
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("party-voice".into()));
    app.dispatch(Action::SetDraft("Never send text to voice".into()));
    assert!(app.dispatch(Action::Send).is_empty());
    assert!(matches!(
        app.dispatch(Action::RequestJoinVoice("party-voice".into()))
            .as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Join(_)))]
    ));
}

#[test]
fn login_recovery_and_error_state_do_not_depend_on_widgets() {
    let mut app = Application::default();
    assert!(
        app.dispatch(Action::Login {
            homeserver: "".into(),
            username: "alice".into(),
            password: "secret".into()
        })
        .is_empty()
    );
    assert!(!app.state.busy);
    let effects = app.dispatch(Action::Login {
        homeserver: " https://example.org ".into(),
        username: " @alice:example.org ".into(),
        password: " secret ".into(),
    });
    assert!(
        matches!(effects.as_slice(), [Effect::Start(chat::Start::Login { homeserver, username, password })] if homeserver == "https://example.org" && username == "@alice:example.org" && password == " secret ")
    );
    assert!(app.state.busy);
    assert!(app.dispatch(Action::Restore).is_empty());
    app.apply(Event::Error("Wrong credentials".into()));
    app.stopped();
    assert_eq!(app.state.notice.as_deref(), Some("Wrong credentials"));
    assert!(!app.state.busy);
    app = connected();
    assert!(
        matches!(app.dispatch(Action::Recover(" key ".into())).as_slice(), [Effect::Backend(Command::Recover(key))] if key == "key")
    );
    assert!(app.state.busy);
    app.backend_failed(&Command::Recover("key".into()));
    assert!(!app.state.busy);
    app.apply(Event::Voice(voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        ..Default::default()
    }));
    assert!(app.dispatch(Action::Recover("key".into())).is_empty());
    assert!(!app.state.busy);
}
