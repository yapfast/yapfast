//! The effect over a card: it plays on hover, once for one that doesn't
//! loop, asks for frames only then, holds still otherwise, and lets clicks
//! through. The widget is driven through its `update` with a headless
//! shell, which says what it asked of the window. What it draws: `looks`;
//! pictures of it and of the frame for review: `gallery`.

mod gallery;
mod looks;
pub(crate) mod support;

use std::time::{Duration, Instant};

use iced::advanced::layout::Layout;
use iced::advanced::renderer::Settings;
use iced::advanced::shell::{Bus, Waker};
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Shell, mouse};
use iced::widget::{button, stack, text};
use iced::window::{self, RedrawRequest};
use iced::{Event, Fill, Point, Rectangle, Size};
use iced_test::Simulator;

use super::*;

const CARD: Size = Size::new(300.0, 500.0);

pub(crate) fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

/// What the effect sends: here only that a layer's picture is wanted.
type Sent = &'static str;

struct Rig {
    tree: Tree,
    renderer: Renderer,
    cursor: mouse::Cursor,
    /// What it sent so far.
    sent: Vec<Sent>,
}

impl Rig {
    fn new(widget: &ProfileEffect<'_, Sent>) -> Self {
        let mut tree = Tree::empty();
        tree.tag = Widget::<Sent, Theme, Renderer>::tag(widget);
        tree.state = Widget::<Sent, Theme, Renderer>::state(widget);
        Self {
            tree,
            renderer: Renderer::new(Settings::default()),
            cursor: mouse::Cursor::Unavailable,
            sent: Vec::new(),
        }
    }

    /// Sends `event` and says what the effect asked of the window; what it
    /// sent goes in `sent`.
    fn send(&mut self, widget: &mut ProfileEffect<'_, Sent>, event: Event) -> RedrawRequest {
        let mut bus = Bus::<Sent>::new();
        let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
        let viewport = Rectangle::new(Point::ORIGIN, CARD);
        widget.update(
            &mut self.tree,
            &event,
            Layout::new(CARD),
            self.cursor,
            &self.renderer,
            &mut shell,
            &viewport,
        );
        assert!(!shell.is_event_captured(), "it takes no event");
        let asked = shell.redraw_request();
        self.sent.extend(bus.drain().map(|(sent, _)| sent));
        asked
    }

    fn point_at(&mut self, widget: &mut ProfileEffect<'_, Sent>, position: Point) -> RedrawRequest {
        self.cursor = mouse::Cursor::Available(position);
        self.send(widget, Event::Mouse(mouse::Event::CursorMoved { position }))
    }

    fn frame(&mut self, widget: &mut ProfileEffect<'_, Sent>, now: Instant) -> RedrawRequest {
        self.send(widget, Event::Window(window::Event::RedrawRequested(now)))
    }

    fn playing(&self) -> bool {
        self.tree.state.downcast_ref::<State>().playback.playing()
    }

