//! The Active Now column: task P04's tests, driving
//! `anomi_people::active_now::view` over its fixtures in the simulator
//! (`super::support`).

use iced::widget::container;
use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_people::active_now::view;
use anomi_theme::Motion;
use anomi_vm::fixtures::people::active_now::{NOW, around, busy, quiet};
use anomi_vm::people::active_now::{ActiveNow, Intent};

use super::support::{asked, front, settings};

const COLUMN: Size = Size::new(360.0, 860.0);

fn ui(
    active_now: &ActiveNow,
    motion: Motion,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), COLUMN, container(view(active_now, motion)))
}

#[test]
fn a_voice_row_opens_its_room_and_the_others_do_nothing() {
    let column = around();
    let mut ui = ui(&column, front());
    ui.click("Lounge").expect("the voice row");
    ui.click("Marconi Union").expect("the detail row");
    assert_eq!(asked(ui), [Intent::OpenRoom("discord:v1".into())]);
}

#[test]
fn a_press_elsewhere_and_a_release_on_the_voice_row_asks_for_nothing() {
    let column = around();
    let mut ui = ui(&column, front());
    let lounge = ui.find("Lounge").expect("the voice row").visible_bounds();
    let lounge = lounge.expect("shown").center();
    ui.point_at(Point::new(200.0, 20.0));
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    ui.point_at(lounge);
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
    assert!(asked(ui).is_empty());
}

#[test]
fn a_cards_head_opens_its_first_friends_profile_beside_it() {
    let column = around();
    let mut ui = ui(&column, front());
    ui.click("Bob and Carol").expect("the head");
    let sent = asked(ui);
    let [Intent::OpenPerson(person, spot)] = sent.as_slice() else {
        panic!("one OpenPerson, got {sent:?}");
    };
    assert_eq!(person, "1400000000000000002");
    // At the head's bottom left, as Slint's `anchor(x, y + height, 0, 0)`.
    assert_eq!(spot.anchor.size(), Size::ZERO);
    assert_eq!(spot.gap, 0.0);
    // The column's padding 16; 16 + 27 heading + 16 gap + 62 head.
    assert!((spot.anchor.x - 16.0).abs() < 0.5, "{:?}", spot.anchor);
    assert!((spot.anchor.y - 121.0).abs() < 0.5, "{:?}", spot.anchor);
}

#[test]
fn a_card_says_what_they_do_and_since_when() {
    let column = around();
    let mut ui = ui(&column, front());
    ui.find("Listening to Spotify – 1m").expect("a minute in");
    ui.find("00:01:36 elapsed").expect("the row's time");
    ui.find("Active Now").expect("the heading");
    assert!(ui.find("It's quiet for now...").is_err());
}

#[test]
fn the_times_follow_the_clock_the_app_writes() {
    let mut column = busy();
    column.now = NOW + 3 * 60;
    let mut ui = ui(&column, front());
    ui.find("Listening to Spotify – 4m")
        .expect("three minutes on");
    ui.find("00:04:36 elapsed").expect("the row's time too");
    ui.find("Dota 2 – 3h").expect("hours");
    ui.find("Solitaire – 2d").expect("days");
}

#[test]
fn a_game_row_counts_its_people_and_those_beyond_its_faces() {
    let column = busy();
    let mut ui = ui(&column, front());
    ui.find("3 People").expect("the game row");
    ui.find("+2").expect("the faces beyond the three");
}

#[test]
fn with_no_card_the_column_is_quiet() {
    let column = quiet();
    let mut ui = ui(&column, front());
    ui.find("Active Now").expect("the heading");
    ui.find("It's quiet for now...").expect("the quiet message");
    assert!(asked(ui).is_empty());
}

#[test]
fn a_new_view_model_replaces_the_cards() {
    let first = busy();
    let mut ui = ui(&first, front());
    ui.find("Ann, Bob, and 1 other").expect("a party");
    let second = quiet();
    let mut ui = ui.rebuild(container(view(&second, front())));
    assert!(ui.find("Ann, Bob, and 1 other").is_err());
    ui.find("It's quiet for now...").expect("quiet now");
}

/// Every kind of card draws, with and without pictures, in a light and a
/// dark theme; the snapshot is thrown away.
#[test]
fn the_column_draws_in_a_light_and_a_dark_theme() {
    let column = busy();
    for (name, theme) in [
        ("dark", anomi_theme::Theme::default()),
        (
            "light",
            anomi_theme::Theme::built_in(Some("catppuccin-latte")),
        ),
    ] {
        let mut ui = ui(&column, Motion::default());
        let shot = ui.snapshot(&theme).expect("a snapshot");
        let dir =
            std::env::temp_dir().join(format!("anomi-active-now-{name}-{}", std::process::id()));
        shot.matches_image(dir.join("column")).expect("saved");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
