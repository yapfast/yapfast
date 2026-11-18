//! The sliders driven through iced's simulator: a press picks a value, a drag
//! follows and the release keeps it, the keys move it once the slider has
//! them, and nothing moves it without them.

use iced::advanced::widget::operation::focusable;
use iced::keyboard::key::Named;
use iced::{Event, Point, Settings, Size, mouse};
use iced_test::Simulator;

use anomi_theme::{Element, Renderer, Theme};

use super::dial::Dial;
use super::sensitivity::{Sensitivity, sensitivity};
use super::track::{Axis, Span};
use super::volume::{Volume, volume};
use super::{level_bar, upright};

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Change(i32),
    Commit(i32),
}

type Ui<'a> = Simulator<'a, Msg, Theme, Renderer>;

const WIDE: Size = Size::new(300.0, 18.0);

fn dial<'a>(value: i32) -> Dial<'a, Msg> {
    Dial::new(Axis::Across, Span { min: 0, max: 200 }, value, Msg::Change)
        .step(5)
        .mark(Some(100))
        .on_commit(Msg::Commit)
}

fn simulate<'a>(size: Size, view: impl iced::Widget<Msg, Theme, Renderer> + 'a) -> Ui<'a> {
    Simulator::with_size(Settings::default(), size, view)
}

fn press() -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}

fn release() -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

fn moved(x: f32, y: f32) -> Event {
    Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(x, y),
    })
}

fn asked(ui: Ui<'_>) -> Vec<Msg> {
    ui.into_messages().collect()
}

#[test]
fn a_press_picks_the_value_under_it() {
    let mut ui = simulate(WIDE, dial(20));
    ui.point_at(Point::new(75.0, 9.0));
    ui.simulate([press()]);
    assert_eq!(asked(ui), [Msg::Change(50)]);
}

#[test]
fn a_drag_follows_and_the_release_keeps_the_last_value() {
    let mut ui = simulate(WIDE, dial(20));
    ui.point_at(Point::new(75.0, 9.0));
    // The pointer strays below and past the end: the value stays on the track.
    ui.simulate([press(), moved(155.0, 400.0), moved(900.0, 9.0), release()]);
    assert_eq!(
        asked(ui),
        [
            Msg::Change(50),
            // 155 of 300 is 103%: within four of the mark, it lands on it.
            Msg::Change(100),
            Msg::Change(200),
            Msg::Commit(200)
        ]
    );
}

#[test]
fn a_drag_snaps_to_the_mark_only_near_it() {
    let mut ui = simulate(WIDE, dial(20));
    ui.point_at(Point::new(150.0, 9.0));
    ui.simulate([press(), moved(180.0, 9.0), release()]);
    assert_eq!(
        asked(ui),
        [Msg::Change(100), Msg::Change(120), Msg::Commit(120)]
    );
}

#[test]
fn moving_without_a_press_does_nothing() {
    let mut ui = simulate(WIDE, dial(20));
    ui.point_at(Point::new(75.0, 9.0));
    ui.simulate([moved(100.0, 9.0), release()]);
    assert!(asked(ui).is_empty());
}

#[test]
fn the_keys_move_it_after_a_press_and_commit() {
    let mut ui = simulate(WIDE, dial(40));
    ui.point_at(Point::new(60.0, 9.0));
    // 60 of 300 is 40%, where it already stands: nothing to ask.
    ui.simulate([press(), release()]);
    assert!(ui.drain().all(|m| m == Msg::Commit(40)));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowRight));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowLeft));
    assert_eq!(
        asked(ui),
        [
            Msg::Change(45),
            Msg::Commit(45),
            // The view still shows 40: the left key steps from the 45 asked.
            Msg::Change(40),
            Msg::Commit(40)
        ]
    );
}

#[test]
fn a_batch_of_moves_compares_with_the_last_value_asked() {
    let mut ui = simulate(WIDE, dial(20));
    ui.point_at(Point::new(30.0, 9.0));
    ui.simulate([press(), moved(75.0, 9.0), moved(30.0, 9.0)]);
    assert_eq!(asked(ui), [Msg::Change(50), Msg::Change(20)]);
}

#[test]
fn a_batch_of_keys_steps_from_the_last_value_asked() {
    let mut ui = simulate(WIDE, dial(40).id("v"));
    ui.operate(&mut focusable::focus::<()>("v".into()));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowRight));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowRight));
    assert_eq!(
        asked(ui),
        [
            Msg::Change(45),
            Msg::Commit(45),
            Msg::Change(50),
            Msg::Commit(50)
        ]
    );
}

#[test]
fn the_keys_do_nothing_until_the_slider_has_them() {
    let mut ui = simulate(WIDE, dial(40));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowRight));
    assert!(asked(ui).is_empty());
}

#[test]
fn a_focus_operation_gives_it_the_keys_and_a_press_elsewhere_takes_them() {
    let mut ui = simulate(WIDE, dial(40).id("volume"));
    ui.operate(&mut focusable::focus::<()>("volume".into()));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowUp));
    // A press outside the slider lets the keys go.
    ui.point_at(Point::new(150.0, 400.0));
    ui.simulate([press()]);
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowUp));
    assert_eq!(asked(ui), [Msg::Change(45), Msg::Commit(45)]);
}

