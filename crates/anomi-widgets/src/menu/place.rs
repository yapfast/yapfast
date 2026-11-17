//! Where a popup goes over the window: beside its anchor on the side it
//! prefers, on the other when it doesn't fit, and moved inside the window
//! when it would cross an edge. The Slint window places each popup by hand
//! the same way: the context menu below the pointer or above it
//! (`ui/yapfast.slint:2312`), popovers under their button, kept 8 px from
//! the window's edges (`:4892`), a member's card beside its row (`:5025`).
//! Each keeps [`MARGIN`] from every edge; only the header's popovers leave
//! [`HEADER_CLEARANCE`] under them, which their opener asks for and
//! [`cleared`] turns into the window they are placed in.

use iced::{Point, Rectangle, Size};

use anomi_vm::menu::{Align, Side, Spot};

/// The closest a popup comes to the window's edge: the `max(8px, ...)` and
/// `root.width - self.width - 8px` of every popup in `ui/yapfast.slint`.
pub const MARGIN: f32 = 8.0;

/// Where a popup went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its rectangle in the window.
    pub bounds: Rectangle,
    /// The side of its anchor it took.
    pub side: Side,
    /// The point it grows from as it arrives: its corner or edge at the
    /// anchor, as each popup's `transform-origin` in the Slint window.
    pub origin: Point,
}

/// Places a popup of `size` at `spot` in a window of `window`, `margin`
/// from its edges.
pub fn place(spot: Spot, size: Size, window: Size, margin: f32) -> Placed {
    let anchor = spot.anchor;
    let side = choose(spot, size, window, margin);
    let position = match side {
        Side::Below => Point::new(
            along(spot.align, anchor.x, anchor.width, size.width),
            anchor.y + anchor.height + spot.gap,
        ),
        Side::Above => Point::new(
            along(spot.align, anchor.x, anchor.width, size.width),
            anchor.y - spot.gap - size.height,
        ),
        Side::Right => Point::new(
            anchor.x + anchor.width + spot.gap,
            along(spot.align, anchor.y, anchor.height, size.height),
        ),
        Side::Left => Point::new(
            anchor.x - spot.gap - size.width,
            along(spot.align, anchor.y, anchor.height, size.height),
        ),
    };
    let bounds = Rectangle::new(
        Point::new(
            within(position.x, margin, window.width - margin - size.width),
            within(position.y, margin, window.height - margin - size.height),
        ),
        size,
    );
    Placed {
        bounds,
        side,
        origin: origin(side, spot.align, bounds, anchor),
    }
}

/// The point on the spot's preferred side where a popup placed there starts:
/// what it grows from when it fills the window and places itself, as a
/// context menu does.
pub fn near(spot: Spot) -> Point {
    let anchor = spot.anchor;
    let across = |start: f32, length: f32| match spot.align {
        Align::Start => start,
        Align::Centre => start + length / 2.0,
        Align::End => start + length,
    };
    match spot.side {
        Side::Below => Point::new(
            across(anchor.x, anchor.width),
            anchor.y + anchor.height + spot.gap,
        ),
        Side::Above => Point::new(across(anchor.x, anchor.width), anchor.y - spot.gap),
        Side::Right => Point::new(
            anchor.x + anchor.width + spot.gap,
            across(anchor.y, anchor.height),
        ),
        Side::Left => Point::new(anchor.x - spot.gap, across(anchor.y, anchor.height)),
    }
}

/// `max(low, min(high, value))`, as Slint writes it: the low edge wins when
/// the popup is larger than the room.
fn within(value: f32, low: f32, high: f32) -> f32 {
    value.min(high).max(low)
}

/// Where a popup `extent` long starts along an anchor's edge.
fn along(align: Align, start: f32, length: f32, extent: f32) -> f32 {
    match align {
        Align::Start => start,
        Align::Centre => start + (length - extent) / 2.0,
        Align::End => start + length - extent,
    }
}

