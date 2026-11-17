//! The inbox popout of mentions and unreads: task N08's tests, driving
//! `anomi_screens::inbox::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::Simulator;

use anomi_vm::fixtures::inbox as fixtures;
use anomi_vm::inbox::{Inbox, Intent, Tab};

use super::support::{SIZE, asked, front, settings};

fn simulate(inbox: &Inbox) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(
        settings(),
        SIZE,
        anomi_screens::inbox::view(inbox, front()).boxed(),
    )
}

#[test]
fn a_line_asks_to_open_its_room() {
    let inbox = fixtures::unreads();
    let mut ui = simulate(&inbox);
    ui.click("#general").expect("the channel's line");
    assert_eq!(asked(ui), [Intent::Open("general".into())]);
}

#[test]
fn a_conversation_opens_by_its_id() {
    let inbox = fixtures::unreads();
    let mut ui = simulate(&inbox);
    ui.click("Alice").expect("the conversation's line");
    assert_eq!(asked(ui), [Intent::Open("dm-alice".into())]);
}

#[test]
fn the_tabs_ask_to_be_shown() {
    let inbox = fixtures::unreads();
    let mut ui = simulate(&inbox);
    ui.click("Mentions").expect("the Mentions tab");
    assert_eq!(asked(ui), [Intent::Tab(Tab::Mentions)]);

    let inbox = fixtures::mentions();
    let mut ui = simulate(&inbox);
    ui.click("Unreads").expect("the Unreads tab");
    assert_eq!(asked(ui), [Intent::Tab(Tab::Unreads)]);
}

#[test]
fn a_tab_with_nothing_says_so() {
    let inbox = fixtures::empty(Tab::Mentions);
    let mut ui = simulate(&inbox);
    assert!(ui.find("Nothing mentions you right now.").is_ok());
    let inbox = fixtures::empty(Tab::Unreads);
    let mut ui = simulate(&inbox);
    assert!(
        ui.find("Unread messages from your unmuted channels show up here.")
            .is_ok()
    );
}

#[test]
fn mentions_show_as_a_count_that_stops_at_99() {
    let inbox = fixtures::unreads();
    assert_eq!(inbox.rows[0].badge, "2");
    assert_eq!(inbox.rows[3].badge, "99+");
    assert_eq!(inbox.rows[1].badge, "");
    let mut ui = simulate(&inbox);
    assert!(ui.find("99+").is_ok());
}
