//! Writing: breaking lines, the reply's bar and Escape, editing in place,
//! `:name` suggestions, the emoji picker, the message's menu leading to
//! Forward, and the keyboard's steps through messages.

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
    assert_eq!(
        app.composer.draft.text(),
        "on my way",
        "and keeps the draft"
    );
}

#[test]
fn up_in_an_empty_composer_edits_ones_last_message_in_its_row() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::ArrowUp));
    });
    let item = app.composer.editing.clone().expect("an edit started");
    let own = app.client.state().message(&item).expect("the message");
    assert!(own.own, "the last of this account's own");
    let before = own.body.clone();
    assert_eq!(
        app.composer.edit.text(),
        before,
        "the edit starts with its text"
    );
    // Escape cancels it, and the text stays.
    interact(&mut app, |ui| {
        ui.click(Id::new(EDIT)).expect("the edit's field");
        ui.tap_key(keyboard::Key::Named(key::Named::Escape));
    });
    assert!(app.composer.editing.is_none());
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
    assert!(app.composer.editing.is_none(), "Enter saves and ends it");
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
    assert!(
        !app.composer.suggest.emoji.is_empty(),
        "emoji are suggested"
    );
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(
        app.composer.draft.text().starts_with("nice 👍"),
        "the emoji took the word's place: {:?}",
        app.composer.draft.text()
    );
    assert!(app.composer.suggest.emoji.is_empty(), "and the list closed");
}

#[test]
fn up_on_the_first_suggestion_goes_round_to_the_last() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("nice :smil");
    });
    let last = app
        .composer
        .suggest
        .emoji
        .last()
        .expect("emoji are suggested")
        .clone();
    assert!(app.composer.suggest.emoji.len() > 1, "more than one");
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.tap_key(keyboard::Key::Named(key::Named::ArrowUp));
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(
        app.composer.draft.text().starts_with(&format!(
            "nice {}",
            yapfast_core::emoji::composer_text(&last.key)
        )),
        "the last took the word's place: {:?}",
        app.composer.draft.text()
    );
}

#[test]
fn the_emoji_pickers_search_puts_its_first_match_in_the_composer() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    assert!(app.win.picker.is_open());
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the picker's search");
        ui.typewrite("grinning face");
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the picker's search");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(app.win.picker.get().is_none(), "picking closes the picker");
    assert!(
        app.composer.draft.text().contains('😀'),
        "{:?}",
        app.composer.draft.text()
    );
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
    let newest = app.timeline.messages.last().expect("messages").item.clone();
    assert_eq!(
        app.keyed.as_ref(),
        Some(&newest),
        "the newest is stepped on"
    );
    assert_eq!(
        app.pointed.message.as_deref(),
        Some(newest.as_str()),
        "and lit"
    );
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
    let _ = app.open_settings(crate::settings::Section::Account);
    window_key(&mut app, letter("x"));
    let _ = app.close_settings();
    app.call.popup = Some(crate::call::Popup::Link);
    window_key(&mut app, letter("x"));
    app.call.popup = None;
    // Nor with Ctrl held, nor keys that write nothing, nor Enter.
    window_chord(&mut app, letter("x"), keyboard::Modifiers::CTRL);
    window_key(&mut app, keyboard::Key::Named(key::Named::ArrowDown));
    window_key(&mut app, keyboard::Key::Named(key::Named::Enter));
    assert_eq!(app.composer.draft.text(), "Hello");
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
    assert_eq!(app.composer.draft.text(), "Hello Y");
    assert_eq!(app.client.state().draft(), "Hello Y");
    assert_eq!(app.composer.draft.caret(), "Hello Y".len());
}

