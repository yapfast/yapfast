//! The tooltip in the simulator: it shows while the pointer is on what it
//! names, a click hides it until the pointer comes back, and nothing asks
//! for frames once it has arrived.

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
