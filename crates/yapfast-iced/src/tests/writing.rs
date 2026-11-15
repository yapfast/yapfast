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
    assert_eq!(app.pointed.message.as_ref(), Some(&newest), "and lit");
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

/// A key nothing took with `modifiers` held, as the window hands it on.
fn window_chord(app: &mut crate::app::App, key: keyboard::Key, modifiers: keyboard::Modifiers) {
    let _ = app.update(Message::Key(key, modifiers));
}

#[test]
fn typing_with_nothing_focused_goes_to_the_composer() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("Hello");
    });
    let letter = |text: &str| keyboard::Key::Character(text.into());

    // Not under the settings or a popup.
    app.settings = Some(Default::default());
    window_key(&mut app, letter("x"));
    app.settings = None;
    app.call.popup = Some(crate::call::Popup::Link);
    window_key(&mut app, letter("x"));
    app.call.popup = None;
    // Nor with Ctrl held, nor keys that write nothing, nor Enter.
    window_chord(&mut app, letter("x"), keyboard::Modifiers::CTRL);
    window_key(&mut app, keyboard::Key::Named(key::Named::ArrowDown));
    window_key(&mut app, keyboard::Key::Named(key::Named::Enter));
    assert_eq!(app.draft.text(), "Hello");
    assert!(
        app.client
            .state()
            .messages
            .iter()
            .all(|m| m.body != "Hello")
    );

    // The text goes on the end, and Backspace edits it there.
    window_key(&mut app, keyboard::Key::Named(key::Named::Space));
    window_key(&mut app, letter("w"));
    window_key(&mut app, keyboard::Key::Named(key::Named::Backspace));
    window_key(&mut app, letter("Y"));
    assert_eq!(app.draft.text(), "Hello Y");
    assert_eq!(app.client.state().draft(), "Hello Y");
    assert_eq!(app.draft.caret(), "Hello Y".len());
}

#[test]
fn stepping_through_the_messages_only_letters_and_digits_leave_them() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        chord(ui, key::Named::ArrowUp, keyboard::Modifiers::ALT);
    });
    assert!(app.keyed.is_some());
    window_key(&mut app, keyboard::Key::Character("?".into()));
    assert!(app.keyed.is_some(), "a sign stays");
    assert_eq!(app.draft.text(), "");
    window_key(&mut app, keyboard::Key::Character("x".into()));
    assert!(app.keyed.is_none(), "a letter leaves");
    assert_eq!(app.draft.text(), "x");
}

#[test]
fn a_voice_messages_volume_opens_over_its_mute_and_the_wheel_turns_it() {
    use crate::playback::Event;
    let (mut app, _folder) = preview();
    let volume = |app: &crate::app::App| app.client.state().voice_tuning.volume;
    assert!((volume(&app) - 1.0).abs() < 0.001);
    let _ = app.voice_event(Event::Turn(-1.0));
    assert!((volume(&app) - 0.95).abs() < 0.001, "{}", volume(&app));
    let _ = app.voice_event(Event::Volume(0.3));
    assert!((volume(&app) - 0.3).abs() < 0.001);
    // The button and the slider over it overlap: it stays open until the
    // pointer has left both.
    let _ = app.voice_event(Event::VolumeHover("v".into(), true));
    let _ = app.voice_event(Event::VolumeHover("v".into(), true));
    let _ = app.voice_event(Event::VolumeHover("v".into(), false));
    assert!(app.voice_volume.is_some());
    let _ = app.voice_event(Event::VolumeHover("v".into(), false));
    assert!(app.voice_volume.is_none());
}

#[test]
fn show_more_under_a_long_message_unfolds_it() {
    let (mut app, _folder) = preview();
    app.layout.collapse_long = true;
    // Quoted and plain lines in turn, each a part of its own.
    let mut lines: Vec<String> = (1..=20)
        .map(|line| {
            let quote = if line % 2 == 1 { "> " } else { "" };
            format!("{quote}line {line}")
        })
        .collect();
    lines.push("> the end".into());
    super::change(&mut app, |state| {
        state.messages =
            std::sync::Arc::new(vec![std::sync::Arc::new(yapfast_core::chat::ChatMessage {
                item: "long".into(),
                author: "Sam".into(),
                body: lines.join("\n"),
                formatted: true,
                timestamp: 1,
                ..Default::default()
            })]);
    });
    assert!(shows(&app, "line 2"));
    assert!(!shows(&app, "the end"), "cut off");
    // The body's cut-off lines lie under Show more: the press is its.
    interact(&mut app, |ui| {
        ui.click("Show more").expect("Show more");
    });
    assert!(
        app.revealed.contains(&crate::message::fold_key("long")),
        "unfolded"
    );
    // Laid out again at its whole height, as the rows measure it.
    interact(&mut app, |_| {});
    assert!(shows(&app, "Show less"));
    assert!(shows(&app, "the end"));
}
