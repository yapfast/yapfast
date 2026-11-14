//! The window driven through `iced_test`'s headless simulator, on the
//! core's preview: each test builds the view, clicks and types into it as a
//! person would, and hands the messages that came out to `App::update`, as
//! iced's runtime does. The tasks `update` returns (focus, the clipboard,
//! timers) don't run here, and the simulator keeps widget state only while
//! it lives, so a field is clicked in the same simulator it's typed into.

use std::{
    path::PathBuf,
    sync::{Arc, Once},
};

use iced::{
    Point, Settings,
    keyboard::{self, key},
    mouse,
    widget::Id,
};
use iced_test::{Simulator, selector::Candidate, simulator::click};
use yapfast_core::{
    application::{Action, Compose},
    runtime::BackgroundRuntime,
};

use crate::{
    app::{App, Message},
    client::Client,
    composer::COMPOSER,
    fonts,
};

/// The preview at the window's own size, its data in a folder of its own.
fn preview() -> (App, tempfile::TempDir) {
    static FONTS: Once = Once::new();
    FONTS.call_once(|| fonts::install(&[]));
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let runtime = Arc::new(BackgroundRuntime::new().expect("runtime"));
    let mut client = Client::new(runtime, folder.path().into());
    client.dispatch(Action::Preview);
    let (app, _) = App::new(client, (1120, 760));
    (app, folder)
}

/// The workspace's ignored `.local`, where tests keep their fixtures.
fn local() -> PathBuf {
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.local");
    std::fs::create_dir_all(&local).expect(".local");
    local
}

/// Builds the view, lets `act` work on it, and applies what came out.
fn interact(app: &mut App, act: impl FnOnce(&mut Simulator<'_, Message>)) {
    let messages: Vec<Message> = {
        let mut ui = Simulator::with_size(Settings::default(), app.window, app.view());
        act(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = app.update(message);
    }
}

/// Whether the window shows `text`.
fn shows(app: &App, text: &str) -> bool {
    let mut ui = Simulator::with_size(Settings::default(), app.window, app.view());
    ui.find(text).is_ok()
}

/// The middle of the last text in view that reads `text`.
fn last(ui: &mut Simulator<'_, Message>, text: &str) -> Point {
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content == text
        {
            found = Some(bounds.center());
        }
        None::<()>
    });
    found.unwrap_or_else(|| panic!("{text:?} isn't in view"))
}

/// A right click on the message reading `text`.
fn right_click(ui: &mut Simulator<'_, Message>, text: &str) {
    let body = ui.find(text).expect("the message");
    ui.point_at(body.visible_bounds().expect("in view").center());
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
}

#[test]
fn the_composer_sends_on_enter() {
    let (mut app, _folder) = preview();
    let before = app.client.state().messages.len();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("hello there");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    let messages = &app.client.state().messages;
    assert_eq!(messages.len(), before + 1);
    assert_eq!(messages.last().expect("sent").body, "hello there");
    assert!(app.draft.is_empty(), "the core empties the draft once sent");
    assert!(shows(&app, "hello there"));
}

#[test]
fn a_channel_clicked_in_the_list_opens() {
    let (mut app, _folder) = preview();
    assert!(!shows(&app, "What are we playing next?"));
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    assert_eq!(app.header, ("games".to_owned(), false));
    assert!(shows(&app, "What are we playing next?"));
}

#[test]
fn a_member_clicked_in_the_list_shows_their_card_until_a_click_outside() {
    let (mut app, _folder) = preview();
    // The last "Riley" is the member list's: the timeline comes before it.
    interact(&mut app, |ui| {
        let line = last(ui, "Riley");
        ui.point_at(line);
        ui.simulate(click());
    });
    let view = app.client.state().profile.as_ref().expect("the profile");
    assert_eq!(view.profile.name, "Riley");
    assert_eq!(app.card.as_ref().expect("the card").user, view.profile.id);
    assert!(shows(&app, "Riley"));
    interact(&mut app, |ui| {
        ui.point_at(Point::new(500.0, 300.0));
        ui.simulate(click());
    });
    assert!(app.card.is_none());
    assert!(app.client.state().profile.is_none());
}

#[test]
fn a_messages_menu_replies_to_it() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    interact(&mut app, |ui| right_click(ui, text));
    assert!(app.menu.is_some(), "the menu opened");
    assert!(
        !shows(&app, "Delete message"),
        "Riley's message isn't ours to delete"
    );
    interact(&mut app, |ui| {
        ui.click("Reply").expect("Reply");
    });
    assert!(app.menu.is_none());
    match app.client.state().compose() {
        Some(Compose::Reply { author, .. }) => assert_eq!(author, "Riley"),
        _ => panic!("not replying"),
    }
    assert!(shows(&app, &format!("Replying to Riley: {text}")));
}

#[test]
fn deleting_ones_own_message_asks_first() {
    let (mut app, _folder) = preview();
    let text = "Nice, let's get a game going.";
    interact(&mut app, |ui| right_click(ui, text));
    interact(&mut app, |ui| {
        ui.click("Delete message").expect("Delete message");
    });
    assert!(app.client.state().confirm_delete.is_some());
    assert!(shows(&app, "Are you sure you want to delete this message?"));
    interact(&mut app, |ui| {
        ui.click("Delete").expect("the dialog's Delete");
    });
    assert!(app.client.state().confirm_delete.is_none());
    assert!(!shows(&app, text), "the message went");
}
