//! The client's widgets around one child hand it on whole: what the child
//! shows over the window (a tooltip) shows through every one of them.

use std::cell::Cell;

use iced::{
    Color, Element, Point, Rectangle, Settings, Size,
    widget::{container, image::Handle, text, tooltip},
};
use iced_test::Simulator;

use crate::{
    app::Message,
    widgets::{
        Above, Autoscroll, AutoscrollLook, Below, Corner, CtrlWheel, Drags, Framed, RightPress,
        Scrub, Spot, Watch,
    },
};

/// A box with a tooltip under it.
fn tipped<'a>() -> Element<'a, Message> {
    let tip = container(text("the tip"))
        .padding(8)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            ..Default::default()
        });
    tooltip(
        container(text("box")).width(80).height(40),
        tip,
        tooltip::Position::Bottom,
    )
    .into()
}

#[test]
fn every_wrapper_shows_its_childs_tooltip() {
    let spot = Cell::new(Rectangle::default());
    let pressed = Cell::new(Point::ORIGIN);
    let picture = Handle::from_rgba(1, 1, vec![0; 4]);
    let look = AutoscrollLook {
        ground: Color::BLACK,
        edge: Color::BLACK,
        dot: Color::BLACK,
        up: picture.clone(),
        down: picture,
    };
    let wrapped: Vec<(&str, Element<'_, Message>)> = vec![
        ("Above", Above::new(tipped(), 4.0).into()),
        ("Below", Below::new(tipped(), 10.0).into()),
        ("Watch", Watch::new(tipped(), None, None).into()),
        (
            "RightPress",
            RightPress::new(tipped(), |_| Message::Menu(crate::menus::Event::Close)).into(),
        ),
        ("Corner", Corner::new(tipped(), 0.0, 0.0).into()),
        ("Spot", Spot::new(tipped(), &spot).into()),
        (
            "RightPress::spot",
            RightPress::spot(tipped(), &pressed).into(),
        ),
        (
            "Drags",
            Drags::new(tipped())
                .click(Message::Menu(crate::menus::Event::Close))
                .into(),
        ),
        (
            "CtrlWheel",
            CtrlWheel::new(
                tipped(),
                |_: &mut (), _| Some(Message::Menu(crate::menus::Event::Close)),
                |_| {},
            )
            .into(),
        ),
        ("Autoscroll", Autoscroll::new(tipped(), &[], look).into()),
        ("Framed", Framed::new(tipped(), Vec::new(), 0.0, 0.0).into()),
        (
            "Scrub",
            Scrub::new(tipped(), |_| Message::Menu(crate::menus::Event::Close)).into(),
        ),
    ];
    for (name, element) in wrapped {
        let mut ui = Simulator::with_size(
            Settings::default(),
            Size::new(300.0, 200.0),
            container(container(element).width(80)).padding(20),
        );
        // In the box, shifted or not; the tooltip comes under it.
        let over = Point::new(60.0, 45.0);
        let under = Rectangle::new(Point::new(20.0, 60.0), Size::new(200.0, 60.0));
        assert!(
            super::hover_draws(&mut ui, over, under),
            "{name}'s child's tooltip"
        );
    }
}

#[test]
fn a_held_scrub_seeks_again_only_where_it_moved_along() {
    let scrub = Scrub::new(container(text("wave")).width(80).height(40), |at| {
        Message::Menu(crate::menus::Event::Scrolled(at))
    });
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(300.0, 200.0),
        container(scrub).padding(20),
    );
    let moved = |ui: &mut Simulator<'_, Message>, to: Point| {
        ui.point_at(to);
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: to,
        })]);
    };
    moved(&mut ui, Point::new(40.0, 30.0));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
        iced::mouse::Button::Left,
    ))]);
    // Up and down along the same place, then across.
    moved(&mut ui, Point::new(40.0, 50.0));
    moved(&mut ui, Point::new(40.0, 25.0));
    moved(&mut ui, Point::new(80.0, 25.0));
    let seeks: Vec<f32> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::Menu(crate::menus::Event::Scrolled(at)) => Some(at),
            _ => None,
        })
        .collect();
    assert_eq!(seeks, [0.25, 0.75]);
}

#[test]
fn a_card_at_the_pointer_opens_below_it_while_it_fits() {
    use crate::widgets::{Anchor, Place};
    // 100 px tall at y 90: its foot at 190, inside the 8 px margin of a
    // 200 px window.
    let card: iced::widget::Container<'_, Message> = container(text("card")).width(100).height(100);
    let place = Place::new(card, Point::new(10.0, 90.0), 8.0).anchor(Anchor::Pointer);
    let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 200.0), place);
    let card = ui.find("card").expect("the card");
    assert_eq!(card.visible_bounds().expect("shown").y, 90.0);
}
