//! A message's embeds: task C05's tests, driving `anomi_chat::embeds::view`
//! over its fixtures in the simulator (`super::support`).

use iced::mouse::{Button, Event as Mouse};
use iced::{Event, Point, Size};
use iced_test::{Simulator, simulator};

use anomi_chat::embeds::view;
use anomi_vm::chat::embeds::{Embeds, Intent};
use anomi_vm::fixtures::chat::embeds as sample;
use anomi_vm::text::Link;

use super::support;

const ROOM: Size = Size::new(700.0, 600.0);

fn ui(embeds: &Embeds) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(support::settings(), ROOM, view(embeds, support::front()))
}

fn click(embeds: &Embeds, at: Point) -> Vec<Intent> {
    let mut ui = ui(embeds);
    ui.point_at(at);
    ui.simulate([Event::Mouse(Mouse::CursorMoved { position: at })]);
    ui.simulate(simulator::click());
    support::asked(ui)
}

#[test]
fn every_sample_lays_out() {
    for embeds in [
        sample::article(),
        sample::status(),
        sample::youtube(),
        sample::gallery(),
    ] {
        let mut ui = ui(&embeds);
        // A card takes room: a click far below it asks nothing.
        ui.point_at(Point::new(300.0, 590.0));
        ui.simulate(simulator::click());
        assert!(support::asked(ui).is_empty());
    }
}

#[test]
fn a_cards_thumbnail_opens_in_the_viewer_and_its_menu_asks_where() {
    let embeds = sample::status();
    // 80 px square at the card's top right, inside its 16 px padding.
    let on_it = Point::new(432.0 - 16.0 - 40.0, 4.0 + 12.0 + 40.0);
    assert_eq!(
        click(&embeds, on_it),
        vec![Intent::View {
            item: embeds.item,
            index: 0
        }]
    );
    let mut ui = ui(&embeds);
    ui.point_at(on_it);
    ui.simulate([
        Event::Mouse(Mouse::ButtonPressed(Button::Right)),
        Event::Mouse(Mouse::ButtonReleased(Button::Right)),
    ]);
    let asked = support::asked(ui);
    assert!(matches!(
        asked.as_slice(),
        [Intent::Menu {
            index: 0,
            video: false,
            ..
        }]
    ));
}

#[test]
fn a_title_link_opens_its_page() {
    let embeds = sample::status();
    let asked = click(&embeds, Point::new(40.0, 4.0 + 12.0 + 10.0));
    assert_eq!(
        asked,
        vec![Intent::Link(Link::Url("https://status.example.com".into()))]
    );
}

#[test]
fn a_videos_poster_opens_its_page_not_the_viewer() {
    let embeds = sample::youtube();
    let asked = click(&embeds, Point::new(200.0, 220.0));
    assert_eq!(asked, vec![Intent::Open(embeds.cards[0].url.clone())]);
}

#[test]
fn a_spoiler_is_lifted_by_a_click_and_a_picture_on_its_way_still_opens() {
    let embeds = sample::gallery();
    let asked = click(&embeds, Point::new(100.0, 90.0));
    assert_eq!(
        asked,
        vec![Intent::Reveal {
            item: embeds.item,
            index: 0
        }]
    );
    assert_eq!(
        click(&embeds, Point::new(16.0 + 184.0 + 80.0, 90.0)),
        vec![Intent::View {
            item: embeds.item,
            index: 1
        }]
    );
}

#[test]
fn a_card_without_embeds_takes_no_room() {
    let none = Embeds::default();
    assert!(none.is_empty());
    assert!(click(&none, Point::new(10.0, 10.0)).is_empty());
}