    /// Which layers have a player, so show.
    fn showing(&self) -> Vec<bool> {
        let state = self.tree.state.downcast_ref::<State>();
        state.layers.iter().map(Option::is_some).collect()
    }
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn at_rest_it_shows_its_still_and_asks_for_nothing() {
    let petals = sample::petals();
    let mut widget = effect(&petals, front());
    let mut rig = Rig::new(&widget);
    assert_eq!(rig.frame(&mut widget, Instant::now()), RedrawRequest::Wait);
    assert!(!rig.playing());
    assert_eq!(rig.showing(), [false, false]);
}

#[test]
fn hovered_it_plays_its_layers_as_they_come() {
    let petals = sample::petals();
    let mut widget = effect(&petals, front());
    let mut rig = Rig::new(&widget);
    let now = Instant::now();
    assert_eq!(
        rig.point_at(&mut widget, Point::new(150.0, 250.0)),
        RedrawRequest::NextFrame
    );
    let due = rig.frame(&mut widget, now);
    assert!(rig.playing());
    assert_eq!(rig.showing(), [true, false], "the intro, not the loop yet");
    // The intro's next frame, a fifth of a second on.
    assert_eq!(due, RedrawRequest::At(now + ms(200)));
    rig.frame(&mut widget, now + ms(1600));
    assert_eq!(rig.showing(), [true, true]);
    rig.frame(&mut widget, now + ms(2100));
    assert_eq!(rig.showing(), [false, true], "the intro played once");
    rig.frame(&mut widget, now + ms(4600));
    assert_eq!(rig.showing(), [false, false], "the loop's delay");
    rig.frame(&mut widget, now + ms(5600));
    assert_eq!(rig.showing(), [false, true], "round again");
}

#[test]
fn an_effect_that_doesnt_loop_plays_once_then_holds_still() {
    let mut once = sample::petals();
    once.layers.truncate(1);
    let mut widget = effect(&once, front());
    let mut rig = Rig::new(&widget);
    let now = Instant::now();
    rig.point_at(&mut widget, Point::new(10.0, 10.0));
    rig.frame(&mut widget, now);
    assert!(rig.playing());
    assert_eq!(rig.frame(&mut widget, now + ms(2100)), RedrawRequest::Wait);
    assert!(!rig.playing(), "over: the still again");
    assert_eq!(rig.frame(&mut widget, now + ms(5000)), RedrawRequest::Wait);
    // Off and on again: it plays from the start.
    rig.point_at(&mut widget, Point::new(400.0, 10.0));
    rig.frame(&mut widget, now + ms(5100));
    rig.point_at(&mut widget, Point::new(10.0, 10.0));
    rig.frame(&mut widget, now + ms(5200));
    assert!(rig.playing());
    assert_eq!(rig.showing(), [true]);
}

#[test]
fn the_pointer_leaving_stops_it_at_once() {
    let petals = sample::petals();
    let mut widget = effect(&petals, front());
    let mut rig = Rig::new(&widget);
    let now = Instant::now();
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    rig.frame(&mut widget, now);
    assert_eq!(
        rig.point_at(&mut widget, Point::new(150.0, 600.0)),
        RedrawRequest::NextFrame
    );
    assert_eq!(rig.frame(&mut widget, now + ms(40)), RedrawRequest::Wait);
    assert!(!rig.playing());
    assert_eq!(rig.showing(), [false, false]);
}

#[test]
fn behind_another_window_it_holds_still_and_asks_for_nothing() {
    let petals = sample::petals();
    let mut widget = effect(&petals, Motion::default());
    let mut rig = Rig::new(&widget);
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    assert_eq!(rig.frame(&mut widget, Instant::now()), RedrawRequest::Wait);
    assert!(!rig.playing());
    let never = Motion {
        never: true,
        ..front()
    };
    let mut widget = effect(&petals, never);
    let mut rig = Rig::new(&widget);
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    assert_eq!(rig.frame(&mut widget, Instant::now()), RedrawRequest::Wait);
    assert!(!rig.playing(), "animations off");
}

#[test]
fn a_held_effect_never_plays_and_one_told_to_plays_unhovered() {
    let petals = sample::petals();
    let mut widget = effect(&petals, front()).play(Play::Held);
    let mut rig = Rig::new(&widget);
    assert_eq!(
        rig.point_at(&mut widget, Point::new(150.0, 250.0)),
        RedrawRequest::Wait
    );
    rig.frame(&mut widget, Instant::now());
    assert!(!rig.playing());
    let mut widget = effect(&petals, front()).play(Play::While(true));
    let mut rig = Rig::new(&widget);
    rig.frame(&mut widget, Instant::now());
    assert!(rig.playing());
}

#[test]
fn an_effect_without_layers_never_plays() {
    let still = Effect {
        still: sample::petals().still,
        layers: Vec::new(),
    };
    let mut widget = effect(&still, front());
    let mut rig = Rig::new(&widget);
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    assert_eq!(rig.frame(&mut widget, Instant::now()), RedrawRequest::Wait);
    assert!(!rig.playing());
}

#[test]
fn a_layer_whose_picture_hasnt_come_draws_nothing_until_it_does() {
    let mut waiting = sample::petals();
    let picture = waiting.layers[0].picture.take();
    let mut widget = effect(&waiting, front());
    let mut rig = Rig::new(&widget);
    let now = Instant::now();
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    // Only the play's own next change: the loop at 1.5 s.
    assert_eq!(
        rig.frame(&mut widget, now),
        RedrawRequest::At(now + ms(1500))
    );
    assert_eq!(rig.showing(), [false, false]);
    waiting.layers[0].picture = picture;
    let mut widget = effect(&waiting, front());
    rig.frame(&mut widget, now + ms(300));
    assert_eq!(rig.showing(), [true, false]);
}

#[test]
fn a_layer_showing_without_its_picture_asks_for_it_once_a_play() {
    let mut waiting = sample::petals();
    waiting.layers[1].picture = None;
    let mut widget = effect(&waiting, front()).on_wanted("wanted");
    let mut rig = Rig::new(&widget);
    let now = Instant::now();
    rig.frame(&mut widget, now);
    assert!(rig.sent.is_empty(), "at rest nothing shows");
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    rig.frame(&mut widget, now);
    assert!(rig.sent.is_empty(), "the intro's is here, the loop not due");
    rig.frame(&mut widget, now + ms(1600));
    assert_eq!(rig.sent, ["wanted"]);
    rig.frame(&mut widget, now + ms(1700));
    rig.frame(&mut widget, now + ms(5600));
    assert_eq!(rig.sent, ["wanted"], "once a play");
    // Pointed at again while it still hasn't come: a new play asks again.
    rig.point_at(&mut widget, Point::new(150.0, 600.0));
    rig.frame(&mut widget, now + ms(5700));
    rig.point_at(&mut widget, Point::new(150.0, 250.0));
    rig.frame(&mut widget, now + ms(5800));
    rig.frame(&mut widget, now + ms(7400));
    assert_eq!(rig.sent, ["wanted", "wanted"]);
}

#[test]
fn held_still_it_wants_no_picture() {
    let mut waiting = sample::petals();
    waiting.layers[0].picture = None;
    for motion in [Motion::default(), front()] {
        let play = if motion.playing() {
            Play::Held
        } else {
            Play::Hover
        };
        let mut widget = effect(&waiting, motion).play(play).on_wanted("wanted");
        let mut rig = Rig::new(&widget);
        rig.point_at(&mut widget, Point::new(150.0, 250.0));
        rig.frame(&mut widget, Instant::now());
        assert!(rig.sent.is_empty(), "behind another window, or held");
    }
}

#[test]
fn clicks_go_through_it() {
    let petals = sample::petals();
    let card = button(text("Under the effect"))
        .on_press("pressed")
        .width(Fill)
        .height(Fill);
    let over = stack![card, effect(&petals, front())];
    let mut ui: Simulator<'_, &'static str, Theme, Renderer> =
        Simulator::with_size(iced::Settings::default(), CARD, over);
    ui.click("Under the effect").expect("the button is there");
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), ["pressed"]);
}

