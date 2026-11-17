//! A Community server's Browse Channels: task N14's tests, driving
//! `anomi_screens::browse::view` over its fixtures in the simulator
//! (`super::support`).

use anomi_screens::browse::view;
use anomi_vm::browse::Intent;
use anomi_vm::fixtures::browse;
use iced::Widget as _;
use iced_test::Simulator;

use super::support::{SIZE, asked, front, settings};

fn simulate(
    browse: &anomi_vm::browse::Browse,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(browse, front()).boxed())
}

#[test]
fn clicking_a_channel_follows_it_or_clears_it() {
    let sample = browse::browse();
    let mut ui = simulate(&sample);
    ui.click("ideas").unwrap();
    assert_eq!(
        asked(ui),
        [Intent::Follow {
            room: "2".into(),
            follow: true
        }]
    );
    let mut ui = simulate(&sample);
    ui.click("general").unwrap();
    assert_eq!(
        asked(ui),
        [Intent::Follow {
            room: "1".into(),
            follow: false
        }]
    );
}

#[test]
fn a_button_opens_its_channel_and_does_not_tick_it() {
    let sample = browse::browse();
    let mut ui = simulate(&sample);
    // The Lounge row's button, at the row's right end.
    ui.point_at(iced::Point::new(1320.0, 250.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::View("3".into())]);
}

#[test]
fn a_channel_followed_through_its_category_does_not_answer() {
    let sample = browse::browse();
    let mut ui = simulate(&sample);
    ui.click("news").unwrap();
    assert_eq!(asked(ui), []);
}

#[test]
fn a_categorys_switch_follows_all_of_it() {
    let sample = browse::browse();
    let mut ui = simulate(&sample);
    assert!(ui.find("Follow category").is_ok());
    // The first heading's switch, at the row's right end under the search.
    ui.point_at(iced::Point::new(1348.0, 82.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        [Intent::Follow {
            room: "c1".into(),
            follow: true
        }]
    );
}

#[test]
fn a_switchs_label_answers_too() {
    let sample = browse::browse();
    let mut ui = simulate(&sample);
    ui.click("Follow category").unwrap();
    assert_eq!(
        asked(ui),
        [Intent::Follow {
            room: "c1".into(),
            follow: true
        }]
    );
}

#[test]
fn typing_in_the_search_filters() {
    let sample = browse::browse();
    let mut ui = simulate(&sample);
    ui.click("Search channels").unwrap();
    ui.typewrite("g");
    assert_eq!(asked(ui).last(), Some(&Intent::Filter("g".into())));
}

#[test]
fn an_empty_list_says_why() {
    let nothing = browse::nothing_matches();
    let mut ui = simulate(&nothing);
    assert!(ui.find("No channel's name matches that.").is_ok());
    let none = browse::empty();
    let mut ui = simulate(&none);
    assert!(ui.find("There are no channels to browse.").is_ok());
}