/// The side preferred when the popup fits there, else the other, where
/// [`place`] keeps it inside the margin when it doesn't fit there either. So
/// the Slint window's context menu (`ui/yapfast.slint:2312`: below the
/// pointer if it fits, else `max(8px, anchor-y - height)`) and its cards
/// beside what opened them (`beside`, `:2237`) choose.
fn choose(spot: Spot, size: Size, window: Size, margin: f32) -> Side {
    let extent = match spot.side {
        Side::Below | Side::Above => size.height,
        Side::Right | Side::Left => size.width,
    };
    if room(spot, spot.side, window, margin) >= extent {
        spot.side
    } else {
        opposite(spot.side)
    }
}

/// The room the header's popovers leave under them: the Slint window bounds
/// the topic, the pins, the threads and adding people by
/// `root.height - self.y - 16px` (`ui/yapfast.slint:4894`, `:4918`, `:4944`,
/// `:4967`). Every other popup keeps [`MARGIN`] there: the emoji picker, the
/// cards and the menus test `... + self.height <= root.height - 8px`
/// (`:2651`, `:2739`, `:5139`).
pub const HEADER_CLEARANCE: f32 = 16.0;

/// The window a popup keeping `clearance` from the window's bottom, and
/// `margin` from its other edges, is placed in: the window shortened by the
/// difference, so [`place`] and [`most`], keeping `margin` from its edges,
/// keep `clearance` from the real bottom. With `clearance` at `margin` it is
/// the window itself.
pub fn cleared(window: Size, clearance: f32, margin: f32) -> Size {
    Size::new(window.width, (window.height - clearance + margin).max(0.0))
}

/// The most a popup at `spot` may measure across its side (its height above
/// or below, its width beside): the room on the side with more. A popover
/// taller than that, in its [`cleared`] window, is laid out again to fit, as
/// the Slint window bounds its popovers' height by the room under them.
pub fn most(spot: Spot, window: Size, margin: f32) -> f32 {
    room(spot, spot.side, window, margin).max(room(spot, opposite(spot.side), window, margin))
}

/// The room on a side of the spot's anchor, up to the window's margin.
pub fn room(spot: Spot, side: Side, window: Size, margin: f32) -> f32 {
    let anchor = spot.anchor;
    match side {
        Side::Below => window.height - margin - (anchor.y + anchor.height + spot.gap),
        Side::Above => anchor.y - spot.gap - margin,
        Side::Right => window.width - margin - (anchor.x + anchor.width + spot.gap),
        Side::Left => anchor.x - spot.gap - margin,
    }
}

fn opposite(side: Side) -> Side {
    match side {
        Side::Below => Side::Above,
        Side::Above => Side::Below,
        Side::Right => Side::Left,
        Side::Left => Side::Right,
    }
}

