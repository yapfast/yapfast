//! The keys while the timeline has the keyboard: stepping through the
//! messages, and acting on the one stepped to (the Slint test
//! `the_keyboard_steps_through_messages_and_acts_on_them`). A timeline the
//! app already knows has the keyboard (`keys.browsing`) isn't told so again.

use std::time::Instant;

use iced::advanced::widget::operation::focusable;
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::{Event, Point, mouse};
use iced_test::Simulator;

use anomi_chat::timeline::ACTIVE;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::{
    self,
    timeline::{Act, Intent, Keys, May, Timeline},
};
use anomi_vm::fixtures::chat::timeline::{self as fixtures, LAST, message_key};
use anomi_vm::menu::Side;
use anomi_widgets::focus;

use super::{SIZE, asked, frame, rebuild, shown};

type Ui<'a> = Simulator<'a, chat::Intent, Theme, Renderer>;

/// The timeline given the keyboard, as the composer's Alt+Up gives it.
fn browse<'a>(timeline: &'a Timeline) -> Ui<'a> {
    let mut ui = shown(timeline);
    ui.operate(&mut focusable::focus(focus::TIMELINE));
    ui
}

fn press(ui: &mut Ui<'_>, key: impl Into<Key>, modifiers: Modifiers) {
    let key = key.into();
    let _ = ui.simulate([Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })]);
}

fn tap(ui: &mut Ui<'_>, key: impl Into<Key>) {
    press(ui, key, Modifiers::default());
}

fn letter(text: &str) -> Key {
    Key::Character(text.into())
}

/// #general with the keyboard on message `index`.
fn on(index: usize, may: May) -> Timeline {
    Timeline {
        keys: Keys {
            active: Some(message_key(index)),
            browsing: true,
            may,
            confirming: false,
        },
        ..fixtures::general()
    }
}

#[test]
fn up_starts_from_the_newest_and_the_keys_step_within_the_messages() {
    let timeline = fixtures::general();
    let mut ui = browse(&timeline);
    tap(&mut ui, Named::ArrowUp);
    assert_eq!(
        asked(&mut ui),
        [Intent::Browsing(true), Intent::Step(message_key(LAST))]
    );
    let timeline = on(fixtures::FIRST, May::default());
    let mut ui = browse(&timeline);
    tap(&mut ui, Named::ArrowUp);
    tap(&mut ui, Named::ArrowDown);
    tap(&mut ui, Named::PageDown);
    tap(&mut ui, Named::End);
    tap(&mut ui, Named::Home);
    assert_eq!(
        asked(&mut ui),
        [
            Intent::Step(message_key(fixtures::FIRST)),
            Intent::Step(message_key(fixtures::FIRST + 1)),
            Intent::Step(message_key(fixtures::FIRST + 5)),
            Intent::Step(message_key(LAST)),
            Intent::Step(message_key(fixtures::FIRST)),
        ],
        "the first stays the first"
    );
}

#[test]
fn letters_act_on_the_message_stepped_to_where_they_may() {
    let timeline = fixtures::browsing();
    let key = message_key(53);
    let mut ui = browse(&timeline);
    tap(&mut ui, letter("r"));
    tap(&mut ui, letter("C"));
    tap(&mut ui, letter("f"));
    press(&mut ui, Named::Enter, Modifiers::ALT);
    // It may not be edited or deleted: those keys go on.
    tap(&mut ui, letter("e"));
    tap(&mut ui, Named::Delete);
    assert_eq!(
        asked(&mut ui),
        [
            Intent::Act(key, Act::Reply),
            Intent::Act(key, Act::Copy),
            Intent::Act(key, Act::Forward),
            Intent::Act(key, Act::MarkUnread),
        ]
    );
}

#[test]
fn enter_and_plus_open_the_menu_and_the_picker_beside_the_message() {
    let timeline = fixtures::browsing();
    let key = message_key(53);
    let mut ui = browse(&timeline);
    tap(&mut ui, Named::Enter);
    press(&mut ui, letter("+"), Modifiers::SHIFT);
    let asked = asked(&mut ui);
    let [
        Intent::Act(menu, Act::Menu(at)),
        Intent::Act(react, Act::React(picker)),
    ] = asked.as_slice()
    else {
        panic!("the menu and the picker: {asked:?}");
    };
    assert_eq!((*menu, *react), (key, key));
    assert_eq!(at, picker);
    // Message 53 is two rows over the newest, low in the view.
    let message = ui.find(ACTIVE).expect("the keyboard's message").bounds();
    assert!(message.y > SIZE.height / 2.0, "{message:?}");
    // Left of its right edge, 16 px in, from 16 px over its top.
    assert_eq!(at.side, Side::Left);
    assert_eq!(at.anchor.x, message.x + message.width - 16.0);
    assert_eq!(at.anchor.y, message.y - 16.0);
    assert!(at.anchor.y < SIZE.height, "in view: {at:?}");
}

#[test]
fn a_deletion_waiting_takes_enter_and_escape() {
    let mut timeline = on(53, fixtures::browsing().keys.may);
    timeline.keys.confirming = true;
    let key = message_key(53);
    let mut ui = browse(&timeline);
    tap(&mut ui, Named::Enter);
    tap(&mut ui, Named::Escape);
    assert_eq!(
        asked(&mut ui),
        [
            Intent::Act(key, Act::ConfirmDelete),
            Intent::Act(key, Act::CancelDelete),
        ]
    );
}

#[test]
fn escape_goes_back_to_the_composer_and_a_click_elsewhere_lets_go() {
    let timeline = fixtures::browsing();
    let mut ui = browse(&timeline);
    tap(&mut ui, Named::Escape);
    assert_eq!(asked(&mut ui), [Intent::Leave]);
    ui.point_at(Point::new(SIZE.width + 10.0, 10.0));
    let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    assert_eq!(asked(&mut ui), [Intent::Browsing(false)]);
    // Without the keyboard, keys go on.
    tap(&mut ui, Named::ArrowUp);
    assert_eq!(asked(&mut ui), []);
}

#[test]
fn a_message_deleted_or_on_its_way_takes_no_keys_but_the_steps() {
    let timeline = on(53, May::default());
    let mut ui = browse(&timeline);
    tap(&mut ui, letter("r"));
    tap(&mut ui, Named::Enter);
    tap(&mut ui, Named::ArrowDown);
    assert_eq!(asked(&mut ui), [Intent::Step(message_key(54))]);
}

#[test]
fn rebuilt_through_the_skeleton_it_lets_the_keyboard_go() {
    let general = fixtures::general();
    let browsing = fixtures::browsing();
    let reloading = Timeline {
        rows: Vec::new(),
        ready: false,
        ..fixtures::browsing()
    };
    let mut ui = browse(&general);
    tap(&mut ui, Named::ArrowUp);
    assert_eq!(
        asked(&mut ui),
        [Intent::Browsing(true), Intent::Step(message_key(LAST))]
    );
    // The app shows it browsing; then the room reloads, the skeleton
    // standing in for its messages until they come again.
    let ui = rebuild(ui, &browsing);
    let mut ui = rebuild(ui, &reloading);
    frame(&mut ui, Instant::now());
    assert_eq!(asked(&mut ui), []);
    let mut ui = rebuild(ui, &browsing);
    frame(&mut ui, Instant::now());
    assert_eq!(asked(&mut ui), [Intent::Browsing(false)]);
    frame(&mut ui, Instant::now());
    assert_eq!(asked(&mut ui), [], "told once");
}
