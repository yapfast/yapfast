//! The voice dock: task V01's tests, driving `anomi_voice::dock::view` over
//! its fixtures in the simulator (`super::support`).

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::dock as fixtures;
use anomi_vm::voice::dock::{Dock, Intent};

use super::support::{asked, front, settings};

/// The dock as the account card gives it room: 359 px wide.
const WIDTH: f32 = 359.0;
const HEIGHT: f32 = 160.0;
/// The first row's buttons, from the right: hang up, then noise.
const LEAVE: Point = Point::new(335.0, 28.0);
const NOISE: Point = Point::new(303.0, 28.0);
/// The row of wide buttons under it, in thirds of 343 px from 8.
const WIDE_Y: f32 = 70.0;
const FIRST: f32 = 62.0;
const SECOND: f32 = 180.0;
const THIRD: f32 = 297.0;

fn dock(dock: &Dock) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(
        settings(),
        Size::new(WIDTH, HEIGHT),
        anomi_voice::dock::view(dock, front()),
    )
}

fn pressed(shown: &Dock, at: Point) -> Vec<Intent> {
    let mut ui = dock(shown);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

fn clicked(shown: &Dock, text: &str) -> Vec<Intent> {
    let mut ui = dock(shown);
    ui.click(text).unwrap();
    asked(ui)
}

#[test]
fn the_channel_line_opens_the_call() {
    let shown = fixtures::joined();
    assert_eq!(
        clicked(&shown, "Lounge · Secure connection"),
        [Intent::OpenCall]
    );
}

#[test]
fn hanging_up_leaves_and_waits_while_leaving() {
    let mut shown = fixtures::joined();
    assert_eq!(pressed(&shown, LEAVE), [Intent::Leave]);
    shown.leaving = true;
    assert!(pressed(&shown, LEAVE).is_empty());
}

#[test]
fn noise_suppression_toggles_where_discord_has_it() {
    let mut shown = fixtures::joined();
    assert_eq!(pressed(&shown, NOISE), [Intent::ToggleNoise]);
    // Without it the button isn't there: the point is the hang-up's.
    shown.can_noise = false;
    assert!(!pressed(&shown, NOISE).contains(&Intent::ToggleNoise));
}

#[test]
fn the_wide_buttons_follow_what_the_call_offers() {
    let shown = fixtures::joined();
    assert!(matches!(
        pressed(&shown, Point::new(FIRST, WIDE_Y)).as_slice(),
        [Intent::OpenSoundboard(_)]
    ));
    assert_eq!(
        pressed(&shown, Point::new(SECOND, WIDE_Y)),
        [Intent::ToggleCamera]
    );
    assert!(matches!(
        pressed(&shown, Point::new(THIRD, WIDE_Y)).as_slice(),
        [Intent::OpenShareMenu(_)]
    ));
    // A DM's call has no soundboard: the camera moves to the first place.
    let dm = Dock {
        can_soundboard: false,
        ..fixtures::joined()
    };
    assert_eq!(
        pressed(&dm, Point::new(FIRST, WIDE_Y)),
        [Intent::ToggleCamera]
    );
}

/// With no camera the button says so under the pointer, and a click still
/// asks for it: the app answers.
#[test]
fn the_camera_says_it_is_unavailable_and_still_asks() {
    let shown = fixtures::camera_unavailable();
    let at = Point::new(SECOND, WIDE_Y);
    let mut ui = dock(&shown);
    assert!(ui.find("Camera unavailable").is_err(), "before the pointer");
    hover(&mut ui, at);
    assert!(ui.find("Camera unavailable").is_ok());
    assert!(ui.find("Turn on camera").is_err());
    assert_eq!(pressed(&shown, at), [Intent::ToggleCamera]);
    let joined = fixtures::joined();
    let mut ui = dock(&joined);
    hover(&mut ui, at);
    assert!(
        ui.find("Turn on camera").is_ok(),
        "with a camera, as before"
    );
}

#[test]
fn nothing_needing_a_connection_works_before_it() {
    let shown = fixtures::joining();
    for x in [FIRST, SECOND, THIRD] {
        assert!(pressed(&shown, Point::new(x, WIDE_Y)).is_empty());
    }
    // But leaving is allowed.
    assert_eq!(pressed(&shown, LEAVE), [Intent::Leave]);
}

#[test]
fn the_screen_button_waits_for_the_stream_to_be_live() {
    let choosing = fixtures::choosing();
    assert!(!pressed_any_wide(&choosing));
    let live = fixtures::sharing();
    assert!(pressed_any_wide(&live));
}

/// Whether the third wide button of a sharing dock (it has two rows above)
/// opens the share menu.
fn pressed_any_wide(shown: &Dock) -> bool {
    let at = Point::new(THIRD, WIDE_Y + 44.0);
    pressed(shown, at)
        .iter()
        .any(|intent| matches!(intent, Intent::OpenShareMenu(_)))
}

#[test]
fn sharing_can_be_stopped_and_its_sound_turned_off() {
    let mut shown = fixtures::sharing();
    // "Your screen" sits on its own row, its buttons at the right.
    let stop = Point::new(335.0, 28.0 + 44.0);
    let sound = Point::new(303.0, 28.0 + 44.0);
    assert_eq!(pressed(&shown, stop), [Intent::StopSharing]);
    assert_eq!(pressed(&shown, sound), [Intent::ToggleStreamSound]);
    shown.share_sound_on = false;
    assert_eq!(pressed(&shown, sound), [Intent::ToggleStreamSound]);
}

#[test]
fn an_error_is_dismissed_by_its_button_or_by_its_cross() {
    // With no call, "Dismiss" under the notice closes the dock.
    let shown = fixtures::failed();
    assert_eq!(
        pressed(&shown, Point::new(90.0, 92.0)),
        [Intent::DismissError]
    );
    // In a call the notice has a cross instead.
    let during = fixtures::failed_in_call();
    assert_eq!(
        pressed(&during, Point::new(334.0, 110.0)),
        [Intent::DismissError]
    );
}

#[test]
fn a_warning_has_no_way_out_of_its_own() {
    let shown = fixtures::warned();
    let mut ui = dock(&shown);
    assert!(ui.find("Bob: packets are being lost").is_ok());
    assert!(ui.find("Dismiss").is_err());
}

#[test]
fn the_connection_opens_once_it_is_measured() {
    let shown = fixtures::joined();
    let mut ui = dock(&shown);
    ui.point_at(Point::new(28.0, 22.0));
    ui.simulate(iced_test::simulator::click());
    let intents = asked(ui);
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::OpenLink(_)))
    );
    let joining = fixtures::joining();
    assert!(
        !pressed(&joining, Point::new(28.0, 22.0))
            .iter()
            .any(|intent| matches!(intent, Intent::OpenLink(_)))
    );
}

#[test]
fn the_text_says_what_the_call_is_doing() {
    let shown = fixtures::joined();
    let mut ui = dock(&shown);
    assert!(ui.find("0:02").is_ok());
    let sharing = fixtures::sharing();
    let mut ui = dock(&sharing);
    assert!(ui.find("Your screen").is_ok());
    assert!(ui.find("LIVE").is_ok());
    assert!(ui.find("Streaming to Lounge · 3 watching").is_ok());
}

#[test]
fn pointing_at_the_status_reads_the_link_once_measured() {
    let measured = fixtures::joined();
    let mut ui = dock(&measured);
    hover(&mut ui, Point::new(100.0, 20.0));
    assert_eq!(asked(ui), [Intent::ReadLink]);

    let joining = fixtures::joining();
    let mut ui = dock(&joining);
    hover(&mut ui, Point::new(100.0, 20.0));
    assert!(asked(ui).is_empty());
}

/// Moves the pointer onto `at`: the cursor and the move event that tells
/// the widgets under it.
fn hover(ui: &mut Simulator<'_, Intent, Theme, Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
}
