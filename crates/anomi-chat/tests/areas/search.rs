//! Searching messages: task C21's tests, driving `anomi_chat::search::view`
//! over its fixtures in the simulator (`super::support`).

use iced::advanced::widget::operation::focusable;
use iced::keyboard::{Key, key::Named};
use iced::{Point, Size};
use iced_test::{Simulator, selector, simulator};

use anomi_chat::search::{COLLAPSED, JUMP, MORE, OPEN, REST, field, natural, view};
use anomi_vm::chat::search::{Intent, Search};
use anomi_vm::fixtures::chat::search as sample;
use anomi_widgets::focus;

use super::support;

/// The panel's own size: 420 wide, as the conversation docks it.
const PANEL: Size = Size::new(420.0, 860.0);
/// The field's: a header's room for it.
const ROOM: Size = Size::new(400.0, 50.0);

fn panel(search: &Search) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(support::settings(), PANEL, view(search, support::front()))
}

fn clicked_panel(search: &Search, at: Point) -> Vec<Intent> {
    let mut ui = panel(search);
    ui.point_at(at);
    ui.simulate(simulator::click());
    support::asked(ui)
}

fn in_field(search: &Search) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(support::settings(), ROOM, field(search, support::front()))
}

fn key(named: Named) -> Key {
    Key::Named(named)
}

#[test]
fn the_field_grows_when_used_and_shrinks_to_the_magnifier_in_a_short_header() {
    let mut search = Search::default();
    assert_eq!(natural(&search), REST);
    search.focused = true;
    assert_eq!(natural(&search), OPEN);
    search.focused = false;
    search.text = "hi".into();
    assert_eq!(natural(&search), OPEN);
    search.text.clear();
    search.compact = true;
    assert_eq!(natural(&search), COLLAPSED);
    search.focused = true;
    assert_eq!(natural(&search), OPEN);
}

#[test]
fn typing_edits_and_enter_searches() {
    let search = Search {
        placeholder: "Search Server".into(),
        ..Search::default()
    };
    let mut ui = in_field(&search);
    ui.operate(&mut focusable::focus::<()>(focus::SEARCH));
    ui.typewrite("bo");
    ui.tap_key(key(Named::Enter));
    let asked = support::asked(ui);
    assert!(asked.contains(&Intent::Edit("b".into())));
    assert!(asked.contains(&Intent::Edit("bo".into())));
    assert_eq!(asked.last(), Some(&Intent::Submit));
}

#[test]
fn clicking_the_field_says_it_has_the_keys() {
    let search = Search::default();
    let mut ui = in_field(&search);
    ui.point_at(Point::new(60.0, 15.0));
    ui.simulate(simulator::click());
    assert_eq!(support::asked(ui), [Intent::Focus(true)]);
}

#[test]
fn the_placeholder_shows_until_there_is_text_and_not_in_the_magnifier() {
    let search = Search {
        placeholder: "Search Server".into(),
        ..Search::default()
    };
    assert!(in_field(&search).find("Search Server").is_ok());
    let typed = Search {
        text: "a".into(),
        ..search.clone()
    };
    assert!(in_field(&typed).find("Search Server").is_err());
    let collapsed = Search {
        compact: true,
        ..search
    };
    assert!(in_field(&collapsed).find("Search Server").is_err());
}

#[test]
fn the_cross_shows_with_text_and_ends_the_search() {
    let typed = Search {
        text: "bob".into(),
        focused: true,
        ..Search::default()
    };
    let mut ui = in_field(&typed);
    // The cross sits 2 px in from the end of the 300 px field.
    ui.point_at(Point::new(300.0 - 2.0 - 12.0, 15.0));
    ui.simulate(simulator::click());
    assert_eq!(support::asked(ui), [Intent::Clear]);

    // With nothing typed, that spot is the field's: no cross.
    let empty = Search::default();
    let mut ui = in_field(&empty);
    ui.point_at(Point::new(240.0 - 14.0, 15.0));
    ui.simulate(simulator::click());
    assert!(!support::asked(ui).contains(&Intent::Clear));
}

