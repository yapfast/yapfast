//! The menu's rows and spots.

use super::*;

#[test]
fn a_ticked_box_is_not_the_current_choice() {
    let level: Row<()> = Row::new("All messages", ()).checked(true);
    assert!(level.current());
    let switch: Row<()> = Row::new("Hide muted channels", ()).checkbox(true);
    assert!(switch.checked && !switch.current());
}

#[test]
fn a_row_without_intent_or_submenu_does_nothing() {
    assert!(!Row::<()>::default().choosable());
    assert!(Row::<()>::opens("Apps", vec![Row::new("Quote", ())]).choosable());
    assert!(!Row::new("Kick", ()).disabled(true).choosable());
}

#[test]
fn a_button_spot_keeps_the_window_gap() {
    let button = Rectangle::new(Point::new(80.0, 8.0), Size::new(248.0, 32.0));
    let spot = Spot::below(button);
    assert_eq!(
        (spot.side, spot.align, spot.gap),
        (Side::Below, Align::Start, GAP)
    );
    assert_eq!(Spot::above(button).side, Side::Above);
    let beside = Spot::beside(button);
    assert_eq!((beside.side, beside.gap), (Side::Right, GAP_BESIDE));
    assert_eq!(
        Spot::pointer(Point::new(3.0, 4.0)).anchor.size(),
        Size::ZERO
    );
}

#[test]
fn a_choice_lifts_into_the_window_intent() {
    let intent = crate::Intent::from(Choice::new("header:s1", "invite"));
    assert_eq!(
        intent,
        crate::Intent::Menu(Intent::Choose(Choice::new("header:s1", "invite")))
    );
}
