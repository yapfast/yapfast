//! The member list: task P05's tests, driving `anomi_people::members::view`
//! over its fixtures in the simulator (`super::support`). The list is laid
//! out alone, 265 by 860 (the panel's width): 8 above the first heading, the
//! heading 42 tall, then the rows 42 tall and 2 apart, so Bob's centre is at
//! y 73 and each next row 44 lower.

use iced::advanced::widget::operation::Text;
use iced::advanced::widget::{Id, Operation};
use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_people::members::view;
use anomi_theme::Theme;
use anomi_vm::fixtures::people::members as samples;
use anomi_vm::menu::{Align, Side, Spot};
use anomi_vm::people::Intent as People;
use anomi_vm::people::members::{Intent, Members};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(265.0, 860.0);

fn row(at: usize) -> Point {
    Point::new(120.0, 73.0 + 44.0 * at as f32)
}

fn simulate(members: &Members) -> Simulator<'_, People, Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(members, front()))
}

fn click_at(members: &Members, at: Point) -> Vec<People> {
    let mut ui = simulate(members);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

fn right_click_at(members: &Members, at: Point) -> Vec<People> {
    let mut ui = simulate(members);
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    asked(ui)
}

/// The intents that are not the list's report of the rows in view, nor of
/// the row under the pointer.
fn asks(intents: Vec<People>) -> Vec<Intent> {
    intents
        .into_iter()
        .filter_map(|intent| match intent {
            People::Members(Intent::Shown(..) | Intent::Pointed(..)) => None,
            People::Members(intent) => Some(intent),
            other => panic!("not the member list's: {other:?}"),
        })
        .collect()
}

#[test]
fn the_pointer_on_a_row_names_its_member_and_leaving_names_none() {
    let members = samples::server();
    let mut ui = simulate(&members);
    for at in [row(0), Point::new(120.0, 30.0)] {
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    }
    let pointed: Vec<Option<String>> = asked(ui)
        .into_iter()
        .filter_map(|intent| match intent {
            People::Members(Intent::Pointed(user)) => Some(user),
            _ => None,
        })
        .collect();
    assert_eq!(pointed, [Some("discord:u2".to_owned()), None]);
}

#[test]
fn clicking_a_member_asks_for_their_card_beside_the_row() {
    let members = samples::server();
    let asked = asks(click_at(&members, row(0)));
    let [Intent::Open(id, spot)] = asked.as_slice() else {
        panic!("one Open, got {asked:?}");
    };
    assert_eq!(id, "discord:u2");
    // Beside the panel: on its right, the card's top 16 px above the row's.
    assert_eq!((spot.side, spot.align), (Side::Right, Align::Start));
    assert_eq!(spot.anchor.height, 42.0);
    assert_eq!(spot.anchor.y, 36.0);
}

#[test]
fn the_row_after_a_heading_is_the_second_group_s() {
    let members = samples::server();
    // Mods: heading, five rows; then the heading of the online group.
    let asked = asks(click_at(
        &members,
        Point::new(120.0, 8.0 + 42.0 + 44.0 * 5.0 + 42.0 + 2.0 + 21.0),
    ));
    assert_eq!(
        asked
            .first()
            .map(|intent| matches!(intent, Intent::Open(id, _) if id == "discord:u1")),
        Some(true)
    );
}

#[test]
fn a_heading_asks_nothing() {
    let members = samples::server();
    assert_eq!(asks(click_at(&members, Point::new(120.0, 30.0))), []);
}

#[test]
fn a_right_click_asks_for_the_menu_at_the_pointer() {
    let members = samples::server();
    let at = Point::new(130.0, 117.0);
    assert_eq!(
        asks(right_click_at(&members, at)),
        [Intent::Menu("discord:m0".into(), Spot::pointer(at))]
    );
}

#[test]
fn a_right_click_does_not_open_the_card() {
    let members = samples::server();
    let asked = asks(right_click_at(&members, row(1)));
    assert!(
        asked
            .iter()
            .all(|intent| matches!(intent, Intent::Menu(..)))
    );
}

#[test]
fn the_headings_and_names_are_drawn() {
    let members = samples::server();
    let mut ui = simulate(&members);
    for words in [
        "Mods \u{2014} 5",
        "Bob",
        "Member 0",
        "Factorio",
        "Online \u{2014} 28",
    ] {
        ui.find(words)
            .unwrap_or_else(|_| panic!("{words} is drawn"));
    }
}

#[test]
fn clicking_a_name_opens_that_member() {
    let members = samples::server();
    let mut ui = simulate(&members);
    ui.click("Member 8").expect("Member 8's row");
    let asked = asks(asked(ui));
    assert!(matches!(asked.as_slice(), [Intent::Open(id, _)] if id == "discord:m8"));
}

#[test]
fn a_group_lists_its_people() {
    let members = samples::group();
    assert!(members.group);
    let mut ui = simulate(&members);
    for words in ["Members \u{2014} 3", "Alice", "Bob", "Carol"] {
        ui.find(words)
            .unwrap_or_else(|_| panic!("{words} is drawn"));
    }
}

#[test]
fn every_state_draws() {
    let members = samples::states();
    let mut ui = simulate(&members);
    for words in [
        "Offline Olga",
        "Status Bot",
        "APP",
        "Booster Bea",
        "Timed-out Tim",
        "Crowned Cy",
    ] {
        ui.find(words)
            .unwrap_or_else(|_| panic!("{words} is drawn"));
    }
}

#[test]
fn the_note_stands_above_the_list() {
    let members = samples::loading();
    let mut ui = simulate(&members);
    ui.find("Loading members...").expect("the note");
}

#[test]
fn thousands_of_rows_build_only_those_near_the_view() {
    let members = samples::many(5000);
    let mut ui = simulate(&members);
    ui.find("Member 3").expect("a row in view");
    assert!(
        ui.find("Member 4000").is_err(),
        "a row far below is not built"
    );
    let asked = asks(click_at(&members, row(2)));
    assert!(matches!(asked.as_slice(), [Intent::Open(id, _)] if id == "discord:n2"));
}

#[test]
fn the_rows_in_view_are_reported_for_their_pictures() {
    let members = samples::many(5000);
    let mut ui = simulate(&members);
    ui.draw(&Theme::default());
    let shown: Vec<_> = asked(ui)
        .into_iter()
        .filter_map(|intent| match intent {
            People::Members(Intent::Shown(first, last)) => Some((first, last)),
            _ => None,
        })
        .collect();
    let Some(&(first, last)) = shown.last() else {
        panic!("no Shown");
    };
    assert_eq!(first, 0);
    assert!(last < 40, "only the rows in view: {last}");
}

/// Counts the drawn texts that are exactly `words`, overlays included.
struct Count<'a> {
    words: &'a str,
    seen: usize,
}

impl Operation for Count<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn text(&mut self, _id: Option<&Id>, _bounds: iced::Rectangle, state: &mut dyn Text) {
        if state.text() == self.words {
            self.seen += 1;
        }
    }
}

#[test]
fn a_gem_under_the_pointer_opens_one_tooltip() {
    let members = samples::states();
    let mut ui = simulate(&members);
    // Booster Bea's row is the third; her gem sits after her name.
    let at = Point::new(160.0, 161.0);
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    let words = "Server boosting since 3 March 2024";
    let mut count = Count { words, seen: 0 };
    ui.operate(&mut count as &mut dyn Operation);
    assert_eq!(count.seen, 1, "one card for the gem");
}