#[test]
fn a_picture_reaching_past_the_cards_foot_is_cut_there_and_rounded() {
    let petals = sample::petals();
    let card = Rectangle::new(Point::new(10.0, 20.0), CARD);
    let rounded = effect::<Sent>(&petals, front()).radius(8.0);
    // Twice as tall as wide: 600 tall on a 300 by 500 card.
    let (drawn, shape) = rounded
        .place(card, Vector::ZERO, Size::new(100, 200))
        .expect("placed");
    assert_eq!(drawn, card, "cut at the foot");
    assert_eq!(shape.fit, Fit::Top, "from its top");
    assert_eq!(<[f32; 4]>::from(shape.radius), [8.0; 4], "every corner");
    // Lying lower, only its foot corners are the card's.
    let (drawn, shape) = rounded
        .place(card, Vector::new(0.0, 100.0), Size::new(100, 200))
        .expect("placed");
    assert_eq!((drawn.y, drawn.height), (120.0, 400.0));
    assert_eq!(<[f32; 4]>::from(shape.radius), [0.0, 0.0, 8.0, 8.0]);
    // A short one keeps its size and its top corners; a square card cuts none.
    let (drawn, shape) = rounded
        .place(card, Vector::ZERO, Size::new(300, 100))
        .expect("placed");
    assert_eq!((drawn.height, shape.fit), (100.0, Fit::Fill));
    assert_eq!(<[f32; 4]>::from(shape.radius), [8.0, 8.0, 0.0, 0.0]);
    let square = effect::<Sent>(&petals, front());
    let (drawn, _) = square
        .place(card, Vector::ZERO, Size::new(100, 200))
        .expect("placed");
    assert_eq!(drawn.height, 600.0, "clipped by the card, not cut");
}
