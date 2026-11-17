//! The composer's keys and who is typing: task C14's tests of what the field
//! maps and what the line over the frame draws.

use iced::Point;
use iced_test::simulator;

use anomi_theme::Theme;
use anomi_vm::chat::Intent as Chat;
use anomi_vm::chat::composer::{Composer, Expression, Intent, Suggest};
use anomi_vm::fixtures::chat::composer as sample;

use super::{BOTTOM, WINDOW, frame, ui};
use crate::support::asked;

fn press(key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> iced::Event {
    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers,
        repeat: false,
        text: None,
    })
}

/// What the field reports to `key` pressed with `modifiers` after a click
/// into it.
fn keyed(
    composer: &Composer,
    key: iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Vec<Chat> {
    let mut ui = ui(composer);
    ui.point_at(Point::new(300.0, BOTTOM - 30.0));
    ui.simulate(simulator::click());
    ui.simulate([press(key, modifiers)]);
    asked(ui)
}

fn letter(text: &str) -> iced::keyboard::Key {
    iced::keyboard::Key::Character(text.into())
}

fn named(key: iced::keyboard::key::Named) -> iced::keyboard::Key {
    iced::keyboard::Key::Named(key)
}

#[test]
fn control_g_and_e_open_the_gif_and_emoji_pickers() {
    use iced::keyboard::Modifiers;
    let dm = sample::dm();
    assert!(
        keyed(&dm, letter("g"), Modifiers::CTRL)
            .contains(&Chat::Composer(Intent::Expressions(Expression::Gifs)))
    );
    assert!(
        keyed(&dm, letter("e"), Modifiers::CTRL)
            .contains(&Chat::Composer(Intent::Expressions(Expression::Emoji)))
    );
}

#[test]
fn control_v_asks_for_the_clipboard() {
    let sent = keyed(&sample::dm(), letter("v"), iced::keyboard::Modifiers::CTRL);
    assert!(sent.contains(&Chat::Composer(Intent::Paste)));
}

#[test]
fn up_in_the_empty_field_edits_the_last_message_and_alt_up_browses() {
    use iced::keyboard::{Modifiers, key::Named};
    let dm = sample::dm();
    assert!(dm.draft.is_empty());
    assert!(
        keyed(&dm, named(Named::ArrowUp), Modifiers::empty())
            .contains(&Chat::Composer(Intent::EditLast))
    );
    assert!(
        keyed(&dm, named(Named::ArrowUp), Modifiers::ALT)
            .contains(&Chat::Composer(Intent::BrowseMessages))
    );
}

#[test]
fn escape_over_suggestions_dismisses_them() {
    let mut writing = sample::dm();
    writing.suggesting = true;
    let sent = keyed(
        &writing,
        named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::empty(),
    );
    assert!(sent.contains(&Chat::Composer(Intent::Suggest(Suggest::Dismiss))));
}

#[test]
fn a_moved_caret_is_reported_with_its_room() {
    let composer = sample::dm();
    let mut ui = ui(&composer);
    ui.point_at(Point::new(300.0, BOTTOM - 30.0));
    ui.simulate(simulator::click());
    ui.typewrite("hi");
    ui.tap_key(named(iced::keyboard::key::Named::ArrowLeft));
    let sent = asked(ui);
    assert!(
        sent.iter()
            .any(|intent| matches!(intent, Chat::Composer(Intent::Caret { room: 1, .. }))),
        "sent {sent:?}"
    );
}

/// The frame's pixels at the window's size, as RGBA at twice the scale.
fn pixels(composer: &Composer) -> Vec<u8> {
    let mut ui = ui(composer);
    let shot = ui.snapshot(&Theme::default()).unwrap();
    let dir = std::env::temp_dir().join(format!("anomi-c14-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shot.matches_image(dir.join("frame.png")).unwrap());
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(file).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes.truncate(info.buffer_size());
    let _ = std::fs::remove_dir_all(&dir);
    bytes
}

#[test]
fn who_is_typing_is_drawn_over_the_twenty_pixels_above_the_frame() {
    let quiet = sample::typing();
    let quiet = Composer {
        typing: String::new(),
        ..quiet
    };
    let typing = sample::typing();
    let (without, with) = (pixels(&quiet), pixels(&typing));
    assert_eq!(without.len(), with.len());
    let width = (WINDOW.width * 2.0) as usize;
    let (mut dots, mut words, mut elsewhere) = (0, 0, 0);
    for (index, (a, b)) in without.chunks(4).zip(with.chunks(4)).enumerate() {
        if a == b {
            continue;
        }
        let y = (index / width) as f32 / 2.0;
        if !(200.0..220.0).contains(&y) {
            elsewhere += 1;
            continue;
        }
        match (index % width) as f32 / 2.0 {
            x if (20.0..52.0).contains(&x) => dots += 1,
            x if x >= 68.0 => words += 1,
            _ => elsewhere += 1,
        }
    }
    assert!(dots > 0, "no dots");
    assert!(words > 0, "no text");
    assert_eq!(elsewhere, 0);
}

#[test]
fn the_field_keeps_the_keys_while_someone_starts_and_stops_typing() {
    let start = sample::dm();
    let mut ui = ui(&start);
    ui.point_at(Point::new(300.0, BOTTOM - 30.0));
    ui.simulate(simulator::click());
    ui.typewrite("hi");
    let busy = Composer {
        typing: "Bob is typing...".into(),
        draft: "hi".into(),
        ..sample::dm()
    };
    let mut ui = ui.rebuild(frame(&busy));
    ui.typewrite("!");
    let quiet = Composer {
        draft: "hi!".into(),
        ..sample::dm()
    };
    let mut ui = ui.rebuild(frame(&quiet));
    ui.typewrite("?");
    let sent = asked(ui);
    assert!(
        !sent.contains(&Chat::Composer(Intent::Focus(false))),
        "sent {sent:?}"
    );
    let last = sent.iter().rev().find_map(|intent| match intent {
        Chat::Composer(Intent::Edited { text, .. }) => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(last, Some("hi!?"), "sent {sent:?}");
}
