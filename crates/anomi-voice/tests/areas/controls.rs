//! The call controls: task V05's tests, driving `anomi_voice::controls::view`
//! over its fixtures in the simulator (`super::support`).
//!
//! The buttons carry no text, so a click goes by position: the pill is as
//! wide as its parts, centred, its row 8 px from the top.

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;
use iced_test::simulator::click;

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::controls as samples;
use anomi_vm::voice::controls::{Controls, Intent};
use anomi_voice::controls::{self, BUTTON, DIVIDER, LEAVE, PAD, SPACING};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(600.0, 100.0);
/// The pill's row: 16 px of room on top of 8, then its padding.
const ROW_TOP: f32 = 8.0 + PAD;

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn simulate(vm: &Controls) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, controls::view(vm, front()))
}

/// The centres of the pill's parts, left to right, given their widths.
fn centres(widths: &[f32]) -> Vec<f32> {
    let total = widths.iter().sum::<f32>() + SPACING * (widths.len() - 1) as f32 + 2.0 * PAD;
    let mut x = (SIZE.width - total) / 2.0 + PAD;
    widths
        .iter()
        .map(|width| {
            let centre = x + width / 2.0;
            x += width + SPACING;
            centre
        })
        .collect()
}

/// The parts of a call that offers everything: mute, deafen, divider,
/// soundboard, camera, share, divider, leave.
fn full() -> Vec<f32> {
    centres(&[
        BUTTON, BUTTON, DIVIDER, BUTTON, BUTTON, BUTTON, DIVIDER, LEAVE,
    ])
}

fn press(ui: &mut Ui<'_>, x: f32) {
    let at = Point::new(x, ROW_TOP + BUTTON / 2.0);
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    ui.simulate(click());
}

#[test]
fn each_button_asks_for_its_own_thing() {
    let vm = samples::joined();
    let mut ui = simulate(&vm);
    assert!(ui.find("Leave voice").is_err());
    let x = full();
    for part in [0, 1, 4, 7] {
        press(&mut ui, x[part]);
    }
    assert_eq!(
        asked(ui),
        [
            Intent::ToggleMute,
            Intent::ToggleDeafen,
            Intent::ToggleCamera,
            Intent::Leave
        ]
    );
}

/// With no camera the button still asks for it: the app answers.
#[test]
fn an_unavailable_camera_still_asks() {
    let vm = samples::camera_unavailable();
    let mut ui = simulate(&vm);
    press(&mut ui, full()[4]);
    assert_eq!(asked(ui), [Intent::ToggleCamera]);
}

#[test]
fn the_soundboard_and_the_share_menu_open_at_their_buttons() {
    let vm = samples::joined();
    let mut ui = simulate(&vm);
    let x = full();
    press(&mut ui, x[3]);
    press(&mut ui, x[5]);
    let asked = asked(ui);
    assert!(matches!(asked[0], Intent::OpenSoundboard(_)), "{asked:?}");
    assert!(matches!(asked[1], Intent::OpenShareMenu(_)), "{asked:?}");
}

#[test]
fn a_click_on_a_divider_or_beside_the_pill_asks_for_nothing() {
    let vm = samples::joined();
    let mut ui = simulate(&vm);
    let x = full();
    press(&mut ui, x[2]);
    press(&mut ui, x[6]);
    press(&mut ui, 10.0);
    assert!(asked(ui).is_empty());
}

#[test]
fn sharing_stops_with_one_click_instead_of_opening_a_menu() {
    let vm = samples::live();
    let mut ui = simulate(&vm);
    press(&mut ui, full()[5]);
    assert_eq!(asked(ui), [Intent::StopSharing]);
}

#[test]
fn a_dm_call_has_the_full_pill() {
    let vm = samples::dm_call();
    let mut ui = simulate(&vm);
    let x = full();
    press(&mut ui, x[3]);
    press(&mut ui, x[4]);
    press(&mut ui, x[5]);
    let asked = asked(ui);
    assert!(matches!(asked[0], Intent::OpenSoundboard(_)), "{asked:?}");
    assert_eq!(asked[1], Intent::ToggleCamera);
    assert!(matches!(asked[2], Intent::OpenShareMenu(_)), "{asked:?}");
}

#[test]
fn a_call_without_a_soundboard_has_no_button_for_it() {
    let vm = Controls {
        can_soundboard: false,
        ..samples::joined()
    };
    let mut ui = simulate(&vm);
    // Mute, deafen, divider, camera, share, divider, leave.
    let x = centres(&[BUTTON, BUTTON, DIVIDER, BUTTON, BUTTON, DIVIDER, LEAVE]);
    press(&mut ui, x[3]);
    press(&mut ui, x[4]);
    let asked = asked(ui);
    assert_eq!(asked[0], Intent::ToggleCamera);
    assert!(matches!(asked[1], Intent::OpenShareMenu(_)), "{asked:?}");
}

#[test]
fn a_call_without_video_or_sharing_has_neither_button() {
    let vm = Controls {
        can_film: false,
        can_share: false,
        ..samples::joined()
    };
    let mut ui = simulate(&vm);
    let x = centres(&[BUTTON, BUTTON, DIVIDER, BUTTON, DIVIDER, LEAVE]);
    press(&mut ui, x[3]);
    press(&mut ui, x[5]);
    let asked = asked(ui);
    assert!(matches!(asked[0], Intent::OpenSoundboard(_)), "{asked:?}");
    assert_eq!(asked[1], Intent::Leave);
}

#[test]
fn nothing_but_leaving_answers_before_the_connection_is_up() {
    let vm = samples::joining();
    let mut ui = simulate(&vm);
    let x = full();
    for part in [0, 1, 3, 4, 5] {
        press(&mut ui, x[part]);
    }
    assert!(ui.find("Mute").is_err());
    press(&mut ui, x[7]);
    assert_eq!(asked(ui), [Intent::Leave]);
}

#[test]
fn leaving_waits_for_the_hang_up_to_finish() {
    let vm = samples::leaving();
    let mut ui = simulate(&vm);
    press(&mut ui, full()[7]);
    press(&mut ui, full()[0]);
    assert_eq!(asked(ui), [Intent::ToggleMute]);
}

#[test]
fn the_tooltips_name_what_a_click_would_do() {
    for (vm, names) in [
        (
            samples::joined(),
            ["Mute", "Deafen", "Open Soundboard", "Turn on camera"],
        ),
        (
            samples::silenced(),
            ["Unmute", "Undeafen", "Open Soundboard", "Turn on camera"],
        ),
        (
            samples::starting_camera(),
            ["Mute", "Deafen", "Open Soundboard", "Starting camera..."],
        ),
        (
            samples::camera_unavailable(),
            ["Mute", "Deafen", "Open Soundboard", "Camera unavailable"],
        ),
    ] {
        let x = full();
        for (name, part) in names.into_iter().zip([0, 1, 3, 4]) {
            let mut ui = simulate(&vm);
            assert!(ui.find(name).is_err(), "{name} before the pointer comes");
            let at = Point::new(x[part], ROW_TOP + BUTTON / 2.0);
            ui.point_at(at);
            ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
            assert!(ui.find(name).is_ok(), "{name}");
        }
    }
}

#[test]
fn a_camera_on_and_sharing_say_how_to_turn_them_off() {
    let vm = samples::live();
    let mut ui = simulate(&vm);
    let x = full();
    for (part, name) in [(4, "Turn off camera"), (5, "Stop sharing")] {
        let at = Point::new(x[part], ROW_TOP + BUTTON / 2.0);
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
        assert!(ui.find(name).is_ok(), "{name}");
    }
}
