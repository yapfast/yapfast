//! A participant's menu: task V11's tests, driving
//! `anomi_voice::participant_menu::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_vm::fixtures::voice::participant_menu as fixture;
use anomi_vm::voice::participant_menu::{Intent, ParticipantMenu};
use anomi_voice::participant_menu::view;

use super::support;

fn card(
    menu: &ParticipantMenu,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    let size = Size::new(menu.width, menu.height());
    Simulator::with_size(support::settings(), size, view(menu, support::front()))
}

fn press() -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}

fn release() -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

/// A line of the slider under the name: its track.
const FIRST: f32 = 76.0;
/// The stream's, a slider and its spacing lower.
const SECOND: f32 = FIRST + 53.0;

#[test]
fn the_card_is_as_tall_as_the_view_model_says() {
    let menu = fixture::card();
    let ui = card(&menu);
    assert!(support::asked(ui).is_empty());
    assert_eq!(menu.height(), 97.0);
    let live = ParticipantMenu {
        live: true,
        ..menu.clone()
    };
    assert_eq!(live.height(), 150.0);
    // Joined, the rows start a pixel inside it; alone, where it started.
    assert_eq!(menu.rows_top(8.0), 104.0);
    let alone = ParticipantMenu {
        joined: false,
        ..menu
    };
    assert_eq!(alone.rows_top(8.0), 8.0);
}

#[test]
fn dragging_the_voice_slider_asks_for_their_voice_and_keeps_the_last() {
    let menu = fixture::card();
    let mut ui = card(&menu);
    ui.point_at(Point::new(menu.width - 14.0, FIRST));
    ui.simulate([press(), release()]);
    let asked = support::asked(ui);
    let voice = |volume, last| Intent::Volume {
        person: fixture::CAROL.into(),
        stream: false,
        volume,
        last,
    };
    // The track's far end, to within a step.
    assert_eq!(asked.first(), Some(&voice(198, false)));
    assert_eq!(asked.last(), Some(&voice(198, true)));
}

#[test]
fn a_person_sharing_their_screen_has_a_stream_slider_of_its_own() {
    let menu = ParticipantMenu {
        live: true,
        stream_volume: 60,
        ..fixture::card()
    };
    let mut ui = card(&menu);
    ui.point_at(Point::new(14.0, SECOND));
    ui.simulate([press(), release()]);
    let asked = support::asked(ui);
    assert!(!asked.is_empty());
    assert!(
        asked
            .iter()
            .all(|intent| matches!(intent, Intent::Volume { stream: true, .. }))
    );
}

#[test]
fn without_a_stream_the_lower_card_has_no_second_slider() {
    let menu = fixture::card();
    let mut ui = card(&menu);
    // Below the one slider, inside the card's padding: nothing to press.
    ui.point_at(Point::new(14.0, menu.height() - 4.0));
    ui.simulate([press(), release()]);
    assert!(support::asked(ui).is_empty());
}

#[test]
fn the_fixtures_put_the_card_and_its_rows_in_the_window() {
    let vm = fixture::carol();
    assert_eq!(vm.voice.participant_menu.name, "Carol");
    assert_eq!(vm.menu.rows.len(), 17);
    assert!(vm.shell.is_open(anomi_vm::shell::Layer::ParticipantMenu));
    assert!(fixture::sharing().voice.participant_menu.live);
}
