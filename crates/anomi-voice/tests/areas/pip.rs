//! A stream in picture in picture: task V06's tests, driving
//! `anomi_voice::pip::view` over its fixtures in the simulator
//! (`super::support`). The card is laid out alone at its own size.

use iced::widget::container;
use iced::{Event, Fill, Padding, Point, Size, Vector, mouse};
use iced_test::simulator::{self, Simulator};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::pip as sample;
use anomi_vm::voice::pip::{Intent, Pip};
use anomi_voice::pip::view;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(Pip::WIDTH, Pip::HEIGHT);
const MIDDLE: Point = Point::new(100.0, 90.0);
/// The stop button at the bottom right, 8 px in.
const STOP: Point = Point::new(Pip::WIDTH - 24.0, Pip::HEIGHT - 24.0);

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn simulate(pip: &Pip) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(pip, front()))
}

fn moved(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

fn press(ui: &mut Ui<'_>) {
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
}

fn release(ui: &mut Ui<'_>) {
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
}

#[test]
fn a_click_returns_to_the_call() {
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    moved(&mut ui, MIDDLE);
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Open]);
}

#[test]
fn a_press_that_hardly_moves_is_still_a_click() {
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    moved(&mut ui, MIDDLE);
    press(&mut ui);
    moved(&mut ui, Point::new(MIDDLE.x + 3.0, MIDDLE.y - 2.0));
    release(&mut ui);
    assert_eq!(asked(ui), [Intent::Open]);
}

#[test]
fn the_stop_button_shows_under_the_pointer_and_stops_without_opening() {
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    assert!(
        ui.find("Bob's screen").is_err(),
        "the title is hidden at rest"
    );
    moved(&mut ui, MIDDLE);
    assert!(ui.find("Bob's screen").is_ok());
    moved(&mut ui, STOP);
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Stop]);
}

#[test]
fn a_camera_has_nothing_to_stop() {
    let pip = sample::camera();
    let mut ui = simulate(&pip);
    moved(&mut ui, STOP);
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Open]);
}

#[test]
fn a_drag_asks_for_how_far_the_card_has_gone_then_drops_it() {
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    moved(&mut ui, MIDDLE);
    press(&mut ui);
    moved(&mut ui, Point::new(MIDDLE.x - 30.0, MIDDLE.y + 20.0));
    moved(&mut ui, Point::new(MIDDLE.x - 50.0, MIDDLE.y + 40.0));
    release(&mut ui);
    assert_eq!(
        asked(ui),
        [
            Intent::Drag(Vector::new(-30.0, 20.0)),
            Intent::Drag(Vector::new(-50.0, 40.0)),
            Intent::Dropped,
        ]
    );
}

#[test]
fn a_drag_that_comes_back_within_four_pixels_is_still_a_drag() {
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    moved(&mut ui, MIDDLE);
    press(&mut ui);
    moved(&mut ui, Point::new(MIDDLE.x + 10.0, MIDDLE.y));
    moved(&mut ui, MIDDLE);
    release(&mut ui);
    let asked = asked(ui);
    assert_eq!(asked.last(), Some(&Intent::Dropped));
    assert!(!asked.contains(&Intent::Open));
}

#[test]
fn the_title_hides_while_it_is_dragged() {
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    moved(&mut ui, MIDDLE);
    press(&mut ui);
    moved(&mut ui, Point::new(MIDDLE.x + 20.0, MIDDLE.y));
    assert!(ui.find("Bob's screen").is_err());
}

#[test]
fn what_it_waits_for_shows_until_the_first_picture() {
    let pip = sample::waiting();
    let mut ui = simulate(&pip);
    assert!(ui.find("Connecting to Bob's stream...").is_ok());
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    assert!(ui.find("Connecting to Bob's stream...").is_err());
}

#[test]
fn the_live_tag_and_the_viewers_show_over_it() {
    let pip = sample::sharing();
    let mut ui = simulate(&pip);
    assert!(ui.find("LIVE").is_ok());
    assert!(ui.find("1").is_ok());
    let pip = sample::watching();
    let mut ui = simulate(&pip);
    assert!(ui.find("LIVE").is_err());
}

/// The dragged card in a larger window, its own place at `(340, 40)`: it
/// draws `dragged().drag` from there, at `(100, 160)`.
fn lifted(pip: &Pip) -> Ui<'_> {
    let place = container(view(pip, front()).boxed())
        .padding(Padding::ZERO.left(340.0).top(40.0))
        .width(Fill)
        .height(Fill);
    Simulator::with_size(settings(), Size::new(700.0, 420.0), place)
}

const LIFTED: Point = Point::new(100.0 + 160.0, 160.0 + 90.0);

#[test]
fn it_draws_lifted_off_its_corner_while_dragged_and_answers_there() {
    let pip = sample::dragged();
    let mut ui = lifted(&pip);
    // Nothing answers at its own place, where it no longer is.
    moved(&mut ui, Point::new(340.0 + 160.0, 40.0 + 90.0));
    ui.simulate(simulator::click());
    moved(&mut ui, LIFTED);
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Open]);
}

#[test]
fn a_press_and_a_move_on_the_lifted_card_ask_for_its_offset_plus_the_move() {
    let pip = sample::dragged();
    let mut ui = lifted(&pip);
    moved(&mut ui, LIFTED);
    press(&mut ui);
    moved(&mut ui, Point::new(LIFTED.x + 10.0, LIFTED.y));
    assert_eq!(
        asked(ui),
        [Intent::Drag(
            sample::dragged().drag + Vector::new(10.0, 0.0)
        )]
    );
}