#[test]
fn ctrl_v_with_nothing_focused_pastes_in_the_composer() {
    let (mut app, _folder) = preview();
    // The tasks don't run here: work for them means the clipboard is read
    // for the composer, and the composer takes the keys.
    let paste = |app: &mut crate::app::App| {
        app.update(Message::Key(
            keyboard::Key::Character("v".into()),
            keyboard::Modifiers::CTRL,
        ))
        .units()
    };
    app.call.popup = Some(crate::call::Popup::Link);
    assert_eq!(paste(&mut app), 0, "not under a popup, as typing isn't");
    app.call.popup = None;
    assert!(paste(&mut app) > 0);

    // Stepping through the messages, it leaves them, as Discord's paste
    // with a message focused goes to the composer.
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        chord(ui, key::Named::ArrowUp, keyboard::Modifiers::ALT);
    });
    assert!(app.keyed.is_some());
    assert!(paste(&mut app) > 0);
    assert!(app.keyed.is_none());
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
    assert_eq!(app.composer.draft.text(), "");
    window_key(&mut app, keyboard::Key::Character("x".into()));
    assert!(app.keyed.is_none(), "a letter leaves");
    assert_eq!(app.composer.draft.text(), "x");
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
        app.timeline
            .revealed
            .contains(&crate::message::fold_key("long")),
        "unfolded"
    );
    // Laid out again at its whole height, as the rows measure it.
    interact(&mut app, |_| {});
    assert!(shows(&app, "Show less"));
    assert!(shows(&app, "the end"));
}

#[test]
fn an_at_words_suggestions_follow_the_members_as_they_change() {
    let (mut app, _folder) = preview();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("hi @Zo");
    });
    assert!(!shows(&app, "Zoe"), "no Zoe yet");
    // Zoe joins while the word is open.
    super::change(&mut app, |state| {
        for members in state.members.iter_mut().flatten() {
            let members = std::sync::Arc::make_mut(members);
            let mut zoe = members[0].clone();
            zoe.id = "zoe".into();
            zoe.name = "Zoe".into();
            zoe.username = Some("zoe".into());
            members.push(zoe);
        }
    });
    assert!(crate::suggest::suggesting(
        app.ctx(),
        crate::composer::Writing::Draft
    ));
    assert!(shows(&app, "Zoe"), "suggested once she's a member");
}

#[test]
fn push_to_talks_own_key_types_nothing_in_a_call_and_the_others_do() {
    use yapfast_core::application::Action;
    let (mut app, _folder) = preview();
    // In the preview's call, a channel open to write in.
    for room in ["lounge", "general"] {
        app.client.dispatch(Action::SelectRoom(room.into()));
        let _ = app.sync();
    }
    assert!(app.client.state().voice.active(), "the preview joins");
    super::change(&mut app, |state| state.voice.muted = false);
    let v = key::Physical::Code(key::Code::KeyV);
    app.layout.voice.input_mode = crate::layout::InputMode::PushToTalk;
    app.layout.voice.push_to_talk_key =
        yapfast_core::keybinds::Shortcut::new(vec![yapfast_core::keybinds::Key {
            scancode: crate::keybinds::scancode(v).expect("V's scan code"),
            name: "V".into(),
            trigger: None,
        }]);
    let _ = crate::keybinds::watch_keybinds(&mut app);
    let press = |app: &mut crate::app::App, text: &str, code: key::Code| {
        let _ = app.update(Message::Pressed(
            keyboard::Key::Character(text.into()),
            keyboard::Modifiers::default(),
            key::Physical::Code(code),
        ));
    };
    // Other keys type while it's held; its own press doesn't.
    press(&mut app, "h", key::Code::KeyH);
    press(&mut app, "v", key::Code::KeyV);
    press(&mut app, "i", key::Code::KeyI);
    assert_eq!(app.composer.draft.text(), "hi");
    // Muted, it sends nothing, so it types.
    super::change(&mut app, |state| state.voice.muted = true);
    press(&mut app, "v", key::Code::KeyV);
    assert_eq!(app.composer.draft.text(), "hiv");
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
    assert!(
        app.composer.draft.is_empty(),
        "the core empties the draft once sent"
    );
    assert!(shows(&app, "hello there"));
}

