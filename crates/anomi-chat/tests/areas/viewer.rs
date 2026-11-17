//! The image viewer: task C10's tests, driving `anomi_chat::viewer::view`
//! over its fixtures in the simulator (`super::support`).

use iced::keyboard::{Key, key::Named};
use iced::{Event, Point, mouse};
use iced_test::Simulator;
use iced_test::simulator::click;

use anomi_chat::viewer::{menu, view};
use anomi_vm::chat::viewer::{Intent, Viewer, Zoom};
use anomi_vm::fixtures::chat::viewer as fixture;
use anomi_vm::menu::Spot;

use super::support::{self, SIZE, asked};

const CENTRE: Point = Point::new(700.0, 430.0);
const PREVIOUS: Point = Point::new(44.0, 430.0);
const NEXT: Point = Point::new(1356.0, 430.0);
const CLOSE: Point = Point::new(1372.0, 32.0);
const SAVE: Point = Point::new(1332.0, 32.0);
const EMPTY: Point = Point::new(700.0, 820.0);

fn ui(viewer: &Viewer) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(support::settings(), SIZE, view(viewer, support::front()))
}

fn clicked(viewer: &Viewer, at: Point) -> Vec<Intent> {
    let mut ui = ui(viewer);
    ui.point_at(at);
    ui.simulate(click());
    asked(ui)
}

fn right_clicked(viewer: &Viewer, at: Point) -> Vec<Intent> {
    let mut ui = ui(viewer);
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    asked(ui)
}

fn key(viewer: &Viewer, key: Key) -> Vec<Intent> {
    let mut ui = ui(viewer);
    ui.tap_key(key);
    asked(ui)
}

#[test]
fn the_arrows_step_through_the_pictures() {
    let viewer = fixture::several_viewer();
    assert_eq!(clicked(&viewer, PREVIOUS), [Intent::Step(-1)]);
    assert_eq!(clicked(&viewer, NEXT), [Intent::Step(1)]);
    assert_eq!(
        key(&viewer, Key::Named(Named::ArrowLeft)),
        [Intent::Step(-1)]
    );
    assert_eq!(
        key(&viewer, Key::Named(Named::ArrowRight)),
        [Intent::Step(1)]
    );
}

#[test]
fn a_message_with_one_picture_has_no_arrows() {
    let viewer = fixture::single_viewer();
    assert!(clicked(&viewer, NEXT).iter().all(|i| *i == Intent::Close));
    assert!(key(&viewer, Key::Named(Named::ArrowRight)).is_empty());
}

#[test]
fn close_and_a_press_outside_the_picture_close_it() {
    let viewer = fixture::several_viewer();
    assert_eq!(clicked(&viewer, CLOSE), [Intent::Close]);
    assert_eq!(clicked(&viewer, EMPTY), [Intent::Close]);
}

#[test]
fn save_asks_for_this_picture() {
    let viewer = fixture::several_viewer();
    let save = Intent::Save {
        item: viewer.item.clone(),
        index: 1,
    };
    assert_eq!(clicked(&viewer, SAVE), [save]);
}

#[test]
fn a_links_preview_has_no_save_button() {
    let viewer = fixture::preview_viewer();
    // Where Save would be, a press is outside everything: it closes.
    assert_eq!(clicked(&viewer, SAVE), [Intent::Close]);
}

#[test]
fn a_press_on_the_picture_zooms_and_does_not_close() {
    let viewer = fixture::several_viewer();
    assert_eq!(clicked(&viewer, CENTRE), [Intent::Zoom(Zoom::Toggle)]);
}

#[test]
fn the_keys_and_the_wheel_zoom() {
    let viewer = fixture::several_viewer();
    assert_eq!(
        key(&viewer, Key::Character("+".into())),
        [Intent::Zoom(Zoom::In)]
    );
    assert_eq!(
        key(&viewer, Key::Character("-".into())),
        [Intent::Zoom(Zoom::Out)]
    );
    assert_eq!(
        key(&viewer, Key::Character("0".into())),
        [Intent::Zoom(Zoom::Reset)]
    );
    let mut wheel = ui(&viewer);
    wheel.point_at(CENTRE);
    wheel.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 });
    assert_eq!(asked(wheel), [Intent::Zoom(Zoom::In)]);
}

#[test]
fn a_right_click_on_the_picture_asks_for_its_menu() {
    let viewer = fixture::several_viewer();
    assert_eq!(
        right_clicked(&viewer, CENTRE),
        [Intent::Menu(Spot::pointer(CENTRE))]
    );
}

#[test]
fn a_preview_without_an_address_has_no_menu() {
    let mut viewer = fixture::preview_viewer();
    viewer.link = None;
    assert!(right_clicked(&viewer, CENTRE).is_empty());
}

#[test]
fn the_menus_rows_follow_what_the_picture_is() {
    let texts = |viewer: &Viewer, video| -> Vec<String> {
        menu::rows(viewer, video)
            .into_iter()
            .map(|row| row.text)
            .collect()
    };
    let image = fixture::several_viewer();
    assert_eq!(
        texts(&image, false),
        ["Copy Image", "Save Image", "Copy Link", "Open Link"]
    );
    assert_eq!(
        texts(&image, true),
        ["Save Video", "Copy Link", "Open Link"]
    );
    assert_eq!(
        texts(&fixture::preview_viewer(), false),
        ["Copy Link", "Open Link"]
    );
    let mut bare = image;
    bare.link = None;
    assert_eq!(texts(&bare, false), ["Save Image"]);
}

#[test]
fn a_zoomed_picture_is_larger_than_the_fit_and_clipped_to_the_room() {
    // The fitted picture is 1171 wide, centred: x 114 to 1285. A press left
    // of it, still in the room (x 80 to 1320), misses it at the fit.
    let edge = Point::new(95.0, 430.0);
    assert_eq!(clicked(&fixture::several_viewer(), edge), [Intent::Close]);
    // At zoom 2 it fills the room, so the same press lands on it.
    let zoomed = fixture::zoomed_viewer();
    assert_eq!(clicked(&zoomed, edge), [Intent::Zoom(Zoom::Toggle)]);
    // Past the room's edge is the arrow's side, not the picture's.
    let single = Viewer {
        zoom: 2.0,
        ..fixture::single_viewer()
    };
    assert_eq!(clicked(&single, Point::new(40.0, 600.0)), [Intent::Close]);
}

#[test]
fn the_save_tooltip_is_the_view_models() {
    let viewer = fixture::several_viewer();
    assert_eq!(viewer.save_tip, "Save sunset.png");
}

#[test]
fn every_state_draws() {
    for viewer in [
        fixture::several_viewer(),
        fixture::single_viewer(),
        fixture::loading_viewer(),
        fixture::failed_viewer(),
        fixture::zoomed_viewer(),
        fixture::preview_viewer(),
        Viewer::default(),
    ] {
        let mut ui = ui(&viewer);
        ui.snapshot(&anomi_theme::Theme::default()).expect("draws");
    }
}

#[test]
fn the_menus_rows_send_the_viewers_intents_through_the_window() {
    let viewer = fixture::several_viewer();
    let rows = menu::rows(&viewer, false);
    let save = rows.iter().find(|row| row.text == "Save Image").unwrap();
    assert_eq!(
        save.intent,
        Some(anomi_vm::Intent::from(Intent::Save {
            item: viewer.item.clone(),
            index: 1,
        }))
    );
}
