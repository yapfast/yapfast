//! A server's Members settings: task X03's tests, driving
//! `anomi_settings::server::members::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Point, mouse};
use iced_test::{Simulator, simulator};

use anomi_settings::server::members::view;
use anomi_vm::fixtures::settings::members::members;
use anomi_vm::settings::members::{Intent, Members};

use super::support::{SIZE, asked, front, settings};

/// The middle of the first row, and of its More Options at the right end.
const FIRST_ROW: f32 = 112.0;
const MORE: f32 = 1384.0;

fn click(vm: &Members, text: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(vm, front()));
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

fn has(vm: &Members, text: &str) -> bool {
    let mut ui = Simulator::with_size(settings(), SIZE, view(vm, front()));
    ui.find(text).is_ok()
}

#[test]
fn a_row_opens_its_members_profile() {
    let vm = members();
    let carol = vm.rows[1].id.clone();
    assert_eq!(click(&vm, "Carol"), vec![Intent::Open(carol)]);
}

#[test]
fn the_pager_moves_between_pages() {
    let mut vm = members();
    assert_eq!(click(&vm, "Next"), vec![Intent::Page(1)]);
    vm.page = 1;
    assert_eq!(click(&vm, "Previous"), vec![Intent::Page(0)]);
    vm.page = 2;
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    let _ = ui.click("Next");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_footer_counts_the_members() {
    let vm = members();
    assert!(has(&vm, "members of 30"));
    assert!(has(&vm, "1 of 3"));
    let few = Members {
        total: 3,
        pages: 1,
        ..members()
    };
    assert!(has(&few, "Showing 3 members"));
    assert!(!has(&few, "Next"));
}

#[test]
fn typing_searches() {
    let vm = members();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    ui.click("Search by username or id").unwrap();
    ui.typewrite("al");
    let asked = asked(ui);
    assert!(
        asked
            .iter()
            .all(|intent| matches!(intent, Intent::Search(_))),
        "{asked:?}"
    );
    assert_eq!(asked.last(), Some(&Intent::Search("al".into())));
}

#[test]
fn search_results_and_empty_pages_say_so() {
    let mut vm = members();
    assert!(has(&vm, "Recent Members"));
    vm.searched = true;
    assert!(has(&vm, "Search Results"));
    vm.rows.clear();
    assert!(has(&vm, "No members match these search results."));
    vm.fetching = true;
    assert!(has(&vm, "Loading..."));
    vm.indexing = true;
    assert!(has(
        &vm,
        "Before searching, we need to index this server. Give us a bit."
    ));
}

#[test]
fn the_join_columns_follow_the_account_and_the_width() {
    let mut vm = members();
    assert!(has(&vm, "JOIN METHOD"));
    assert!(has(&vm, "JOINED DISCORD"));
    vm.join_method = false;
    vm.narrow = true;
    assert!(!has(&vm, "JOIN METHOD"));
    assert!(!has(&vm, "JOINED DISCORD"));
}

#[test]
fn enter_searches_at_once() {
    let vm = Members {
        search: "al".into(),
        ..members()
    };
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    ui.point_at(Point::new(1000.0, 18.0));
    ui.simulate(simulator::click());
    ui.tap_key(iced::keyboard::Key::Named(
        iced::keyboard::key::Named::Enter,
    ));
    assert_eq!(asked(ui), vec![Intent::Find("al".into())]);
}

#[test]
fn the_pickers_choose_an_order_and_a_page_size() {
    let vm = members();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    ui.click("Member Since (Newest first)").unwrap();
    ui.click("Joined Discord (Oldest first)").unwrap();
    assert_eq!(asked(ui), vec![Intent::Sort(3)]);
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    ui.click("12").unwrap();
    ui.click("50").unwrap();
    assert_eq!(asked(ui), vec![Intent::PageSize(2)]);
}

#[test]
fn more_options_and_a_right_click_open_the_members_menu() {
    let vm = members();
    let dave = vm.rows[0].id.clone();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    ui.point_at(Point::new(MORE, FIRST_ROW));
    ui.simulate(simulator::click());
    let asked_more = asked(ui);
    assert!(
        matches!(asked_more.as_slice(), [Intent::Menu(id, _)] if *id == dave),
        "{asked_more:?}"
    );
    let mut ui = Simulator::with_size(settings(), SIZE, view(&vm, front()));
    ui.point_at(Point::new(600.0, FIRST_ROW));
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    let asked_right = asked(ui);
    assert!(
        matches!(asked_right.as_slice(), [Intent::Menu(id, _)] if *id == dave),
        "{asked_right:?}"
    );
}