/// The window on #general of a Discord server whose app, Status Bot, has
/// `ping` (no options), `echo` (a text option) and `extra` more commands
/// named `cmd0`, `cmd1`…, with `/` typed in the composer.
fn with_commands(extra: usize) -> (crate::app::App, tempfile::TempDir) {
    use std::sync::Arc;
    use yapfast_core::{
        application::{Action, Navigation},
        chat::{CommandIndex, Event},
        yapfast_discord::commands::USE_APPLICATION_COMMANDS,
    };
    let (mut app, folder) = super::server::discord_server(USE_APPLICATION_COMMANDS, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.client
        .dispatch(Action::SelectRoom("discord:10:11".into()));
    app.client.apply(Event::Timeline {
        room: "discord:10:11".into(),
        messages: Vec::new(),
    });
    let _ = app.sync();
    let mut commands = vec![
        serde_json::json!({"id": "501", "application_id": "500", "version": "1", "type": 1,
                           "name": "ping", "description": "Pong", "default_member_permissions": null}),
        serde_json::json!({"id": "502", "application_id": "500", "version": "1", "type": 1,
                           "name": "echo", "description": "Says it back", "default_member_permissions": null,
                           "options": [{"type": 3, "name": "text", "description": "What to say", "required": true}]}),
    ];
    for at in 0..extra {
        commands.push(
            serde_json::json!({"id": (600 + at).to_string(), "application_id": "500",
            "version": "1", "type": 1, "name": format!("cmd{at}"), "description": "More",
            "default_member_permissions": null}),
        );
    }
    let index = serde_json::json!({
        "version": "1",
        "applications": [{"id": "500", "name": "Status Bot", "description": "", "icon": null, "flags": "0"}],
        "application_commands": commands,
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("/");
    });
    app.client.apply(Event::CommandIndex {
        room: "discord:10:11".into(),
        index: Ok(Arc::new(CommandIndex::parse(&index, false))),
    });
    let _ = app.sync();
    (app, folder)
}

#[test]
fn escape_stops_a_command_once_its_list_is_gone() {
    let (mut app, _folder) = with_commands(0);
    assert!(shows(&app, "/ping"), "the list offers the commands");
    interact(&mut app, |ui| {
        ui.click("/ping").expect("ping in the list");
    });
    assert!(
        app.client.state().command_draft().is_some(),
        "ping is written"
    );
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.tap_key(keyboard::Key::Named(key::Named::Escape));
    });
    assert!(
        app.client.state().command_draft().is_none(),
        "Escape stops writing a command with no options"
    );
    assert!(app.composer.draft.is_empty(), "and empties the composer");
    assert!(!shows(&app, "/ping"));
}

#[test]
fn the_command_list_scrolls_past_its_eight_rows() {
    use iced::{Settings, mouse};
    let (mut app, _folder) = with_commands(20);
    assert!(shows(&app, "/ping"));
    assert!(!shows(&app, "/cmd10"), "eight rows show at once");
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    let first = ui
        .find("/ping")
        .expect("the list")
        .visible_bounds()
        .expect("in view");
    ui.point_at(first.center());
    ui.simulate(crate::widgets::swipe(-10.0 * 44.0));
    let scrolled: Vec<f32> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::Write(crate::composer::Event::CommandsScrolled(down)) => Some(down),
            _ => None,
        })
        .collect();
    assert_eq!(scrolled.last(), Some(&440.0), "{scrolled:?}");
    let _ = app.update(Message::Write(crate::composer::Event::CommandsScrolled(
        440.0,
    )));
    // The rows down there are built, and a click on one picks it.
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    ui.point_at(first.center());
    ui.simulate(crate::widgets::swipe(-10.0 * 44.0));
    let row = ui
        .find("/cmd10")
        .expect("a row scrolled to")
        .visible_bounds()
        .expect("in view");
    ui.point_at(row.center());
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
    for message in ui.into_messages() {
        let _ = app.update(message);
    }
    assert_eq!(
        app.client
            .state()
            .command_draft()
            .map(|command| command.name.as_str()),
        Some("cmd10")
    );
}

