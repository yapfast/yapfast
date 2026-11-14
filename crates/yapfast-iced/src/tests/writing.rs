//! Writing, as the Slint client's tests of its composer and menus check
//! it (`src/ui/tests.rs`): breaking lines, the reply's bar and Escape,
//! editing in place, `:name` suggestions, the emoji picker, the message's
//! menu leading to Forward, and the keyboard's steps through messages.

use iced::{
    keyboard::{self, key},
    widget::Id,
};
use iced_test::Simulator;
use yapfast_core::application::Compose;

use super::{interact, preview, right_click, shows};
use crate::{
    app::Message,
    composer::{COMPOSER, EDIT},
    picker::{Tab, Target, kit::FIELD},
};

/// Riley's message and one of this account's in the preview.
const RILEYS: &str = "Yep. I should be free after dinner.";
const OWN: &str = "Nice, let's get a game going.";

/// A key pressed and let go with `modifiers` held.
fn chord(ui: &mut Simulator<'_, Message>, named: key::Named, modifiers: keyboard::Modifiers) {
    let key = keyboard::Key::Named(named);
    let text = (named == key::Named::Enter).then(|| "\n".into());
    ui.simulate([
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key.clone(),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
            repeat: false,
            text,
        }),
        iced::Event::Keyboard(keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
        }),
    ]);
}

/// A key no field takes, as the window's subscription hands it on.
fn window_key(app: &mut crate::app::App, key: keyboard::Key) {
    let _ = app.update(Message::Key(key, keyboard::Modifiers::default()));
}

#[test]
fn shift_enter_breaks_the_line_and_enter_sends_both() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("one");
        chord(ui, key::Named::Enter, keyboard::Modifiers::SHIFT);
        ui.typewrite("two");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    let sent = app.client.state().messages.last().expect("sent").clone();
    assert_eq!(sent.body, "one\ntwo");
}

#[test]
fn escape_in_the_composer_cancels_a_reply_and_keeps_the_draft() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| right_click(ui, RILEYS));
    interact(&mut app, |ui| {
        ui.click("Reply").expect("Reply");
    });
    let ping = |app: &crate::app::App| match app.client.state().compose() {
        Some(Compose::Reply { ping, .. }) => *ping,
        _ => panic!("not replying"),
    };
    let before = ping(&app);
    let label = if before { "@ On" } else { "@ Off" };
    interact(&mut app, |ui| {
        ui.click(label).expect("the mention's toggle");
    });
    assert_ne!(ping(&app), before, "the toggle turned it over");
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("on my way");
        ui.tap_key(keyboard::Key::Named(key::Named::Escape));
    });
    assert!(
        app.client.state().compose().is_none(),
        "Escape cancels the reply"
    );
    assert_eq!(app.draft.text(), "on my way", "and keeps the draft");
}

#[test]
fn up_in_an_empty_composer_edits_ones_last_message_in_its_row() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::ArrowUp));
    });
    let item = app.editing.clone().expect("an edit started");
    let own = app.client.state().message(&item).expect("the message");
    assert!(own.own, "the last of this account's own");
    let before = own.body.clone();
    assert_eq!(app.edit.text(), before, "the edit starts with its text");
    // Escape cancels it, and the text stays.
    interact(&mut app, |ui| {
        ui.click(Id::new(EDIT)).expect("the edit's field");
        ui.tap_key(keyboard::Key::Named(key::Named::Escape));
    });
    assert!(app.editing.is_none());
    // Again, and Enter saves what was added at its end.
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::ArrowUp));
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(EDIT)).expect("the edit's field");
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        chord(ui, key::Named::End, keyboard::Modifiers::CTRL);
        ui.typewrite(" Now?");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(app.editing.is_none(), "Enter saves and ends it");
    let saved = app.client.state().message(&item).expect("the message");
    assert_eq!(saved.body, format!("{before} Now?"));
}

#[test]
fn a_colon_word_suggests_emoji_and_enter_takes_the_first() {
    let (mut app, _folder) = preview();
    if super::twemoji().is_none() {
        eprintln!("no Twemoji font built yet: build the root crate first");
        return;
    }
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("nice :thumbsu");
    });
    assert!(!app.suggest.emoji.is_empty(), "emoji are suggested");
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(
        app.draft.text().starts_with("nice 👍"),
        "the emoji took the word's place: {:?}",
        app.draft.text()
    );
    assert!(app.suggest.emoji.is_empty(), "and the list closed");
}

#[test]
fn the_emoji_pickers_search_puts_its_first_match_in_the_composer() {
    let (mut app, _folder) = preview();
    if super::twemoji().is_none() {
        eprintln!("no Twemoji font built yet: build the root crate first");
        return;
    }
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    assert!(app.picker.is_some());
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the picker's search");
        ui.typewrite("grinning face");
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the picker's search");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(app.picker.is_none(), "picking closes the picker");
    assert!(app.draft.text().contains('😀'), "{:?}", app.draft.text());
}

#[test]
fn forward_in_a_messages_menu_opens_the_dialog_which_ticks_and_closes() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| right_click(ui, OWN));
    interact(&mut app, |ui| {
        ui.click("Forward").expect("Forward");
    });
    assert!(app.client.state().forwarding.is_some());
    assert!(shows(&app, "Forward To"));
    let first = app
        .client
        .state()
        .forward_places()
        .first()
        .map(|room| room.id.clone())
        .expect("somewhere to forward to");
    let _ = app.update(Message::Forward(crate::forward::Event::Choose(first)));
    let chosen = &app.client.state().forwarding.as_ref().expect("open").chosen;
    assert_eq!(chosen.len(), 1, "the place is ticked");
    window_key(&mut app, keyboard::Key::Named(key::Named::Escape));
    assert!(app.client.state().forwarding.is_none(), "Escape closes it");
}

#[test]
fn alt_up_steps_into_the_messages_and_r_replies_to_the_one_stepped_on() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        chord(ui, key::Named::ArrowUp, keyboard::Modifiers::ALT);
    });
    let newest = app.messages.last().expect("messages").item.clone();
    assert_eq!(
        app.keyed.as_ref(),
        Some(&newest),
        "the newest is stepped on"
    );
    assert_eq!(app.pointed_message.as_ref(), Some(&newest), "and lit");
    window_key(&mut app, keyboard::Key::Named(key::Named::ArrowUp));
    let stepped = app.keyed.clone().expect("still stepping");
    assert_ne!(stepped, newest, "Up steps to an earlier message");
    let author = app
        .client
        .state()
        .message(&stepped)
        .expect("the message")
        .author
        .clone();
    window_key(&mut app, keyboard::Key::Character("r".into()));
    match app.client.state().compose() {
        Some(Compose::Reply { author: to, .. }) => assert_eq!(*to, author),
        _ => panic!("R didn't reply"),
    }
    assert!(app.keyed.is_none(), "replying goes back to the composer");
}
