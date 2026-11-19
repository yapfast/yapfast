//! The tooltip in the simulator: it shows while the pointer is on what it
//! names, a click or the window going behind another hides it until the
//! pointer comes back, and nothing asks for frames once it has arrived.

use iced::widget::{column, space, text};
use iced::{Event, Point, Settings, Size, mouse};
use iced_test::Simulator;

use anomi_theme::{Element, Motion, Renderer, Theme};

use super::{Side, tip};

fn view() -> Element<'static, ()> {
    use iced::Widget as _;
    column![
        tip(
            text("Inbox"),
            "Notifications",
            Side::Below,
            Motion::default()
        ),
        space().height(100),
    ]
    .boxed()
}

fn move_to(ui: &mut Simulator<'_, (), Theme, Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

#[test]
fn it_shows_on_hover_and_a_click_hides_it_until_the_pointer_returns() {
    let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 300.0), view());
    assert!(ui.find("Notifications").is_err());
    let on = ui
        .find("Inbox")
        .expect("Inbox")
        .visible_bounds()
        .expect("shown")
        .center();
    move_to(&mut ui, on);
    let card = ui.find("Notifications").expect("the tooltip");
    // Below what it names, 6 px under it.
    let inbox = ui.find("Inbox").unwrap().visible_bounds().unwrap();
    let shown = card.visible_bounds().expect("shown");
    assert_eq!(shown.y, inbox.y + inbox.height + 6.0);

    ui.simulate(iced_test::simulator::click());
    assert!(ui.find("Notifications").is_err());
    move_to(&mut ui, Point::new(300.0, 250.0));
    move_to(&mut ui, on);
    assert!(ui.find("Notifications").is_ok());
}

#[test]
fn behind_another_window_it_goes_until_the_pointer_returns() {
    let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 300.0), view());
    let inbox = ui.find("Inbox").unwrap().visible_bounds().unwrap();
    let on = inbox.center();
    move_to(&mut ui, on);
    assert!(ui.find("Notifications").is_ok());
    ui.simulate([Event::Window(iced::window::Event::Unfocused)]);
    assert!(ui.find("Notifications").is_err());
    // The pointer still on it brings nothing back while it's behind.
    move_to(&mut ui, on);
    assert!(ui.find("Notifications").is_err());
    move_to(&mut ui, Point::new(300.0, 250.0));
    move_to(&mut ui, on);
    assert!(ui.find("Notifications").is_ok());
}

#[derive(Clone, Debug, PartialEq)]
enum Held {
    Left,
    Card,
    Face,
}

/// A chip whose card of its own takes the pointer, at the window's foot so
/// the card goes above it.
fn holding() -> Element<'static, Held> {
    use iced::Widget as _;
    let face = crate::menu::opener(text("Face")).on_press(|_| Held::Face);
    let card = iced::widget::mouse_area(iced::widget::row![face, text("Who reacted")])
        .on_release(Held::Card);
    column![
        space().height(200),
        super::tip_card(text("Chip"), card, Motion::default()).holding(Held::Left),
    ]
    .boxed()
}

fn redraw_after(ui: &mut Simulator<'_, Held, Theme, Renderer>, wait: std::time::Duration) {
    let at = std::time::Instant::now() + wait;
    ui.simulate([Event::Window(iced::window::Event::RedrawRequested(at))]);
}

#[test]
fn a_holding_card_stays_while_the_pointer_crosses_to_it_and_takes_its_press() {
    let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 300.0), holding());
    let chip = ui.find("Chip").unwrap().visible_bounds().unwrap();
    let to = |ui: &mut Simulator<'_, Held, Theme, Renderer>, at: Point| {
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    };
    to(&mut ui, chip.center());
    let card = ui
        .find("Who reacted")
        .expect("the card")
        .visible_bounds()
        .unwrap();
    // Off the chip, across the gap, onto the card: it stays.
    to(
        &mut ui,
        Point::new(card.center_x(), card.y + card.height + 3.0),
    );
    redraw_after(&mut ui, std::time::Duration::from_millis(50));
    to(&mut ui, card.center());
    redraw_after(&mut ui, std::time::Duration::from_millis(400));
    assert!(ui.find("Who reacted").is_ok(), "held on the card");
    ui.simulate(iced_test::simulator::click());
    let face = ui.find("Face").unwrap().visible_bounds().unwrap().center();
    to(&mut ui, face);
    ui.simulate(iced_test::simulator::click());
    // Off both, it goes once the grace has passed.
    to(&mut ui, Point::new(390.0, 10.0));
    redraw_after(&mut ui, std::time::Duration::from_millis(50));
    assert!(ui.find("Who reacted").is_ok(), "within the grace");
    redraw_after(&mut ui, std::time::Duration::from_millis(400));
    assert!(ui.find("Who reacted").is_err());
    let asked: Vec<_> = ui.into_messages().collect();
    assert_eq!(asked, [Held::Card, Held::Face, Held::Left]);
}

/// The rail's tooltips stay after a click, as Slint's.
#[test]
fn a_staying_tip_stays_after_a_click() {
    use iced::Widget as _;
    let view = column![tip(text("Rail"), "Server", Side::Right, Motion::default()).stays()].boxed();
    let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 300.0), view);
    let on = ui.find("Rail").unwrap().visible_bounds().unwrap().center();
    move_to(&mut ui, on);
    ui.simulate(iced_test::simulator::click());
    assert!(ui.find("Server").is_ok());
}
