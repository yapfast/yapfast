//! A server's Invites settings: task X04's tests, driving
//! `anomi_settings::server::invites::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced::{Event, Rectangle, mouse};
use iced_test::selector::Candidate;
use iced_test::simulator::{self, Simulator};

use anomi_settings::server::invites::view;
use anomi_vm::fixtures::settings::invites as sample;
use anomi_vm::settings::invites::{Intent, Invites};

use super::support::{SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(page: &Invites) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

/// Where a cell's `words` are drawn.
fn words(ui: &mut Ui<'_>, words: &str) -> Rectangle {
    ui.find(|candidate: Candidate<'_>| match candidate {
        Candidate::Text {
            content, bounds, ..
        } if content == words => Some(bounds),
        _ => None,
    })
    .expect("the words are there")
}

/// The hovered row's Remove: an icon button, 32 px square, its name only
/// in its tooltip.
fn remove(ui: &mut Ui<'_>) -> Result<Rectangle, iced_test::Error> {
    ui.find(|candidate: Candidate<'_>| match candidate {
        Candidate::Container { bounds, .. } if bounds.width == 32.0 && bounds.height == 32.0 => {
            Some(bounds)
        }
        _ => None,
    })
}

#[test]
fn the_actions_ask_for_a_link_and_for_security_actions() {
    let page = sample::invites();
    let mut ui = simulate(&page);
    ui.click("Create Invite Link").expect("the button is there");
    ui.click("Pause Invites").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Create, Intent::SecurityActions]);
}

#[test]
fn a_paused_page_offers_to_enable_and_a_reader_may_not_create() {
    let mut page = sample::invites();
    page.paused = true;
    page.may_create = false;
    let mut ui = simulate(&page);
    assert!(ui.find("Create Invite Link").is_err());
    ui.click("Enable Invites").expect("the button is there");
    assert_eq!(asked(ui), [Intent::SecurityActions]);
}

#[test]
fn the_remove_button_shows_on_the_hovered_row_and_revokes_it() {
    let mut page = sample::invites();
    assert!(remove(&mut simulate(&page)).is_err());
    page.hovered = Some("carol1".into());
    let mut ui = simulate(&page);
    let at = remove(&mut ui).expect("the button is there").center();
    ui.point_at(at);
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Revoke("carol1".into())]);
}

#[test]
fn the_pointer_crossing_rows_names_each_it_enters_and_leaves() {
    let page = sample::invites();
    let mut ui = simulate(&page);
    let carol = words(&mut ui, "carol1").center();
    let bob = words(&mut ui, "bobinvite").center();
    for position in [carol, bob] {
        ui.point_at(position);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position })]);
    }
    assert_eq!(
        asked(ui),
        [
            Intent::Hover("carol1".into(), true),
            Intent::Hover("bobinvite".into(), true),
            Intent::Hover("carol1".into(), false),
        ]
    );
}

#[test]
fn the_countdowns_read_from_the_ticks() {
    let mut page = sample::invites();
    assert!(simulate(&page).find("05:23:59:58").is_ok());
    page.ticks = 60;
    assert!(simulate(&page).find("05:23:58:58").is_ok());
    assert!(simulate(&page).find("∞").is_ok());
}

#[test]
fn no_invites_says_so() {
    let page = Invites::default();
    assert!(simulate(&page).find("NO ACTIVE INVITE LINKS").is_ok());
}
