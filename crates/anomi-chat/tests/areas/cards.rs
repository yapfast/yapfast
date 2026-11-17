//! The card a custom emoji or a sticker opens: task C08's tests, driving
//! `anomi_chat::cards::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced_test::Simulator;

use anomi_chat::cards::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::cards::{Cards, Intent};
use anomi_vm::fixtures::chat::cards as samples;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(400.0, 300.0);

fn card(cards: &Cards) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(cards, front()))
}

#[test]
fn an_emoji_card_names_it_and_its_server() {
    let cards = samples::showing_emoji(samples::emoji());
    let mut ui = card(&cards);
    for text in [
        ":pog:",
        "This emoji is from one of your servers.",
        "This emoji is from",
        "Server",
        "3,999 Online \u{b7} Discoverable Server",
    ] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn a_default_emoji_has_no_server_strip() {
    let cards = samples::showing_emoji(samples::emoji_unicode());
    let mut ui = card(&cards);
    ui.find(":sob:").expect("the name");
    assert!(ui.find("This emoji is from").is_err());
}

#[test]
fn a_server_without_counts_shows_its_initial() {
    let cards = samples::showing_emoji(samples::emoji_late());
    let mut ui = card(&cards);
    ui.find("F").expect("the initial");
    ui.find("Filler 2").expect("the name");
}

#[test]
fn clicking_the_server_opens_it() {
    for cards in [
        samples::showing_emoji(samples::emoji()),
        samples::showing_sticker(samples::sticker()),
    ] {
        let mut ui = card(&cards);
        ui.click("Server").expect("the server");
        assert_eq!(asked(ui), vec![Intent::OpenServer("discord:g1".into())]);
    }
}

#[test]
fn a_sticker_card_names_it_and_its_server() {
    let cards = samples::showing_sticker(samples::sticker());
    let mut ui = card(&cards);
    for text in [
        "wave",
        "This sticker is from one of your servers.",
        "This sticker is from",
        "Server",
    ] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn a_default_sticker_has_no_server_strip() {
    let cards = samples::showing_sticker(samples::sticker_default());
    let mut ui = card(&cards);
    ui.find("sob").expect("the name");
    assert!(ui.find("This sticker is from").is_err());
}

#[test]
fn nothing_shows_while_both_are_closed() {
    let cards = Cards::default();
    let mut ui = card(&cards);
    assert!(ui.find("Server").is_err());
}
