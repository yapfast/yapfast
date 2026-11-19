//! The dialog a click on the camera opens with no camera to film with
//! (`anomi_screens::shell::camera`), in the simulator (`super::support`):
//! what it says, and that "OK", Enter, Escape and a press beside it each
//! ask the shell to close its layer.

use iced::Point;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::key::Named;
use iced_test::Simulator;

use anomi_screens::shell::camera::{OK, dialog};
use anomi_vm::Intent;
use anomi_vm::fixtures::voice::dock as sample;
use anomi_vm::shell::{Intent as ShellIntent, Layer};

use super::support::{self, SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

/// The dialog alone, at the window's top-left.
fn simulate() -> Ui<'static> {
    Simulator::with_size(settings(), SIZE, dialog(front()))
}

fn closes() -> Intent {
    Intent::Shell(ShellIntent::Close(Layer::CameraUnavailable))
}

#[test]
fn it_says_no_camera_was_found() {
    let mut ui = simulate();
    assert!(ui.find("Camera unavailable").is_ok());
    assert!(ui.find("Oops, we couldn't find any camera.").is_ok());
}

/// "OK" focused, as the app focuses it as the dialog opens: Space presses
/// it, a key the dialog itself leaves alone.
#[test]
fn ok_closes_it() {
    let mut ui = simulate();
    let mut focus = focusable::focus::<()>(Id::new(OK));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(Named::Space);
    assert_eq!(asked(ui), [closes()]);
}

#[test]
fn enter_closes_it_with_nothing_focused() {
    let mut ui = simulate();
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [closes()]);
}

/// In the whole window, through its layer: Escape, then a press on the
/// dimmed window beside the card.
#[test]
fn escape_and_a_press_beside_it_close_it() {
    let vm = sample::camera_unavailable_window();
    let mut ui = support::simulate(&vm);
    ui.tap_key(Named::Escape);
    ui.point_at(Point::new(40.0, 600.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [closes(), closes()]);
}