#[test]
fn keys_stop_at_the_ends() {
    let mut ui = simulate(WIDE, dial(0).id("v"));
    ui.operate(&mut focusable::focus::<()>("v".into()));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowLeft));
    // Already at 0: no new value is asked for, the commit keeps it.
    assert_eq!(asked(ui), [Msg::Commit(0)]);
}

#[test]
fn the_volume_slider_under_its_header_picks_by_its_track() {
    let view: Element<'_, Msg> = volume(Volume::new("Volume", 100), Msg::Change, Msg::Commit);
    let mut ui = simulate(Size::new(300.0, 80.0), view);
    // The header is a line tall, then 6 px, then the 18 px track.
    ui.point_at(Point::new(30.0, 34.0));
    ui.simulate([press(), release()]);
    assert_eq!(asked(ui), [Msg::Change(20), Msg::Commit(20)]);
}

#[test]
fn the_upright_slider_runs_from_the_bottom_and_pages_by_20() {
    let height = 116.0;
    let mut ui = simulate(Size::new(24.0, height), upright(50, Msg::Change));
    // Track from y 108 (0%) to y 8 (100%).
    ui.point_at(Point::new(12.0, 33.0));
    ui.simulate([press(), release()]);
    ui.tap_key(iced::keyboard::Key::Named(Named::PageDown));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowUp));
    // No commit: the upright one only asks, the player keeps the value.
    assert_eq!(
        asked(ui),
        [Msg::Change(75), Msg::Change(55), Msg::Change(60)]
    );
}

#[test]
fn the_sensitivity_slider_runs_from_minus_80_to_0() {
    let view: Element<'_, Msg> = sensitivity(
        Sensitivity {
            label: "Input sensitivity",
            value: -60,
            level: super::Reading::Fixed(-30.0),
            sending: false,
        },
        Msg::Change,
        Msg::Commit,
    );
    let mut ui = simulate(Size::new(320.0, 80.0), view);
    ui.point_at(Point::new(160.0, 34.0));
    ui.simulate([press(), release()]);
    assert_eq!(asked(ui), [Msg::Change(-40), Msg::Commit(-40)]);
}

#[test]
fn the_meter_and_the_sliders_draw() {
    let theme = Theme::default();
    let mut ui = simulate(Size::new(200.0, 8.0), level_bar(-10.0));
    ui.draw(&theme);
    let mut ui = simulate(WIDE, dial(150));
    ui.draw(&theme);
    let mut ui = simulate(Size::new(24.0, 100.0), upright(30, Msg::Change));
    ui.draw(&theme);
}

#[test]
fn a_drag_tells_when_it_starts_and_ends() {
    #[derive(Clone, Debug, PartialEq)]
    enum Held {
        Change(i32),
        Held(bool),
    }
    let view = Dial::new(Axis::Up, Span { min: 0, max: 100 }, 50, Held::Change).on_drag(Held::Held);
    let mut ui: Simulator<'_, Held, Theme, Renderer> =
        Simulator::with_size(Settings::default(), Size::new(24.0, 116.0), view);
    ui.point_at(Point::new(12.0, 33.0));
    ui.simulate([press(), release()]);
    let told: Vec<Held> = ui.into_messages().collect();
    assert_eq!(
        told,
        [Held::Held(true), Held::Change(75), Held::Held(false)]
    );
}

#[test]
fn a_value_put_back_from_outside_is_asked_for_again() {
    let mut ui = simulate(WIDE, dial(100));
    ui.point_at(Point::new(0.5, 9.0));
    ui.simulate([press(), release()]);
    let mut ui = ui.rebuild(dial(0)).rebuild(dial(100));
    ui.simulate([press(), release()]);
    assert_eq!(
        asked(ui),
        [
            Msg::Change(0),
            Msg::Commit(0),
            Msg::Change(0),
            Msg::Commit(0)
        ]
    );
}

#[test]
fn a_key_after_a_value_put_back_steps_from_what_shows() {
    let mut ui = simulate(WIDE, dial(100));
    ui.point_at(Point::new(0.5, 9.0));
    ui.simulate([press(), release()]);
    let mut ui = ui.rebuild(dial(0)).rebuild(dial(100));
    ui.tap_key(iced::keyboard::Key::Named(Named::ArrowRight));
    assert_eq!(&asked(ui)[2..], [Msg::Change(105), Msg::Commit(105)]);
}

#[test]
fn the_upright_slider_asks_for_zero_again_after_a_put_back() {
    let mut ui = simulate(Size::new(24.0, 116.0), upright(100, Msg::Change));
    ui.point_at(Point::new(12.0, 112.0));
    ui.simulate([press(), release()]);
    let mut ui = ui
        .rebuild(upright(0, Msg::Change))
        .rebuild(upright(100, Msg::Change));
    ui.simulate([press(), release()]);
    assert_eq!(asked(ui), [Msg::Change(0), Msg::Change(0)]);
}
