//! The account's popover and the dialogs of `ui/dialogs.slint`, as the
//! Slint client's tests of them: the custom status starts from the one set
//! and saves what's written; the quick switcher finds a place and goes
//! there with the keys.

use iced::keyboard::{Key, key};
use yapfast_core::chat::CustomStatus;

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    status::Event as StatusEvent,
    switcher::Event as SwitcherEvent,
};

fn press(app: &mut App, named: key::Named) {
    let _ = app.update(Message::Key(Key::Named(named), Default::default()));
}

#[test]
fn the_custom_status_starts_from_the_one_set_and_saves_whats_written() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.discord_user = Some("42".into());
        state.custom_status = Some(CustomStatus {
            text: "Testing".into(),
            emoji: Some("🧪".into()),
            ..Default::default()
        });
    });
    // The account card opens the popover, and its row the dialog.
    let _ = app.update(Message::Status(StatusEvent::Toggle));
    assert!(shows(&app, "Change custom status") && shows(&app, "User Settings"));
    interact(&mut app, |ui| {
        ui.click("Change custom status").expect("the row");
    });
    assert!(app.popover.is_none(), "the popover goes");
    assert!(shows(&app, "Set a custom status") && shows(&app, "CLEAR AFTER"));
    let draft = app.status_draft.clone().expect("open");
    assert_eq!(draft.text, "Testing");
    assert_eq!(draft.emoji.as_deref(), Some("🧪"));
    let _ = app.update(Message::Status(StatusEvent::Text("Away".into())));
    interact(&mut app, |ui| {
        ui.click("1 hour").expect("Clear after");
    });
    assert_eq!(app.status_draft.as_ref().expect("open").clear, 2);
    interact(&mut app, |ui| {
        ui.click("Save").expect("Save");
    });
    // There's no Discord here: the core sent it, and its backend said why
    // it couldn't.
    let editor = app.client.state().status_editor.clone().expect("open");
    assert!(
        editor.saving
            || editor
                .error
                .as_deref()
                .is_some_and(|error| error.contains("connection")),
        "{editor:?}"
    );
    // Escape closes it, and what was written goes with it.
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().status_editor.is_none());
    assert!(app.status_draft.is_none());
}

#[test]
fn the_quick_switcher_finds_a_place_and_goes_there() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Switcher(SwitcherEvent::Toggle));
    assert!(app.client.state().switcher.is_some());
    assert!(shows(
        &app,
        "Start a search with @ for people, # for text channels, ! for voice channels or * for servers."
    ));
    let _ = app.update(Message::Switcher(SwitcherEvent::Search("gen".into())));
    assert!(shows(&app, "general"));
    let found = app.client.state().switcher_places().len();
    assert!(found >= 1);
    // Down goes round to the first again past the last.
    for _ in 0..found {
        press(&mut app, key::Named::ArrowDown);
    }
    assert_eq!(app.switcher_chosen, 0);
    let first = match &app.client.state().switcher_places()[0] {
        yapfast_core::application::Place::Room(room) => room.id.clone(),
        yapfast_core::application::Place::Server(server) => server.id.clone(),
    };
    press(&mut app, key::Named::Enter);
    let state = app.client.state();
    assert!(state.switcher.is_none(), "it closes");
    assert!(
        state.selected.as_deref() == Some(first.as_str())
            || state.servers.iter().any(|server| server.id == first),
        "it went there"
    );
    // Nothing found says so; Escape closes it.
    let _ = app.update(Message::Switcher(SwitcherEvent::Toggle));
    let _ = app.update(Message::Switcher(SwitcherEvent::Search("zzzz".into())));
    assert!(shows(&app, "Nothing goes by that name."));
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().switcher.is_none());
}
