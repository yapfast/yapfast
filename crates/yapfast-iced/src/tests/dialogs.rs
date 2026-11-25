//! The account's popover, the custom status's dialog and the quick
//! switcher: the custom status starts from the one set and saves what's
//! written; the quick switcher finds a place and goes there with the keys.

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
    assert!(app.status.popover.is_none(), "the popover goes");
    assert!(shows(&app, "Set a custom status") && shows(&app, "CLEAR AFTER"));
    let draft = app.status.draft.clone().expect("open");
    assert_eq!(draft.text, "Testing");
    assert_eq!(draft.emoji.as_deref(), Some("🧪"));
    let _ = app.update(Message::Status(StatusEvent::Text("Away".into())));
    interact(&mut app, |ui| {
        ui.click("1 hour").expect("Clear after");
    });
    assert_eq!(app.status.draft.as_ref().expect("open").clear, 2);
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
    assert!(app.status.draft.is_none());
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
    let pitch = crate::switcher::PITCH;
    let top = 12.0 * pitch;
    let height = crate::switcher::list_height(app.window.height);
    assert!(
        app.quick.offset <= top && top + pitch <= app.quick.offset + height,
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
    assert!(app.quick.offset + height >= (last + 1) as f32 * pitch);
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
                guild: None,
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
    assert!(
        crate::status::popover_playing(&app),
        "its time kept up to the minute"
    );
    press(&mut app, key::Named::Escape);
    assert!(app.status.popover.is_none());
    assert!(app.profiles.account.is_none(), "let go as it closes");
}

#[test]
fn the_account_popover_shows_rich_presence_and_follows_live_changes() {
    use std::sync::Arc;
    use yapfast_core::chat::{Activity, ActivityKind, Picture, Pixels, Playing};

    let (mut app, _folder) = preview();
    let activity = Activity {
        kind: ActivityKind::Playing,
        name: "Zed".into(),
        details: Some("In yapfast".into()),
        state: Some("Working on README.md".into()),
        party: None,
        party_id: None,
        application_id: Some("42".into()),
        start: Some(yapfast_core::clock::now_ms() - 33_000),
        end: None,
        large_image: Some("activity:zed".into()),
        large_text: Some("Zed".into()),
        small_image: Some("activity:workspace".into()),
        small_text: Some("Workspace".into()),
        url: None,
        link: None,
        emoji: None,
        created: None,
        display: None,
        console: None,
        buttons: None,
    };
    change(&mut app, |state| {
        // Rich Presence does not wait for a profile HTTP request.
        state.account_profile = None;
        state.discord_activities = Arc::from([activity]);
        state.discord_playing = Some(Playing {
            name: "Zed".into(),
            started_ms: Some(yapfast_core::clock::now_ms() - 33_000),
        });
        for (url, color) in [
            ("activity:zed", [38, 140, 200, 255]),
            ("activity:workspace", [45, 45, 50, 255]),
        ] {
            state.pictures.insert(
                url.into(),
                Arc::new(Picture {
                    width: 4,
                    height: 4,
                    rgba: Pixels::new(4, 4, color.repeat(16)),
                    animation: None,
                }),
            );
        }
    });
    assert_eq!(app.status.activity_pictures().count(), 0);
    let _ = app.update(Message::Status(StatusEvent::Toggle));
    for line in ["Playing", "Zed", "In yapfast", "Working on README.md"] {
        assert!(shows(&app, line), "{line}");
    }
    assert_eq!(
        app.status.activity_pictures().collect::<Vec<_>>(),
        ["activity:zed", "activity:workspace"]
    );
    assert!(
        !crate::status::popover_playing(&app),
        "the widget's own second clock replaces minute updates"
    );

    if let Some(path) = std::env::var_os("YAPFAST_TEST_ACCOUNT_SNAPSHOT") {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.snapshot(&iced::Theme::Dark)
            .unwrap()
            .matches_image(path)
            .unwrap();
    }

    change(&mut app, |state| {
        Arc::make_mut(&mut state.discord_activities)[0].state =
            Some("Working on Cargo.toml".into());
    });
    assert!(shows(&app, "Working on Cargo.toml"));
    assert!(!shows(&app, "Working on README.md"));
    change(&mut app, |state| {
        state.discord_activities = Default::default();
        state.discord_playing = None;
    });
    assert!(!shows(&app, "Zed"), "clearing removes the activity");
    assert_eq!(app.status.activity_pictures().count(), 0);
    press(&mut app, key::Named::Escape);
    assert!(app.status.popover.is_none());
}

/// One's own card follows what this account shares while it's open, such
/// as a song's line of lyrics set here, and the account panel's line too.
#[test]
fn the_accounts_own_card_follows_its_activities() {
    use std::sync::Arc;
    use yapfast_core::chat::{Activity, ActivityKind, Profile};
    let (mut app, _folder) = preview();
    let song = |line: &str| Activity {
        details: Some(line.into()),
        application_id: Some("1".into()),
        ..Activity::new(ActivityKind::Listening, "Lyrics")
    };
    change(&mut app, |state| {
        state.discord_user = Some("42".into());
        state.account_profile = Some(Arc::new(Profile {
            id: "42".into(),
            name: "Alice".into(),
            ..Default::default()
        }));
        state.discord_activities = Arc::from([song("First line")]);
    });
    let _ = app.update(Message::Status(StatusEvent::Toggle));
    let title = |app: &crate::app::App| {
        let (_, shown) = app.profiles.account.as_ref().expect("open");
        shown.look.activities.first().map(|look| look.name.clone())
    };
    assert_eq!(title(&app).as_deref(), Some("First line"));
    change(&mut app, |state| {
        state.discord_activities = Arc::from([song("Second line")]);
    });
    assert_eq!(title(&app).as_deref(), Some("Second line"));
}
