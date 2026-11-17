//! The Friends screen: task P01's tests, driving
//! `anomi_people::friends::view` over its fixtures in the simulator
//! (`super::support`). The screen is laid out alone, 660 by 800: its
//! padding is 16, the search 36 tall, the heading 36, the gaps 8, so the
//! rows are 62 tall from y 104 (the first one's centre is at 135), and each
//! row's Message button is at x 616.

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_people::friends::{tabs, view};
use anomi_theme::Theme;
use anomi_vm::fixtures::people::friends as samples;
use anomi_vm::people::friends::{Friends, Intent};

use super::support::{asked, settings};

const SIZE: Size = Size::new(660.0, 800.0);

fn row(at: usize) -> Point {
    Point::new(300.0, 135.0 + 62.0 * at as f32)
}

fn simulate(friends: &Friends) -> Simulator<'_, Intent, Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(friends, super::support::front()))
}

fn click_at(friends: &Friends, at: Point) -> Vec<Intent> {
    let mut ui = simulate(friends);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn clicking_a_row_asks_to_message_them() {
    let friends = samples::all();
    assert_eq!(
        click_at(&friends, row(2)),
        [Intent::Message("160000000000000000".to_owned() + "0")]
    );
    assert_eq!(
        click_at(&friends, row(0)),
        [Intent::Message("1400000000000000002".into())]
    );
}

#[test]
fn the_message_button_asks_for_the_same() {
    let friends = samples::online();
    assert_eq!(
        click_at(&friends, Point::new(616.0, 197.0)),
        [Intent::Message("1400000000000000003".into())]
    );
}

#[test]
fn clicking_a_name_messages_that_friend() {
    let friends = samples::all();
    let mut ui = simulate(&friends);
    ui.click("Person 4").expect("Person 4's row");
    assert_eq!(
        asked(ui),
        [Intent::Message("160000000000000000".to_owned() + "4")]
    );
}

#[test]
fn a_right_click_asks_for_their_menu_at_the_pointer() {
    let friends = samples::online();
    let mut ui = simulate(&friends);
    ui.point_at(row(1));
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    let asked = asked(ui);
    let [Intent::Menu(id, spot)] = asked.as_slice() else {
        panic!("one menu, got {asked:?}");
    };
    assert_eq!(id, "1400000000000000003");
    assert_eq!(spot.anchor.position(), row(1));
}

#[test]
fn a_voice_channel_alone_is_named_and_goes_there() {
    let friends = samples::lines();
    let mut ui = simulate(&friends);
    ui.click("Lounge \u{2014} Server")
        .expect("Bob's voice channel");
    assert_eq!(asked(ui), [Intent::OpenVoice("discord:v1".into())]);
}

#[test]
fn a_status_with_a_voice_channel_keeps_its_words() {
    let friends = samples::online();
    let mut ui = simulate(&friends);
    assert!(ui.find("Playing Factorio").is_ok(), "Bob's activity");
    assert!(
        ui.find("Lounge \u{2014} Server").is_err(),
        "the place is a speaker"
    );
}

#[test]
fn a_friend_who_says_nothing_shows_how_they_are() {
    let friends = samples::all();
    let mut ui = simulate(&friends);
    assert!(ui.find("Offline").is_ok());
}

#[test]
fn the_heading_counts_the_rows() {
    let online = samples::online();
    assert!(simulate(&online).find("ONLINE \u{2014} 2").is_ok());
    let all = samples::all();
    assert!(simulate(&all).find("ALL FRIENDS \u{2014} 5").is_ok());
}

#[test]
fn typing_in_the_search_asks_to_filter() {
    let friends = samples::online();
    let mut ui = simulate(&friends);
    ui.point_at(Point::new(300.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("bo");
    let asked = asked(ui);
    assert_eq!(
        asked.last(),
        Some(&Intent::Filter("bo".into())),
        "{asked:?}"
    );
}

#[test]
fn nobody_around_says_so() {
    let friends = samples::nobody_around();
    let mut ui = simulate(&friends);
    assert!(ui.find("ONLINE \u{2014} 0").is_ok());
    assert!(
        ui.find("None of your friends are around right now.")
            .is_ok()
    );
}

#[test]
fn a_search_that_matches_nobody_says_so() {
    let friends = samples::no_match();
    let mut ui = simulate(&friends);
    assert!(ui.find("No friend's name matches that.").is_ok());
}

#[test]
fn no_friends_shows_the_welcome_and_its_button() {
    let friends = samples::none();
    let mut ui = simulate(&friends);
    assert!(ui.find("No friends yet").is_ok());
    assert!(ui.find("Search").is_err(), "no search without friends");
    drop(ui);
    // Its button is a drawn face with no text to find, in the middle of the
    // screen under the title (y 464 to 496).
    assert_eq!(
        click_at(&friends, Point::new(330.0, 480.0)),
        [Intent::AddFriend]
    );
    assert!(click_at(&friends, Point::new(330.0, 300.0)).is_empty());
}

#[test]
fn a_tab_presses_when_clicked() {
    let mut ui = Simulator::with_size(
        settings(),
        Size::new(200.0, 60.0),
        tabs::tab("All", false, 7_u8),
    );
    ui.click("All").expect("the tab");
    assert_eq!(asked(ui), [7]);
}
