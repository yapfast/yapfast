//! The Sounds settings page: task S05's tests, driving
//! `anomi_settings::sounds::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Point;
use iced::Widget as _;
use iced_test::simulator::{Simulator, click};

use anomi_settings::sounds::view;
use anomi_vm::fixtures::settings::sounds as sample;
use anomi_vm::settings::sounds::{Intent, Sounds};

use super::support::{SIZE, asked, front, settings};

/// A slider's height: its label's line, the gap, the track (`header::GAP`,
/// `metrics::SLIDER_HEIGHT`).
const ROW: f32 = 18.0 + 6.0 + TRACK;
const TRACK: f32 = 18.0;

fn simulate(page: &Sounds) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn the_list_of_sounds_shows_only_when_asked() {
    let page = sample::page();
    assert!(
        simulate(&page.settings.sounds)
            .find("Someone joins the call")
            .is_err()
    );
    let page = sample::each();
    assert!(
        simulate(&page.settings.sounds)
            .find("Someone joins the call")
            .is_ok()
    );
}

#[test]
fn the_switch_asks_for_a_slider_each() {
    let page = sample::page();
    let mut ui = simulate(&page.settings.sounds);
    ui.click("Adjust each sound").expect("the switch is there");
    assert_eq!(asked(ui), [Intent::AdjustEach(true)]);
}

#[test]
fn every_sound_has_a_button_that_plays_it() {
    let page = sample::each();
    let mut ui = simulate(&page.settings.sounds);
    // A message's play button: an icon its tooltip names, which is no text to
    // find, at the end of the row its slider's label starts.
    let label = ui.find("A message").expect("the sound's slider").bounds();
    ui.point_at(Point::new(SIZE.width - 16.0, label.y + ROW / 2.0));
    ui.simulate(click());
    assert_eq!(asked(ui), [Intent::Play(6)]);
}

#[test]
fn a_set_picked_from_the_list_is_asked_for() {
    let page = sample::page();
    let mut ui = simulate(&page.settings.sounds);
    ui.click("New Kalimba").expect("the picker shows the set");
    ui.click("Glow").expect("a set in the open list");
    assert_eq!(asked(ui), [Intent::Set(4)]);
}

#[test]
fn letting_go_of_a_slider_settles_it() {
    let page = sample::each();
    for (label, index) in [("Volume", None), ("Muting", Some(2))] {
        let mut ui = simulate(&page.settings.sounds);
        let at = ui.find(label).expect("the slider's label").bounds();
        // The track runs under the label, across the page but the play button.
        ui.point_at(Point::new(SIZE.width / 4.0, at.y + ROW - TRACK / 2.0));
        ui.simulate(click());
        let asked = asked(ui);
        let settled = match (index, &asked[..]) {
            (None, [Intent::Volume(a), Intent::VolumeSettled(b)]) => a == b,
            (Some(i), [Intent::Each(j, a), Intent::EachSettled(k, b)]) => {
                i == *j && i == *k && a == b
            }
            _ => false,
        };
        assert!(settled, "{label}: {asked:?}");
    }
}