#[test]
fn the_timed_out_line_follows_the_twelve_hour_clock() {
    use std::sync::Arc;
    let (mut app, _folder) = with_commands(0);
    app.layout.prefs.twelve_hour_clock = true;
    let at = chrono::Local::now()
        .date_naive()
        .and_hms_opt(14, 30, 0)
        .and_then(|at| at.and_local_timezone(chrono::Local).single())
        .expect("half past two today")
        .timestamp() as u64;
    super::change(&mut app, |state| {
        let room = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "discord:10:11")
            .expect("#general");
        Arc::make_mut(room).timed_out_until = Some(at * 1000);
    });
    assert!(shows(
        &app,
        "You're timed out in this server until 2:30 PM."
    ));
}

/// With Matrix, the line under the composer says whether the room is
/// end-to-end encrypted, and why an unencrypted one can't be written in.
#[cfg(feature = "matrix")]
#[test]
fn a_matrix_rooms_composer_says_whether_it_is_encrypted() {
    use std::sync::Arc;
    use yapfast_core::network::Network;
    let (mut app, _folder) = preview();
    let matrix = |app: &mut crate::app::App, encrypted: bool| {
        super::change(app, |state| {
            // Signed in, out of the preview, on a verified device.
            state.preview = false;
            state.account = Some("@me:example.org".into());
            state.trusted = true;
            let selected = state.selected.clone();
            let room = state
                .rooms
                .iter_mut()
                .find(|room| Some(&room.id) == selected.as_ref())
                .expect("a room open");
            let room = Arc::make_mut(room);
            room.network = Network::Matrix;
            room.encrypted = encrypted;
        });
    };
    matrix(&mut app, true);
    assert!(shows(
        &app,
        "End-to-end encrypted. Enter to send, Shift+Enter for a new line."
    ));
    matrix(&mut app, false);
    assert!(shows(&app, "Unencrypted room. Reading only in this build."));
}

#[test]
fn a_waiting_file_without_a_preview_shows_its_spoiler_cover() {
    use yapfast_core::application::Action;
    let (mut app, folder) = with_commands(0);
    let notes = folder.path().join("notes.txt");
    std::fs::write(&notes, "plans").expect("a file");
    app.client.dispatch(Action::Stage(vec![notes]));
    let _ = app.sync();
    assert!(shows(&app, "notes.txt"), "the file waits");
    assert!(!shows(&app, "SPOILER"));
    let _ = app.update(Message::Write(crate::composer::Event::Uploads(
        crate::uploads::Event::Spoiler(0),
    )));
    assert!(shows(&app, "SPOILER"), "its icon is covered");
}

#[test]
fn the_typing_lines_words_breathe_while_animations_play_and_hold_still_otherwise() {
    use iced::{Settings, window};
    use std::time::{Duration, Instant};
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        let room = state.selected.clone().expect("a room open");
        state.typing.insert(room, vec!["Sam".into()]);
    });
    assert!(shows(&app, "Sam is typing..."));
    // How bright the words are drawn `into` their breathing: the sum of
    // each pixel's brightest channel over them.
    let brightness = |app: &crate::app::App, into: Duration| -> u64 {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        let words = ui
            .find("Sam is typing...")
            .expect("the line")
            .visible_bounds()
            .expect("in view");
        ui.simulate([iced::Event::Window(window::Event::RedrawRequested(
            Instant::now() - into,
        ))]);
        let (width, _, rgba) = super::drawn(&mut ui);
        let scale = 2.0;
        let rows = (words.y * scale) as usize..((words.y + words.height) * scale) as usize;
        let columns = (words.x * scale) as usize..((words.x + words.width) * scale) as usize;
        rows.flat_map(|y| columns.clone().map(move |x| (y * width as usize + x) * 4))
            .map(|at| u64::from(rgba[at..at + 3].iter().copied().max().unwrap_or(0)))
            .sum()
    };
    // A quarter of a breath in, they're at their brightest; three
    // quarters in, at half of it.
    let (full, dim) = (
        brightness(&app, Duration::from_millis(360)),
        brightness(&app, Duration::from_millis(1080)),
    );
    assert!(dim * 10 < full * 9, "dimmer: {dim} against {full}");
    app.layout.prefs.animate = crate::layout::Animate::Never;
    let (full, dim) = (
        brightness(&app, Duration::from_millis(360)),
        brightness(&app, Duration::from_millis(1080)),
    );
    assert_eq!(dim, full, "held still");
}
