//! The tooltips of the faces in the simulator: the card shows beside each
//! widget on the side it places it, a click hides it until the pointer leaves
//! and returns, a disabled face shows none, and the caret shows none while
//! its menu is open, nor when its menu closes under a pointer that has not
//! moved.

use iced::widget::container;
use iced::{Event, Point, Rectangle, Size, mouse};
use iced_test::Simulator;
use iced_test::simulator::click as click_events;

use anomi_theme::{Motion, Theme};

use super::tests::{Ui, rendered};
use super::*;
use crate::icon::Icon;
use crate::tip::Side;

/// The widget sits 60 px in from the window's corner: room for a card on
/// every side.
const AT: f32 = 60.0;

fn still() -> Motion {
    Motion::default()
}

fn show<'a>(widget: impl iced::Widget<u8, Theme, anomi_theme::Renderer> + 'a) -> Ui<'a> {
    Simulator::with_size(
        iced::Settings::default(),
        Size::new(300.0, 200.0),
        container(widget).padding(AT),
    )
}

fn move_to(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

fn away(ui: &mut Ui<'_>) {
    move_to(ui, Point::new(290.0, 190.0));
}

/// The card's bounds, if one shows.
fn card(ui: &mut Ui<'_>, label: &str) -> Option<Rectangle> {
    ui.find(label).ok().and_then(|found| found.visible_bounds())
}

#[test]
fn an_icon_button_shows_its_name_above_it() {
    let mut ui = show(icon_button::<u8>(Icon::Mic, "Mute", still()).view());
    assert!(card(&mut ui, "Mute").is_none());
    move_to(&mut ui, Point::new(AT + 16.0, AT + 16.0));
    let card = card(&mut ui, "Mute").expect("the tooltip");
    // 6 px over the 32 px face, centred on it.
    assert_eq!(card.y + card.height, AT - 6.0);
    assert!((card.center_x() - (AT + 16.0)).abs() <= 0.5, "{card:?}");
}

#[test]
fn an_icon_button_can_put_its_card_to_the_right() {
    let face = icon_button::<u8>(Icon::Mic, "Mute", still()).tip_side(Side::Right);
    let mut ui = show(face.view());
    move_to(&mut ui, Point::new(AT + 16.0, AT + 16.0));
    let card = card(&mut ui, "Mute").expect("the tooltip");
    assert_eq!(card.x, AT + 32.0 + 10.0);
    assert!((card.center_y() - (AT + 16.0)).abs() <= 0.5, "{card:?}");
}

#[test]
fn a_header_toggle_shows_its_name_below_it() {
    let toggle = header_toggle::<u8>(Icon::Users, "Members", "Show members", false, still());
    let mut ui = show(toggle.view());
    move_to(&mut ui, Point::new(AT + 10.0, AT + 16.0));
    let card = card(&mut ui, "Show members").expect("the tooltip");
    assert_eq!(card.y, AT + 32.0 + 6.0);
}

#[test]
fn the_caret_shows_its_name_above_it_and_the_toggle_its_own() {
    let pair = || device_toggle::<u8>(Icon::Mic, "Mute", "Input device", still()).view();
    let mut ui = show(pair());
    move_to(&mut ui, Point::new(AT + 42.0, AT + 16.0));
    let card = card(&mut ui, "Input device").expect("the caret's tooltip");
    assert_eq!(card.y + card.height, AT - 6.0);
    let mut ui = show(pair());
    move_to(&mut ui, Point::new(AT + 16.0, AT + 16.0));
    assert!(card_of(&mut ui, "Mute"));
    assert!(!card_of(&mut ui, "Input device"));
}

fn card_of(ui: &mut Ui<'_>, label: &str) -> bool {
    card(ui, label).is_some()
}

#[test]
fn a_click_hides_the_card_until_the_pointer_leaves_and_returns() {
    let mut ui = show(
        icon_button::<u8>(Icon::Mic, "Mute", still())
            .on_press(1)
            .view(),
    );
    let on = Point::new(AT + 16.0, AT + 16.0);
    move_to(&mut ui, on);
    assert!(card_of(&mut ui, "Mute"));
    ui.simulate(click_events());
    assert!(!card_of(&mut ui, "Mute"));
    // Moving on the face does not bring it back.
    move_to(&mut ui, Point::new(AT + 10.0, AT + 10.0));
    assert!(!card_of(&mut ui, "Mute"));
    away(&mut ui);
    move_to(&mut ui, on);
    assert!(card_of(&mut ui, "Mute"));
}

#[test]
fn a_disabled_face_shows_no_card() {
    let off = icon_button::<u8>(Icon::Mic, "Mute", still()).enabled(false);
    let mut ui = show(off.view());
    move_to(&mut ui, Point::new(AT + 16.0, AT + 16.0));
    assert!(!card_of(&mut ui, "Mute"));
}

#[test]
fn the_caret_shows_none_while_its_menu_is_open() {
    let open = device_toggle::<u8>(Icon::Mic, "Mute", "Input device", still()).open(true);
    let mut ui = show(open.view());
    move_to(&mut ui, Point::new(AT + 42.0, AT + 16.0));
    assert!(!card_of(&mut ui, "Input device"));
}

/// The colour of the pixel at `at` in a shot of the window.
fn pixel(ui: &mut Ui<'_>, at: Point) -> [u8; 3] {
    let image = rendered(ui, "tip");
    let scale = image.width() as f32 / 300.0;
    let p = image
        .get_pixel((at.x * scale) as u32, (at.y * scale) as u32)
        .0;
    [p[0], p[1], p[2]]
}

#[test]
fn a_menu_that_closes_under_a_still_pointer_leaves_no_card_not_even_a_blank_one() {
    let pair = |open| {
        device_toggle::<u8>(Icon::Mic, "Mute", "Input device", still())
            .open(open)
            .view()
    };
    let mut ui = show(pair(true));
    move_to(&mut ui, Point::new(AT + 42.0, AT + 16.0));
    let mut ui = ui.rebuild(container(pair(false)).padding(AT));
    // The card would sit 6 px over the caret, 40 px wide at least.
    assert!(!card_of(&mut ui, "Input device"));
    let card_spot = Point::new(AT + 42.0, AT - 6.0 - 16.0);
    let ground = pixel(&mut ui, Point::new(290.0, 10.0));
    assert_eq!(pixel(&mut ui, card_spot), ground, "a card is drawn");

    // Once the pointer has left and come back, the card shows as usual.
    away(&mut ui);
    move_to(&mut ui, Point::new(AT + 42.0, AT + 16.0));
    assert!(card_of(&mut ui, "Input device"));
}
