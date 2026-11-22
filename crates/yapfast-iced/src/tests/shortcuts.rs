use super::{preview, start};
use crate::{
    app::Message,
    settings::{self, Section},
    shortcuts::{self, Shortcut},
};

#[test]
fn built_in_shortcuts_show_the_list_and_respect_dialogs_and_recording() {
    let (mut app, _folder) = preview();
    let _ = shortcuts::update(&mut app, Shortcut::Inbox);
    assert!(app.inbox.is_some());
    let _ = shortcuts::update(&mut app, Shortcut::Inbox);
    assert!(app.inbox.is_none());
    let _ = shortcuts::update(&mut app, Shortcut::Help);
    assert_eq!(app.win.settings_section, Section::Keybinds);
    assert!(app.win.settings.get().is_some());
    let _ = shortcuts::update(&mut app, Shortcut::Inbox);
    assert!(app.inbox.is_none(), "settings keep chat shortcuts out");
    let deafened = app.layout.deafen_on_join;
    let _ = shortcuts::update(&mut app, Shortcut::Deafen);
    assert_eq!(
        app.layout.deafen_on_join, !deafened,
        "voice works in settings"
    );
    let _ = app.update(Message::Settings(settings::Event::Record(
        settings::keybinds::Target::Talk,
        true,
    )));
    let _ = shortcuts::update(&mut app, Shortcut::Deafen);
    assert_eq!(
        app.layout.deafen_on_join, !deafened,
        "recording suppresses built-ins"
    );
}

#[test]
fn built_in_shortcuts_do_nothing_before_signin() {
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    let mut app = start(&folder, false);
    let _ = shortcuts::update(&mut app, Shortcut::Help);
    assert!(app.win.settings.get().is_none());
}

#[test]
fn configured_keybinds_take_priority_over_built_ins() {
    use iced::keyboard::{
        Key, Location, Modifiers,
        key::{Code, Named, Physical},
    };
    use yapfast_core::keybinds::{Action, Shortcut as Keys, Watched};
    let (mut app, _folder) = preview();
    let record = |key, code, location| {
        settings::keybinds::recorded(&key, Physical::Code(code), location).expect("recorded key")
    };
    let keys = Keys::new(vec![
        record(
            Key::Named(Named::Control),
            Code::ControlLeft,
            Location::Left,
        ),
        record(Key::Named(Named::Shift), Code::ShiftLeft, Location::Left),
        record(Key::Character("m".into()), Code::KeyM, Location::Standard),
    ])
    .expect("chord");
    app.keybinds.watched = vec![Watched {
        id: "test".into(),
        action: Action::ToggleMute,
        keys,
    }];
    let physical = Physical::Code(Code::KeyM);
    let chord = Modifiers::CTRL | Modifiers::SHIFT;
    assert!(shortcuts::custom_owns(&app, physical, chord));
    assert!(!shortcuts::custom_owns(&app, physical, Modifiers::CTRL));
    assert!(!shortcuts::custom_owns(
        &app,
        Physical::Code(Code::KeyD),
        chord
    ));
    app.keybinds.status = Some(yapfast_core::keybinds::Status {
        reach: yapfast_core::keybinds::Reach::Desktop,
        given: Default::default(),
    });
    assert!(
        !shortcuts::custom_owns(&app, physical, chord),
        "an unapproved portal shortcut cannot suppress a built-in"
    );
}

#[test]
fn a_built_in_chord_does_not_also_act_on_the_focused_message() {
    use iced::keyboard::{
        Key, Modifiers,
        key::{Code, Named, Physical},
    };
    let (mut app, _folder) = preview();
    app.keyed = Some(
        app.timeline
            .messages
            .iter()
            .find(|message| message.own && message.can_edit)
            .expect("own message")
            .item
            .clone(),
    );
    let keyed = app.keyed.clone();
    let _ = app.update(Message::Pressed(
        Key::Character("e".into()),
        Modifiers::CTRL,
        Physical::Code(Code::KeyE),
    ));
    assert!(
        app.client.state().edit().is_none(),
        "Ctrl+E must not also edit"
    );
    let _ = app.update(Message::Pressed(
        Key::Named(Named::Enter),
        Modifiers::CTRL,
        Physical::Code(Code::Enter),
    ));
    assert!(
        app.win.menu.is_none(),
        "Ctrl+Enter must not open the message menu"
    );
    let _ = app.update(Message::Pressed(
        Key::Named(Named::ArrowUp),
        Modifiers::CTRL | Modifiers::ALT,
        Physical::Code(Code::ArrowUp),
    ));
    assert_eq!(
        app.keyed, keyed,
        "server navigation must not step through messages"
    );
    let _ = app.update(Message::Shortcut(
        Shortcut::Emoji,
        Physical::Code(Code::KeyE),
        Modifiers::CTRL,
    ));
    assert_eq!(
        app.win.picker.get().map(|picker| picker.tab),
        Some(crate::picker::Tab::Emoji)
    );
}

#[test]
fn recording_does_not_type_into_or_navigate_messages() {
    use iced::keyboard::{
        Key, Modifiers,
        key::{Code, Physical},
    };
    let (mut app, _folder) = preview();
    let _ = shortcuts::update(&mut app, Shortcut::Help);
    let _ = app.update(Message::Settings(settings::Event::Record(
        settings::keybinds::Target::Talk,
        true,
    )));
    let draft = app.composer.draft.text();
    let _ = app.update(Message::Pressed(
        Key::Character("x".into()),
        Modifiers::default(),
        Physical::Code(Code::KeyX),
    ));
    assert_eq!(app.composer.draft.text(), draft);
    assert!(app.recording_keybind());
}