/// The corner or edge that faces the anchor.
fn origin(side: Side, align: Align, bounds: Rectangle, anchor: Rectangle) -> Point {
    let across = match align {
        Align::Start => bounds.x,
        Align::Centre => bounds.center_x(),
        Align::End => bounds.x + bounds.width,
    };
    let level = anchor.center_y().clamp(bounds.y, bounds.y + bounds.height);
    match side {
        Side::Below => Point::new(across, bounds.y),
        Side::Above => Point::new(across, bounds.y + bounds.height),
        Side::Right => Point::new(bounds.x, level),
        Side::Left => Point::new(bounds.x + bounds.width, level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1400.0, 860.0);
    const MENU: Size = Size::new(255.0, 349.0);

    #[test]
    fn a_right_click_opens_below_the_pointer() {
        let placed = place(
            Spot::pointer(Point::new(700.0, 300.0)),
            MENU,
            WINDOW,
            MARGIN,
        );
        assert_eq!(placed.bounds.position(), Point::new(700.0, 300.0));
        assert_eq!(placed.origin, Point::new(700.0, 300.0));
    }

    #[test]
    fn near_the_bottom_it_opens_above_the_pointer() {
        let placed = place(
            Spot::pointer(Point::new(700.0, 700.0)),
            MENU,
            WINDOW,
            MARGIN,
        );
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.bounds.y, 700.0 - MENU.height);
        // It grows from its bottom corner, at the pointer.
        assert_eq!(placed.origin, Point::new(700.0, 700.0));
    }

    #[test]
    fn near_the_right_edge_it_moves_left_into_the_window() {
        let placed = place(
            Spot::pointer(Point::new(1300.0, 100.0)),
            MENU,
            WINDOW,
            MARGIN,
        );
        assert_eq!(placed.bounds.x, WINDOW.width - MARGIN - MENU.width);
    }

    #[test]
    fn a_button_menu_opens_under_the_button() {
        // The server header's button, as in menu/server_header.png.
        let button = Rectangle::new(Point::new(80.0, 8.0), Size::new(248.0, 32.0));
        let placed = place(Spot::below(button), MENU, WINDOW, MARGIN);
        assert_eq!(placed.bounds.position(), Point::new(80.0, 46.0));
    }

    #[test]
    fn a_popover_lines_its_end_up_with_the_button() {
        let button = Rectangle::new(Point::new(1060.0, 9.0), Size::new(32.0, 32.0));
        let spot = Spot::below(button).align(Align::End);
        let placed = place(spot, Size::new(420.0, 300.0), WINDOW, MARGIN);
        assert_eq!(placed.bounds.x + placed.bounds.width, 1092.0);
        assert_eq!(placed.origin, Point::new(1092.0, 47.0));
    }

    #[test]
    fn a_card_beside_a_row_flips_to_its_left_without_room() {
        let row = Rectangle::new(Point::new(1140.0, 400.0), Size::new(250.0, 42.0));
        let placed = place(Spot::beside(row), Size::new(300.0, 500.0), WINDOW, MARGIN);
        assert_eq!(placed.side, Side::Left);
        // 8 px from the row, as `beside` in `ui/yapfast.slint:2237`.
        assert_eq!(placed.bounds.x, 1140.0 - 8.0 - 300.0);
        assert_eq!(anomi_vm::menu::GAP_BESIDE, 8.0);
        // Too tall to start at the row, it moves up into the window.
        assert_eq!(placed.bounds.y, WINDOW.height - MARGIN - 500.0);
    }

    #[test]
    fn a_menu_fitting_neither_side_takes_the_other_at_the_margin() {
        // A 463 px member menu right-clicked at y=300 in a 1000x640 window:
        // 332 px below, 292 above. Slint opens it at `max(8px, 300 - 463)`.
        let window = Size::new(1000.0, 640.0);
        let menu = Size::new(255.0, 463.0);
        let placed = place(
            Spot::pointer(Point::new(700.0, 300.0)),
            menu,
            window,
            MARGIN,
        );
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.bounds.y, 8.0);
    }

    #[test]
    fn a_menu_taller_than_the_window_starts_at_the_margin() {
        let tall = Size::new(255.0, WINDOW.height - 2.0 * MARGIN);
        let placed = place(Spot::pointer(Point::new(10.0, 400.0)), tall, WINDOW, MARGIN);
        assert_eq!(placed.bounds.y, MARGIN);
    }

    #[test]
    fn a_card_at_the_pointer_keeps_only_the_margin_under_it() {
        // 500 + 350 <= 860 - 8: Slint opens it below (`ui/yapfast.slint:2739`).
        let card = Size::new(300.0, 350.0);
        let spot = Spot::pointer(Point::new(700.0, 500.0));
        let window = cleared(WINDOW, MARGIN, MARGIN);
        assert_eq!(window, WINDOW);
        assert_eq!(place(spot, card, window, MARGIN).side, Side::Below);
        // Asked to leave the header's 16 px, it has 344 px under it and flips.
        let header = cleared(WINDOW, HEADER_CLEARANCE, MARGIN);
        assert_eq!(room(spot, Side::Below, header, MARGIN), 344.0);
        assert_eq!(place(spot, card, header, MARGIN).side, Side::Above);
    }

    #[test]
    fn a_button_menu_grows_from_under_the_button() {
        let button = Rectangle::new(Point::new(80.0, 8.0), Size::new(248.0, 32.0));
        assert_eq!(near(Spot::below(button)), Point::new(80.0, 46.0));
        assert_eq!(
            near(Spot::pointer(Point::new(7.0, 9.0))),
            Point::new(7.0, 9.0)
        );
    }
}
