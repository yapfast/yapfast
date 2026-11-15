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
    assert!(app.account_popover.is_none(), "the popover goes");
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
    // The core sent what was written, clearing in an hour...
    let sent = app
        .client
        .sent
        .iter()
        .rev()
        .find_map(|command| match command {
            yapfast_core::chat::Command::Discord(
                yapfast_core::chat::DiscordCommand::SetCustomStatus(status),
            ) => Some(status.clone()),
            _ => None,
        });
    let status = sent.flatten().expect("a status sent");
    assert_eq!(status.text, "Away");
    assert_eq!(status.emoji.as_deref(), Some("🧪"));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let left = status.expires_at_ms.expect("it clears") as i64 - now as i64;
    assert!((left - 3_600_000).abs() < 60_000, "{left} ms left");
    // ...and, with no Discord here, its backend said why it couldn't.
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
fn the_quick_switchers_arrows_keep_the_lit_place_in_view() {
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        let template = (*state.rooms[0]).clone();
        for index in 0..20 {
            state
                .rooms
                .push(std::sync::Arc::new(yapfast_core::chat::ChatRoom {
                    id: format!("garden-{index}"),
                    name: format!("garden-{index}"),
                    ..template.clone()
                }));
        }
    });
    let _ = app.update(Message::Switcher(SwitcherEvent::Toggle));
    let _ = app.update(Message::Switcher(SwitcherEvent::Search("garden".into())));
    assert!(app.quick.places.len() >= 20, "{}", app.quick.places.len());
    for _ in 0..12 {
        press(&mut app, key::Named::ArrowDown);
    }
    assert_eq!(app.quick.chosen, 12);
    let top = 12.0 * 40.0;
    let height = ((app.window.height - 48.0).min(440.0) - 2.0 * 16.0 - 52.0 - 60.0).max(40.0);
    assert!(
        app.quick.offset <= top && top + 40.0 <= app.quick.offset + height,
        "row 12 at {top} px in view of {} px from {} px",
        height,
        app.quick.offset
    );
    // Up past the first goes round to the last, at the list's end.
    for _ in 0..13 {
        press(&mut app, key::Named::ArrowUp);
    }
    let last = app.quick.places.len() - 1;
    assert_eq!(app.quick.chosen, last);
    assert!(app.quick.offset + height >= (last + 1) as f32 * 40.0);
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
    assert_eq!(app.quick.chosen, 0);
    let first = match &app.client.state().switcher_places()[0] {
        yapfast_core::application::Place::Room(room) => room.id.clone(),
        yapfast_core::application::Place::Server(server) => server.id.clone(),
    };
    press(&mut app, key::Named::Enter);
    let state = app.client.state();
    assert!(state.switcher.is_none(), "it closes");
    let there = match &state.navigation {
        yapfast_core::application::Navigation::Server(server) => *server == first,
        _ => false,
    };
    assert!(
        state.selected.as_deref() == Some(first.as_str()) || there,
        "it went there"
    );
    // Nothing found says so; Escape closes it.
    let _ = app.update(Message::Switcher(SwitcherEvent::Toggle));
    let _ = app.update(Message::Switcher(SwitcherEvent::Search("zzzz".into())));
    assert!(shows(&app, "Nothing goes by that name."));
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().switcher.is_none());
}

#[test]
fn the_account_popover_shows_the_accounts_own_card_while_open() {
    use std::sync::Arc;
    use yapfast_core::chat::{Playing, Profile, ServerTag};
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.discord_user = Some("42".into());
        state.account_profile = Some(Arc::new(Profile {
            id: "42".into(),
            name: "Alice".into(),
            tag: Some(ServerTag {
                text: "YAP".into(),
                badge: None,
            }),
            ..Default::default()
        }));
        state.discord_playing = Some(Playing {
            name: "Factorio".into(),
            started_ms: Some(yapfast_core::clock::now_ms() - 125 * 60_000),
        });
    });
    assert!(app.profiles.account.is_none(), "made only while it shows");
    let _ = app.update(Message::Status(StatusEvent::Toggle));
    assert!(app.profiles.account.is_some());
    // The server tag beside the name, and the game played, as Discord's.
    assert!(shows(&app, "YAP"));
    assert!(shows(&app, "Playing") && shows(&app, "Factorio"));
    assert!(shows(&app, "2 hr 5 min") || shows(&app, "2 hr 4 min"));
    assert!(app.popover_playing(), "its time kept up to the minute");
    press(&mut app, key::Named::Escape);
    assert!(app.account_popover.is_none());
    assert!(app.profiles.account.is_none(), "let go as it closes");
}
