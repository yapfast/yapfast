//! The full profile: task P09's tests, driving
//! `anomi_people::profile::full::view` over its fixtures in the simulator
//! (`super::support`). The modal is 960 by 800: its card 400 wide 48 px in,
//! the tabs' column after it, and the close button 32 px in the top right
//! corner, 8 px in (its middle at 936, 24).

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_people::profile::full::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::people::full_profile as samples;
use anomi_vm::people::full_profile::{FullProfile, Intent, Page};
use anomi_vm::people::profile::Intent as CardIntent;
use anomi_vm::people::{self};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(960.0, 800.0);
const CLOSE: Point = Point::new(936.0, 24.0);

type Ui<'a> = Simulator<'a, people::Intent, Theme, Renderer>;

fn at(full: &FullProfile) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(full, front()))
}

/// What the modal asked for, but the pointer coming and going.
fn said(ui: Ui<'_>) -> Vec<people::Intent> {
    asked(ui)
        .into_iter()
        .filter(|intent| {
            !matches!(
                intent,
                people::Intent::Profile(CardIntent::Pointed(..) | CardIntent::Bubble(..))
            )
        })
        .collect()
}

fn click_text<'a>(mut ui: Ui<'a>, words: &str) -> Vec<people::Intent> {
    ui.click(words).expect(words);
    said(ui)
}

fn own(intent: Intent) -> people::Intent {
    people::Intent::FullProfile(intent)
}

#[test]
fn the_close_button_closes() {
    let full = samples::bob();
    let mut ui = at(&full);
    ui.point_at(CLOSE);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(said(ui), [own(Intent::Close)]);
}

#[test]
fn a_tab_is_chosen_by_its_words() {
    let full = samples::bob();
    assert_eq!(
        click_text(at(&full), "1 Mutual Friend"),
        [own(Intent::Choose(Page::Friends))]
    );
    assert_eq!(
        click_text(at(&full), "1 Mutual Server"),
        [own(Intent::Choose(Page::Servers))]
    );
}

#[test]
fn the_activity_tab_lists_what_they_are_doing() {
    let full = samples::bob();
    let mut ui = at(&full);
    ui.find("Factorio").expect("the game's card");
    // The tab is Activity: nothing of the mutual lists shows.
    assert!(ui.find("Carol").is_err());
}

#[test]
fn recent_activity_comes_under_the_cards() {
    let full = samples::recent();
    let mut ui = at(&full);
    ui.find("Recent activity").expect("its title");
    ui.find("Lofi beats").expect("an entry");
}

#[test]
fn a_mutual_friend_opens_their_profile() {
    let full = samples::on_friends();
    assert_eq!(
        click_text(at(&full), "Carol"),
        [own(Intent::OpenFriend(
            "discord:1400000000000000003".into()
        ))]
    );
}

#[test]
fn a_mutual_server_opens_its_card_beside_its_row() {
    let mut full = samples::on_servers();
    full.servers[0].nick = "Bobby".into();
    let mut ui = at(&full);
    ui.find("Bobby").expect("the name they go by there");
    let asked = click_text(ui, "Server");
    let [people::Intent::FullProfile(Intent::OpenServer(id, row))] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(id, "1400000000000000100");
    assert!(row.width > 100.0 && row.height >= 46.0, "{row:?}");
}

#[test]
fn a_list_still_on_its_way_shows_rows_not_words() {
    let full = samples::loading();
    let mut ui = at(&full);
    assert!(ui.find("You don't have any friends in common").is_err());
    assert!(ui.find("Mutual Friends").is_ok());
}

#[test]
fn an_empty_tab_says_so_and_offers_a_message() {
    let mut full = samples::empty();
    let mut ui = at(&full);
    ui.find("Bob doesn't have any activity to share here")
        .expect("the heading");
    let asked = click_text(ui, "Message");
    assert!(
        matches!(asked.as_slice(), [people::Intent::Profile(CardIntent::Message(id))] if id == &full.card.id),
        "{asked:?}"
    );
    // On their friends, with none shared.
    full.chosen = Some(Page::Friends);
    let mut ui = at(&full);
    ui.find("You don't have any friends in common")
        .expect("the friends' words");
}

#[test]
fn the_board_and_the_wishlist_show_their_items() {
    let board = samples::board();
    let mut ui = at(&board);
    ui.find("Favourite games").expect("a widget");
    ui.find("This one only shows in Discord for now.")
        .expect("the widget only Discord shows");
    let wishlist = samples::wishlist();
    let mut ui = at(&wishlist);
    ui.find("Satisfactory").expect("an item");
    ui.find("Not a gift list.").expect("the note");
}

#[test]
fn a_chosen_tab_that_goes_falls_back_to_the_first() {
    let mut full = samples::on_servers();
    full.servers_tab = false;
    let mut ui = at(&full);
    // The Board is the first when there is one, else Activity.
    ui.find("Factorio").expect("Activity shows");
}