#[test]
fn escape_in_a_field_with_the_keys_leaves_it_with_or_without_text() {
    let typed = Search {
        text: "bob".into(),
        ..Search::default()
    };
    let mut ui = in_field(&typed);
    ui.operate(&mut focusable::focus::<()>(focus::SEARCH));
    ui.tap_key(key(Named::Escape));
    assert!(support::asked(ui).contains(&Intent::Leave { cleared: true }));

    let empty = Search::default();
    let mut ui = in_field(&empty);
    ui.operate(&mut focusable::focus::<()>(focus::SEARCH));
    ui.tap_key(key(Named::Escape));
    assert!(support::asked(ui).contains(&Intent::Leave { cleared: false }));

    // Without the keys, Escape is not the field's.
    let mut ui = in_field(&empty);
    ui.tap_key(key(Named::Escape));
    assert!(
        !support::asked(ui)
            .iter()
            .any(|i| matches!(i, Intent::Leave { .. }))
    );
}

#[test]
fn the_panel_says_how_many_it_found_and_closes_with_its_cross() {
    let search = sample::searching();
    assert!(panel(&search).find("55 Results").is_ok());
    assert!(panel(&search).find("general").is_ok());
    // The cross: 28 square, 10 px from the right, in a 48 px header.
    assert_eq!(
        clicked_panel(&search, Point::new(420.0 - 10.0 - 14.0, 24.0)),
        [Intent::Close]
    );
}

#[test]
fn a_card_jumps_to_its_message_from_anywhere_on_it_and_from_its_button() {
    let search = sample::searching();
    let first = &search.results[0];
    let jump = Intent::Jump {
        room: first.room.clone(),
        item: first.item.clone(),
    };
    // Under the channel's name, over the first card's body.
    assert_eq!(
        clicked_panel(&search, Point::new(150.0, 125.0)),
        std::slice::from_ref(&jump)
    );

    // The pointer over the card brings its Jump button.
    let mut ui = panel(&search);
    let over = Point::new(150.0, 125.0);
    ui.point_at(over);
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: over,
    })]);
    ui.click(selector::id(JUMP)).unwrap();
    assert!(support::asked(ui).contains(&jump));
}

#[test]
fn a_card_shows_its_author_when_and_words() {
    let search = sample::searching();
    let mut ui = panel(&search);
    assert!(ui.find("Bob").is_ok());
    assert!(ui.find("01.10.2026 16:59").is_ok());
}

#[test]
fn every_kind_of_find_lays_out_with_what_it_holds() {
    let search = sample::kinds();
    let mut ui = Simulator::with_size(
        support::settings(),
        Size::new(420.0, 1400.0),
        view(&search, support::front()),
    );
    assert!(ui.find("Where do we meet?").is_ok());
    assert!(ui.find("The usual place").is_ok());
    assert!(ui.find("75%").is_ok());
    assert!(ui.find("4 votes").is_ok());
    assert!(ui.find("release-notes.txt, report.pdf").is_ok());
}

#[test]
fn show_more_asks_for_the_next_page_and_not_while_it_loads() {
    let search = sample::searching();
    let mut ui = Simulator::with_size(
        support::settings(),
        Size::new(420.0, 3000.0),
        view(&search, support::front()),
    );
    ui.click(selector::id(MORE)).unwrap();
    assert_eq!(support::asked(ui), [Intent::More]);

    let loading = sample::loading();
    let mut ui = Simulator::with_size(
        support::settings(),
        Size::new(420.0, 3000.0),
        view(&loading, support::front()),
    );
    // A disabled button takes no key and offers no focus.
    assert!(ui.find(selector::id(MORE)).is_err());
}

#[test]
fn a_search_with_no_results_says_why_and_offers_no_more() {
    let search = sample::nothing();
    let mut ui = panel(&search);
    assert!(
        ui.find("Nothing in this server matches. Try other words.")
            .is_ok()
    );
    assert!(ui.find(selector::id(MORE)).is_err());
}

#[test]
fn the_whole_window_draws_the_panel() {
    // The conversation docks it at SEARCH_WIDTH; the window's simulation
    // belongs to the screens' tests, here only that the fixture is the
    // search beside a channel.
    let vm = sample::beside();
    assert!(vm.conversation.beside.search);
    assert_eq!(vm.chat.search.results.len(), 12);
}
