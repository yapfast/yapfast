//! The shell, driven headless: a click on a row asks to open it, a click on
//! a server asks for it, typing in the search field filters. Each screen's
//! task adds tests like these, over its fixtures.

use std::borrow::Cow;

use iced::{Settings, Size};
use iced_test::Simulator;

use anomi_theme::{Element, Motion, Renderer, Theme, fonts, typography};
use anomi_vm::{Intent, fixtures, rail, sidebar};

const SIZE: Size = Size::new(1400.0, 860.0);

fn settings() -> Settings {
    Settings {
        fonts: fonts::ALL.iter().map(|font| Cow::Borrowed(*font)).collect(),
        font: typography::UI,
        ..Settings::default()
    }
}

fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

/// The messages a simulator gathered.
fn asked(ui: Simulator<'_, Intent, Theme, Renderer>) -> Vec<Intent> {
    ui.into_messages().collect()
}

fn window(vm: &anomi_vm::Vm) -> Element<'_, Intent> {
    use iced::Widget as _;
    anomi_screens::window(vm, front()).boxed()
}

#[test]
fn clicking_a_dm_asks_to_open_it() {
    let vm = fixtures::dm_home();
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm));
    ui.click("Person 4").expect("Person 4's row");
    assert_eq!(
        asked(ui),
        [Intent::Sidebar(sidebar::Intent::Open("discord:p4".into()))]
    );
}

#[test]
fn clicking_friends_asks_for_the_friends_screen() {
    let vm = fixtures::dm_home();
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm));
    // No language is chosen in tests, so the text is the code's own.
    ui.click("Friends").expect("the Friends row");
    assert_eq!(asked(ui), [Intent::Sidebar(sidebar::Intent::Friends)]);
}

#[test]
fn typing_in_the_search_field_filters() {
    let vm = fixtures::dm_home();
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm));
    ui.click("Find a conversation").expect("the search field");
    ui.typewrite("bo");
    let intents = asked(ui);
    assert_eq!(
        intents.last(),
        Some(&Intent::Sidebar(sidebar::Intent::Filter("bo".into()))),
        "{intents:?}"
    );
}

#[test]
fn a_server_on_the_rail_is_a_button() {
    let vm = fixtures::server_home();
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm));
    // The rail draws its tiles itself; a click at the second server's tile.
    ui.point_at(iced::Point::new(36.0, 133.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        [Intent::Rail(rail::Intent::Server("discord:g2".into()))]
    );
}
