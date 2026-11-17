//! The composer's suggestions: task C15's tests, driving
//! `anomi_chat::composer::suggestions::view` over its fixtures in the
//! simulator (`super::support`).

use iced::widget::{column, container, space};
use iced::{Fill, Point, Size};
use iced_test::{Simulator, simulator};

use anomi_chat::composer::suggestions::view;
use anomi_vm::chat::suggestions::{Intent, Offer, Suggestions};
use anomi_vm::fixtures::chat::suggestions as sample;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(760.0, 520.0);

/// Where the slot is: the list floats up from it, its bottom 8 px over it.
const SLOT: f32 = 420.0;

/// A click in the middle of the row whose top is `at` px into a frame
/// `frame` high (12 px of padding, a 24 px heading, rows of 36).
fn click(vm: &Suggestions, frame: f32, at: f32) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.point_at(Point::new(300.0, SLOT - 8.0 - frame + 6.0 + at + 18.0));
    ui.simulate(simulator::click());
    asked(ui)
}

fn simulate(vm: &Suggestions) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    // The list floats up from a slot of no height: put the slot low, as it
    // is over the composer.
    let page = column![space().height(Fill), view(vm, front()), space().height(100)]
        .width(Fill)
        .height(Fill);
    Simulator::with_size(settings(), SIZE, container(page))
}

#[test]
fn clicking_a_member_chooses_its_row() {
    let vm = sample::members();
    assert_eq!(click(&vm, 108.0, 24.0 + 36.0), [Intent::Choose(1)]);
}

#[test]
fn clicking_the_first_member_chooses_zero() {
    let vm = sample::members();
    assert_eq!(click(&vm, 108.0, 24.0), [Intent::Choose(0)]);
}

#[test]
fn members_and_roles_sit_under_their_headings() {
    let vm = sample::everything();
    let mut ui = simulate(&vm);
    assert!(ui.find("MEMBERS").is_ok());
    assert!(ui.find("ROLES").is_ok());
    assert_eq!(click(&vm, 204.0, 24.0 + 72.0 + 24.0), [Intent::Choose(2)]);
}

#[test]
fn a_group_says_what_it_reaches() {
    let vm = sample::everything();
    let mut ui = simulate(&vm);
    assert!(ui.find("Everyone in this channel").is_ok());
}

#[test]
fn channels_list_text_and_voice() {
    let vm = sample::channels();
    assert_eq!(click(&vm, 108.0, 24.0 + 36.0), [Intent::Choose(1)]);
}

#[test]
fn emoji_say_what_they_match_and_choose_by_row() {
    let vm = sample::emoji();
    let mut ui = simulate(&vm);
    assert!(ui.find("EMOJI MATCHING :par").is_ok());
    assert_eq!(click(&vm, 396.0, 24.0 + 72.0), [Intent::Choose(2)]);
}

#[test]
fn a_custom_emoji_is_the_first_row() {
    let vm = sample::emoji();
    assert_eq!(click(&vm, 396.0, 24.0), [Intent::Choose(0)]);
}

#[test]
fn nothing_offered_shows_nothing() {
    let vm = Suggestions::default();
    assert!(!vm.showing());
    let mut ui = simulate(&vm);
    assert!(ui.find("MEMBERS").is_err());
    assert!(asked(ui).is_empty());
}

#[test]
fn an_empty_offer_is_not_showing() {
    let vm = Suggestions {
        offer: Offer::Mentions {
            rows: Vec::new(),
            index: 0,
        },
    };
    assert!(!vm.showing());
    assert_eq!(vm.len(), 0);
}
