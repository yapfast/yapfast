//! The status dialog and menu: task P10's tests, driving
//! `anomi_people::status::view` and `status::menu::view` over their fixtures
//! in the simulator (`super::support`).

use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced::{Point, Rectangle, Size};
use iced_test::Simulator;

use anomi_people::status::{CANCEL, CLEAR, SAVE, menu, text_id, view};
use anomi_vm::account::Choice;
use anomi_vm::fixtures::people::status as fixtures;
use anomi_vm::menu::Spot;
use anomi_vm::people::status::{ClearAfter, Intent, Menu, Status};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(600.0, 500.0);

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(vm: &Status) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

fn simulate_menu(vm: &Menu) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, menu::view(vm, front()))
}

/// Focus a button and press it with Enter.
fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

fn click_at(ui: &mut Ui<'_>, x: f32, y: f32) {
    ui.point_at(Point::new(x, y));
    ui.simulate(iced_test::simulator::click());
}

/// Where the emoji's well lies: the card's 20 px padding, the 28 px title
/// and the 7 px under it, then the well below the badge's 5 px.
const WELL: Rectangle = Rectangle::new(Point::new(20.0, 60.0), Size::new(40.0, 40.0));

fn saved(status: &Status) -> Intent {
    status.save()
}

#[test]
fn save_carries_the_draft() {
    let vm = fixtures::timed();
    let mut ui = simulate(&vm);
    press(&mut ui, SAVE);
    assert_eq!(
        asked(ui),
        [Intent::Save {
            emoji: Some("talk:1".into()),
            text: "Testing Yapfast".into(),
            clear_after: ClearAfter::AnHour,
        }]
    );
}

#[test]
fn enter_in_the_field_saves_too() {
    let vm = fixtures::set();
    let mut ui = simulate(&vm);
    ui.click(text_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(asked(ui), [saved(&vm)]);
}

#[test]
fn nothing_saves_while_it_saves() {
    let vm = fixtures::saving();
    let mut ui = simulate(&vm);
    press(&mut ui, SAVE);
    press(&mut ui, CLEAR);
    ui.click(text_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(asked(ui).is_empty());
}

#[test]
fn typing_asks_to_keep_the_words() {
    let vm = fixtures::empty();
    let mut ui = simulate(&vm);
    ui.click(text_id()).expect("the field");
    let _ = ui.typewrite("hi");
    assert_eq!(asked(ui).last(), Some(&Intent::Text("hi".into())));
}

#[test]
fn each_clear_after_option_asks_for_itself() {
    let vm = fixtures::set();
    for (label, after) in [
        ("Never", ClearAfter::Never),
        ("30 min", ClearAfter::HalfAnHour),
        ("1 hour", ClearAfter::AnHour),
        ("4 hours", ClearAfter::FourHours),
        ("Today", ClearAfter::Today),
    ] {
        let mut ui = simulate(&vm);
        ui.click(label).expect("the option");
        assert_eq!(asked(ui), [Intent::ClearAfter(after)], "{label}");
    }
}

#[test]
fn the_well_opens_the_picker_beside_itself() {
    for vm in [fixtures::set(), fixtures::empty()] {
        let mut ui = simulate(&vm);
        click_at(&mut ui, 30.0, 90.0);
        assert_eq!(asked(ui), [Intent::PickEmoji(Spot::beside(WELL))]);
    }
}

#[test]
fn the_badge_takes_the_emoji_off() {
    let vm = fixtures::set();
    let mut ui = simulate(&vm);
    // The badge lies on the well's top right corner, 7 px past its edge.
    click_at(&mut ui, 56.0, 64.0);
    assert_eq!(asked(ui), [Intent::RemoveEmoji]);
}

#[test]
fn with_no_emoji_there_is_no_badge() {
    let vm = fixtures::empty();
    let mut ui = simulate(&vm);
    click_at(&mut ui, 56.0, 64.0);
    assert_eq!(asked(ui), [Intent::PickEmoji(Spot::beside(WELL))]);
}

#[test]
fn clear_status_shows_only_for_a_status_that_is_set() {
    let set = fixtures::set();
    let mut ui = simulate(&set);
    press(&mut ui, CLEAR);
    assert_eq!(asked(ui), [Intent::Clear]);
    // With none set there is no button to focus: Enter reaches nothing.
    let empty = fixtures::empty();
    let mut ui = simulate(&empty);
    press(&mut ui, CLEAR);
    assert!(asked(ui).is_empty());
}

#[test]
fn cancel_and_the_x_close() {
    let vm = fixtures::set();
    let mut ui = simulate(&vm);
    press(&mut ui, CANCEL);
    assert_eq!(asked(ui), [Intent::Close]);
    let mut ui = simulate(&vm);
    // The X: 28 px wide at the card's right padding, in the title's row.
    click_at(&mut ui, 460.0 - 20.0 - 14.0, 20.0 + 14.0);
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn what_went_wrong_shows() {
    let failed = fixtures::failed();
    assert!(simulate(&failed).find("Custom status is too long.").is_ok());
    let set = fixtures::set();
    assert!(simulate(&set).find("Custom status is too long.").is_err());
}

#[test]
fn the_menu_chooses_a_status() {
    let vm = fixtures::menu();
    for (label, choice) in [
        ("Online", Choice::Online),
        ("Idle", Choice::Idle),
        ("Do Not Disturb", Choice::DoNotDisturb),
        ("Invisible", Choice::Invisible),
    ] {
        let mut ui = simulate_menu(&vm);
        ui.click(label).expect("the status");
        assert_eq!(asked(ui), [Intent::Choose(choice)], "{label}");
    }
}

#[test]
fn the_menu_explains_two_of_them() {
    let vm = fixtures::menu();
    let mut ui = simulate_menu(&vm);
    assert!(ui.find("You won't get message notifications.").is_ok());
    assert!(
        ui.find("You'll show as offline, but can use Yapfast as usual.")
            .is_ok()
    );
}

#[test]
fn the_menu_names_its_way_to_the_custom_status_by_what_is_set() {
    let with = fixtures::menu();
    let mut ui = simulate_menu(&with);
    ui.click("Change custom status").expect("the row");
    assert_eq!(asked(ui), [Intent::CustomStatus]);
    let without = Menu {
        rows: fixtures::rows(false),
        ..fixtures::menu()
    };
    let mut ui = simulate_menu(&without);
    ui.click("Set a custom status").expect("the row");
    assert_eq!(asked(ui), [Intent::CustomStatus]);
}

#[test]
fn the_menu_opens_the_profile_editor() {
    let vm = fixtures::menu();
    let mut ui = simulate_menu(&vm);
    ui.click("Edit Profile").expect("the row");
    assert_eq!(asked(ui), [Intent::EditProfile]);
}

#[test]
fn the_menu_tells_the_game() {
    let vm = fixtures::menu();
    let mut ui = simulate_menu(&vm);
    assert!(ui.find("Factorio").is_ok());
    assert!(ui.find("Playing for 47 minutes").is_ok());
    let started = Menu {
        game: fixtures::menu_just_started().game,
        ..fixtures::menu()
    };
    let mut ui = simulate_menu(&started);
    assert!(ui.find("Playing now").is_ok());
    let none = Menu::default();
    assert!(simulate_menu(&none).find("Factorio").is_err());
}
