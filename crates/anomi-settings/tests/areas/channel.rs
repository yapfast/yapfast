//! A channel's settings frame and its overview: task X13's tests, driving
//! `anomi_settings::channel::view` over its fixtures in the simulator
//! (`super::support`). The window is the golden one: the card is 1288 by 748
//! from (56, 56); the sections are 266 wide, the page's column starts at
//! x 485, its close button's centre is (1320, 80), the slowmode's steppers
//! (501, 388) and (597, 388), and the bar for unsaved changes, with its two
//! buttons, floats at the card's bottom.

use iced::Point;
use iced_test::Simulator;

use anomi_settings::channel::view;
use anomi_vm::fixtures::settings::channel as sample;
use anomi_vm::settings::Intent;
use anomi_vm::settings::channel::{Channel, Intent as Asked, Page};

use super::support::{SIZE, asked, front, settings};

const CLOSE: Point = Point::new(1320.0, 80.0);
const SHORTER: Point = Point::new(501.0, 388.0);
const LONGER: Point = Point::new(597.0, 388.0);
/// Inside the topic's lines, under their text.
const TOPIC: Point = Point::new(900.0, 310.0);
const SAVE: Point = Point::new(1106.0, 742.0);
const RESET: Point = Point::new(986.0, 742.0);

fn ask(intent: Asked) -> Vec<Intent> {
    vec![Intent::Channel(intent)]
}

fn click(channel: &Channel, text: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(channel, front()));
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

fn click_at(channel: &Channel, at: Point) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(channel, front()));
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

fn shows(channel: &Channel, text: &str) -> bool {
    let mut ui = Simulator::with_size(settings(), SIZE, view(channel, front()));
    ui.find(text).is_ok()
}

#[test]
fn choosing_a_section_asks_to_open_its_page() {
    let channel = sample::overview();
    assert_eq!(
        click(&channel, "Permissions"),
        ask(Asked::Choose(Page::Permissions))
    );
    assert_eq!(
        click(&channel, "Delete Channel"),
        ask(Asked::Choose(Page::Delete))
    );
}

#[test]
fn a_category_deletes_as_a_category() {
    let channel = sample::category();
    assert_eq!(
        click(&channel, "Delete Category"),
        ask(Asked::Choose(Page::Delete))
    );
}

#[test]
fn the_close_button_asks_to_close() {
    assert_eq!(click_at(&sample::overview(), CLOSE), ask(Asked::Close));
}

#[test]
fn the_slowmode_steps_one_interval_and_stops_at_its_ends() {
    let mut channel = sample::overview();
    assert_eq!(click_at(&channel, LONGER), ask(Asked::Slowmode(1)));
    assert_eq!(click_at(&channel, SHORTER), ask(Asked::Slowmode(0)));
    channel.slowmode = 7;
    assert_eq!(click_at(&channel, SHORTER), ask(Asked::Slowmode(6)));
    channel.slowmode = 13;
    assert_eq!(click_at(&channel, LONGER), ask(Asked::Slowmode(13)));
    assert!(shows(&channel, "6h"));
}

#[test]
fn the_age_restriction_toggles_the_other_way() {
    let mut channel = sample::overview();
    assert_eq!(
        click(&channel, "Age-Restricted Channel"),
        ask(Asked::Nsfw(true))
    );
    channel.nsfw = true;
    assert_eq!(
        click(&channel, "Age-Restricted Channel"),
        ask(Asked::Nsfw(false))
    );
}

#[test]
fn typing_in_the_name_asks_to_write_it() {
    let channel = sample::overview();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&channel, front()));
    ui.click("general").expect("the name");
    ui.typewrite("x");
    assert!(asked(ui).contains(&Intent::Channel(Asked::Name("generalx".into()))));
}

#[test]
fn typing_in_the_topic_asks_to_write_it() {
    let channel = sample::overview();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&channel, front()));
    ui.point_at(TOPIC);
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("x");
    let asked = asked(ui);
    assert!(
        asked
            .iter()
            .any(|intent| matches!(intent, Intent::Channel(Asked::Topic(topic)) if topic.contains('x') && topic.len() == channel.topic.len() + 1)),
        "{asked:?}"
    );
}

#[test]
fn only_a_text_channel_has_a_topic_a_slowmode_and_an_age_restriction() {
    for channel in [sample::voice(), sample::category()] {
        assert!(!shows(&channel, "CHANNEL TOPIC"));
        assert!(!shows(&channel, "SLOWMODE"));
        assert!(!shows(&channel, "Age-Restricted Channel"));
    }
    assert!(shows(&sample::category(), "CATEGORY NAME"));
    assert!(shows(&sample::voice(), "CHANNEL NAME"));
    assert!(shows(&sample::overview(), "SLOWMODE"));
}

#[test]
fn a_draft_on_its_way_asks_for_nothing() {
    let channel = sample::saving();
    assert!(click(&channel, "Age-Restricted Channel").is_empty());
    assert!(click_at(&channel, LONGER).is_empty());
    assert!(click_at(&channel, SAVE).is_empty());
}

#[test]
fn the_bar_for_unsaved_changes_resets_and_saves() {
    assert!(!shows(&sample::overview(), "Save Changes"));
    let channel = sample::changed();
    assert!(shows(&channel, "Save Changes"));
    assert_eq!(click_at(&channel, SAVE), ask(Asked::Save));
    assert_eq!(click_at(&channel, RESET), ask(Asked::Undo));
}

#[test]
fn what_discord_refused_is_in_red_under_the_page() {
    assert!(shows(&sample::refused(), "Missing Permissions"));
    assert!(shows(&sample::loading(), "Loading..."));
    assert!(!shows(&sample::loading(), "SLOWMODE"));
}
