//! A profile's badges: task P08's tests, driving
//! `anomi_people::profile::badges::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Event, Point, mouse};
use iced_test::Simulator;

use anomi_people::profile::badges::view;
use anomi_theme::Theme;
use anomi_vm::fixtures::people::badges::{few, many, waiting};
use anomi_vm::people::badges::{Badges, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(badges: &Badges) -> Simulator<'_, Intent, Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(badges, front()))
}

/// The middle of the `n`th badge of the first row: 1px edge and 3px inside,
/// 22px a badge, 20px each, 1px above.
fn badge(n: usize) -> Point {
    Point::new(4.0 + 22.0 * n as f32 + 10.0, 2.0 + 10.0)
}

const BRAVERY: &str = "https://support.discord.com/hc/en-us/articles/360035962891";

fn click(badges: &Badges, at: Point) -> Vec<Intent> {
    let mut ui = simulate(badges);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn a_badge_with_a_page_opens_it() {
    assert_eq!(
        click(&few(), badge(0)),
        vec![Intent::OpenLink(BRAVERY.to_owned())]
    );
}

#[test]
fn a_badge_without_a_page_asks_for_nothing() {
    assert!(click(&few(), badge(1)).is_empty());
    assert!(click(&few(), badge(2)).is_empty());
}

#[test]
fn a_levelled_badge_opens_its_page_too() {
    let mut badges = many(3);
    badges.badges[0].link = "https://example.test/level".to_owned();
    assert_eq!(
        click(&badges, badge(0)),
        vec![Intent::OpenLink("https://example.test/level".to_owned())]
    );
}

#[test]
fn badges_whose_pictures_are_still_coming_keep_their_places() {
    // The place of the first still answers a click.
    assert_eq!(
        click(&waiting(), badge(0)),
        vec![Intent::OpenLink(BRAVERY.to_owned())]
    );
}

#[test]
fn the_thirteenth_badge_starts_a_second_row() {
    // Row two: the pill is 24px high and 4px below the first.
    let second = Point::new(4.0 + 10.0, 24.0 + 4.0 + 2.0 + 10.0);
    let mut badges = many(13);
    badges.badges[12].link = "https://example.test/13".to_owned();
    assert_eq!(
        click(&badges, second),
        vec![Intent::OpenLink("https://example.test/13".to_owned())]
    );
    // The first row holds twelve: the thirteenth is not beside them.
    assert!(click(&badges, badge(12)).is_empty());
}

#[test]
fn no_badges_draw_nothing_and_ask_nothing() {
    assert!(click(&Badges::default(), badge(0)).is_empty());
}

fn hovered(badges: &Badges, at: Point, shows: &str) -> bool {
    let mut ui = simulate(badges);
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    ui.find(shows).is_ok()
}

#[test]
fn a_levelled_badge_shows_its_card_on_hover() {
    let badges = many(2);
    assert!(hovered(&badges, badge(0), "EISERN"));
    assert!(hovered(&badges, badge(0), "Spielzeit"));
    assert!(hovered(&badges, badge(0), "1.816 Stunden gespielt"));
    // Not before the pointer is on it.
    assert!(!hovered(&badges, Point::new(900.0, 500.0), "EISERN"));
}

#[test]
fn a_plain_badge_says_what_it_is_on_hover() {
    assert!(hovered(&few(), badge(1), "Active Developer"));
    assert!(!hovered(&few(), badge(1), "HypeSquad Bravery"));
}
