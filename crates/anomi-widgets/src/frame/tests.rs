//! The frame around a card: stills that ask for no frames, take no clicks,
//! and draw past the card, down to its foot however much room it has.

use std::time::Instant;

use iced::advanced::layout::Layout;
use iced::advanced::renderer::Settings;
use iced::advanced::shell::{Bus, Waker};
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Shell, mouse};
use iced::widget::{button, column, container, space, stack, text};
use iced::window::{self, RedrawRequest};
use iced::{Event, Fill, Point, Rectangle, Size};
use iced_test::Simulator;

use anomi_theme::catalog::container::raised;

use super::*;
use crate::effect::tests::support::{Shot, ground, near};
use crate::picture::sample as pictures;

const CARD: Size = Size::new(300.0, 500.0);

/// Sends a frame at `now` to `widget` and says what it asked of the window.
fn frame_at(widget: &mut ProfileFrame<'_>, tree: &mut Tree, now: Instant) -> RedrawRequest {
    let renderer = Renderer::new(Settings::default());
    let mut bus = Bus::<()>::new();
    let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
    let viewport = Rectangle::new(Point::ORIGIN, Size::new(400.0, 600.0));
    let layout = Layout::new(CARD).move_to(Point::new(50.0, 50.0));
    let event = Event::Window(window::Event::RedrawRequested(now));
    widget.update(
        tree,
        &event,
        layout,
        mouse::Cursor::Unavailable,
        &renderer,
        &mut shell,
        &viewport,
    );
    let asked = shell.redraw_request();
    assert!(bus.is_empty());
    asked
}

fn tree_of(widget: &ProfileFrame<'_>) -> Tree {
    let mut tree = Tree::empty();
    tree.tag = Widget::<(), Theme, Renderer>::tag(widget);
    tree.state = Widget::<(), Theme, Renderer>::state(widget);
    tree
}

#[test]
fn a_frame_is_still_and_asks_for_no_frames() {
    let layers = sample::brass();
    for side in [Side::Back, Side::Front] {
        let mut widget = frame(&layers, side);
        let mut tree = tree_of(&widget);
        assert_eq!(
            frame_at(&mut widget, &mut tree, Instant::now()),
            RedrawRequest::Wait
        );
    }
}

#[test]
fn a_moving_picture_in_a_frame_holds_its_first_frame() {
    let layers = [FrameLayer {
        picture: Some(pictures::bouncing(40, 4, [200, 120, 40])),
        kind: Kind::Rail,
        anchor: Anchor::Center,
        front: true,
    }];
    let mut widget = frame(&layers, Side::Front);
    let mut tree = tree_of(&widget);
    assert_eq!(
        frame_at(&mut widget, &mut tree, Instant::now()),
        RedrawRequest::Wait
    );
}

/// A card of `contents` with both sides of `layers` around it, stacked as
/// [`frame`] says: the contents its base, the back side and then the
/// ground pushed under them, the front side over them.
fn framed<'a, Message: 'a>(
    contents: impl iced::Widget<Message, Theme, Renderer> + 'a,
    layers: &'a [FrameLayer],
    large: bool,
) -> impl iced::Widget<Message, Theme, Renderer> + 'a {
    use iced::Widget as _;
    let ground = container(space()).width(Fill).height(Fill).style(raised);
    stack![contents, frame(layers, Side::Front).large(large)]
        .push_under(frame(layers, Side::Back).large(large).boxed())
        .push_under(ground.boxed())
}

#[test]
fn clicks_go_through_both_sides_of_a_frame() {
    let layers = sample::brass();
    // As tall as what it holds, as a profile card is.
    let contents = column![
        button(text("On the card"))
            .on_press("pressed")
            .width(Fill)
            .height(240),
        text("Under the button"),
    ];
    let around = container(framed(contents, &layers, true)).padding(40);
    let mut ui: Simulator<'_, &'static str, Theme, Renderer> =
        Simulator::with_size(iced::Settings::default(), Size::new(400.0, 600.0), around);
    ui.draw(&Theme::default());
    ui.click("On the card").expect("the button is there");
    // On the staple's knob, over the card's top: the card is 320 wide at
    // (40, 40), the staple 352 wide from x = 24 and 84 tall from y = 74.
    ui.point_at(Point::new(200.0, 120.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        ["pressed", "pressed"]
    );
}

#[test]
fn its_borders_end_at_the_cards_foot_with_room_below() {
    let theme = Theme::default();
    let ground = ground(&theme);
    let layers = sample::brass();
    // A 300 by 515 card at (40, 40), its foot at 555, in a window 760 tall.
    let contents = space().width(300).height(515);
    let around = container(framed::<()>(contents, &layers, false)).padding(40);
    let mut shot = Shot::new(around, Size::new(380.0, 760.0));
    shot.frame(Instant::now());
    let pixels = shot.pixels(&theme, 1.0);
    // The left border's brass, 12 px past the card's side: 28 to 37.
    let brass = |y: u32| pixels.get_pixel(32, y).0;
    for y in [60, 300, 550, 554] {
        assert!(near(brass(y), [196, 150, 60, 255]), "brass at {y}");
    }
    for y in [556, 560, 700] {
        assert!(near(brass(y), ground), "the window's ground at {y}");
    }
}
