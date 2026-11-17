//! The server, tag and role cards: task P12's tests, driving
//! `anomi_people::cards::view` and `role` over its fixtures in the simulator
//! (`super::support`). Clicks are found by the code's English text.

use iced::{Point, Size, Widget as _};
use iced_test::Simulator;

use anomi_people::cards::{role, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::people::cards as samples;
use anomi_vm::people::cards::{Cards, Intent};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(400.0, 700.0);

fn card(cards: &Cards) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(cards, front()))
}

/// The card standing on the simulator's floor, so that its last control, the
/// primary button, is where the card's padding says: its 36 px end 16 px
/// above the card's 1 px edge.
fn grounded(cards: &Cards) -> Simulator<'_, Intent, Theme, Renderer> {
    let floor = iced::widget::container(view(cards, front()).boxed())
        .height(iced::Fill)
        .align_bottom(iced::Fill);
    Simulator::with_size(settings(), SIZE, floor)
}

/// A click on that button.
fn press_the_button(ui: &mut Simulator<'_, Intent, Theme, Renderer>) {
    ui.point_at(Point::new(150.0, SIZE.height - 1.0 - 16.0 - 18.0));
    ui.simulate(iced_test::simulator::click());
}

fn roles(cards: &Cards) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, role(cards, front()))
}

#[test]
fn a_server_card_names_the_server_its_counts_and_nickname() {
    let cards = samples::showing_server(samples::server());
    let mut ui = card(&cards);
    for text in [
        "Server",
        "31 members",
        "4 channels",
        "THEIR NICKNAME",
        "Bobby",
    ] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn a_server_without_a_member_count_or_nickname_shows_neither() {
    let cards = samples::showing_server(samples::server_plain());
    let mut ui = card(&cards);
    ui.find("1 channel").expect("the channels");
    assert!(ui.find("THEIR NICKNAME").is_err());
}

#[test]
fn go_to_server_opens_the_server() {
    let cards = samples::showing_server(samples::server());
    let mut ui = grounded(&cards);
    press_the_button(&mut ui);
    assert_eq!(asked(ui), [Intent::OpenServer("discord:g1".into())]);
}

#[test]
fn a_tag_card_shows_what_discord_said() {
    let cards = samples::showing_tag(samples::tag());
    let mut ui = card(&cards);
    for text in [
        "Server",
        "BOB",
        "1,200 Online",
        "4,644 Members",
        "Est. Nov 2026",
    ] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn a_joined_servers_name_and_button_open_it() {
    let cards = samples::showing_tag(samples::tag());
    let mut ui = grounded(&cards);
    ui.click("Server").expect("the name");
    press_the_button(&mut ui);
    let id = String::from("discord:g1");
    assert_eq!(
        asked(ui),
        [Intent::OpenServer(id.clone()), Intent::OpenServer(id)]
    );
}

#[test]
fn a_server_this_account_is_not_in_opens_nothing() {
    let cards = samples::showing_tag(samples::tag_stranger());
    let mut ui = card(&cards);
    assert!(ui.find("Go to Server").is_err());
    let _ = ui.click("Server");
    assert_eq!(asked(ui), []);
}

#[test]
fn a_tag_waiting_for_discord_shows_no_words_yet() {
    let cards = samples::showing_tag(samples::tag_loading());
    let mut ui = card(&cards);
    ui.find("BOB").expect("the tag is known already");
    for text in ["Server", "Go to Server", "Est. Nov 2026"] {
        assert!(ui.find(text).is_err(), "{text} waits for Discord");
    }
}

#[test]
fn a_private_server_says_why_and_offers_no_way_in() {
    let cards = samples::showing_tag(samples::tag_limited());
    let mut ui = card(&cards);
    ui.find("Private Server").expect("its name");
    ui.find("The server has limited who can see this profile.")
        .expect("the note");
    assert!(ui.find("Go to Server").is_err());
}

#[test]
fn a_failed_profile_tries_again_when_its_note_is_clicked() {
    let cards = samples::showing_tag(samples::tag_failed());
    let mut ui = card(&cards);
    ui.click("Unable to load server profile").expect("the note");
    assert_eq!(asked(ui), [Intent::Retry("discord:g1".into())]);
}

#[test]
fn a_role_card_names_the_role_and_its_people() {
    let cards = samples::showing_role(samples::role());
    let mut ui = roles(&cards);
    for text in ["@Mod", "Bob", "Member 1", "Member 4"] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn a_click_on_a_person_opens_their_card_beside_the_row() {
    let cards = samples::showing_role(samples::role());
    let mut ui = roles(&cards);
    // The card's padding is 12 and its heading 28 tall: the first row spans
    // y 44 to 84 of a card at the top left, 260 wide.
    ui.point_at(Point::new(100.0, 64.0));
    ui.simulate(iced_test::simulator::click());
    let asked = asked(ui);
    assert_eq!(asked.len(), 1);
    let Intent::OpenMember(id, spot) = &asked[0] else {
        panic!("asked {asked:?}");
    };
    assert_eq!(id, &cards.role.as_ref().unwrap().members[0].id);
    assert!((spot.anchor.height - 40.0).abs() < 1.0, "{spot:?}");
    assert_eq!(spot.anchor.x, 0.0, "{spot:?}");
    assert_eq!(spot.anchor.width, 260.0, "{spot:?}");
    assert_eq!(spot.gap, anomi_vm::menu::GAP_BESIDE);
}

#[test]
fn a_role_still_loading_says_so() {
    let cards = samples::showing_role(samples::role_loading());
    let mut ui = roles(&cards);
    ui.find("Loading...").expect("the note");
    assert!(ui.find("Bob").is_err());
}

#[test]
fn a_role_no_one_is_known_to_have_says_so() {
    let cards = samples::showing_role(samples::role_empty());
    let mut ui = roles(&cards);
    ui.find("Yapfast hasn't seen anyone with this role yet.")
        .expect("the note");
}

#[test]
fn hundreds_of_people_build_only_the_rows_in_view() {
    let cards = samples::showing_role(samples::role_many());
    let mut ui = roles(&cards);
    ui.find("Member 0").expect("the first row");
    assert!(ui.find("Member 399").is_err(), "the last is far below");
}

#[test]
fn closed_cards_draw_nothing_asked() {
    let cards = Cards::default();
    let mut ui = roles(&cards);
    assert!(ui.find("@Mod").is_err());
    assert_eq!(asked(ui), []);
}
