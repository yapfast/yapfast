//! Where a layer at a spot goes in the window: a header's popover too tall
//! for the room under its button shrinks to it, 16 px clear of the bottom as
//! it asks; any other card keeps only the margin there.

use iced::Widget as _;
use iced::advanced::widget::Id;
use iced::widget::{scrollable, text};
use iced::{Length, Point, Rectangle, Size};

use anomi_theme::Element;
use anomi_vm::menu::Spot;

use super::super::{Place, layer, layers};
use super::{Asked, base, front, simulate};
use crate::menu::place::HEADER_CLEARANCE;

/// Forty pins, 20 px each, scrolling in a card 120 px wide.
fn pins(height: impl Into<Length>) -> Element<'static, Asked> {
    let column = iced::widget::column((0..40).map(|n| text(format!("pin {n}")).height(20).boxed()));
    scrollable(column)
        .id(Id::new("pins"))
        .height(height)
        .width(120)
        .boxed()
}

#[test]
fn a_popover_too_tall_for_the_room_under_its_button_shrinks_to_it() {
    let button = Rectangle::new(Point::new(10.0, 10.0), Size::new(30.0, 30.0));
    let view = layers(base(), front())
        .push(
            layer("pins", pins(Length::Fill))
                .place(Place::At(Spot::below(button)))
                .bottom_clearance(HEADER_CLEARANCE),
        )
        .boxed();
    let mut ui = simulate(view);
    let first = ui.find("pin 0").expect("the first pin");
    // Under the button, 6 px down, rather than moved up over it.
    assert_eq!(first.visible_bounds().expect("shown").y, 46.0);
    // As tall as the room under it, 16 px clear of the window's bottom as
    // in Slint (`root.height - self.y - 16px`): 300 - 46 - 16 = 238 px.
    let pins = ui.find(Id::new("pins")).expect("the popover").bounds();
    assert_eq!((pins.y, pins.height), (46.0, 238.0));
    assert_eq!(pins.y + pins.height, 284.0);
    assert!(
        ui.find("pin 30")
            .expect("a later pin")
            .visible_bounds()
            .is_none()
    );
}

#[test]
fn a_card_under_a_mention_keeps_only_the_margin_under_it() {
    // An emoji's card under its mention: Slint opens it below while
    // 96 + 4 + 190 = 290 <= 300 - 8 (`ui/yapfast.slint:5139`), whole.
    let mention = Rectangle::new(Point::new(10.0, 66.0), Size::new(30.0, 30.0));
    let view = layers(base(), front())
        .push(layer("card", pins(190.0)).place(Place::At(Spot::below(mention).gap(4.0))))
        .boxed();
    let mut ui = simulate(view);
    let card = ui.find(Id::new("pins")).expect("the card").bounds();
    assert_eq!((card.y, card.height), (100.0, 190.0));
}
