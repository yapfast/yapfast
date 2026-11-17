//! A middle click that scrolls the list under it ([`Autoscroll`]).

use iced::{
    Border, Color, Element, Point, Rectangle, Size, Vector,
    advanced::{Shell, image::Handle, mouse, renderer, widget::Operation},
    window,
};

use super::{At, Hooks, Inner, Renderer, Wrap, draw_picture, moved_to};

/// How far the pointer can be from where it clicked without scrolling; how
/// fast it scrolls just past that, in px a second for each px; how much
/// faster the farther, for each px squared; and the fastest, in px a
/// second.
const AUTOSCROLL_STILL: f32 = 10.0;
const AUTOSCROLL_SPEED: f32 = 8.0;
const AUTOSCROLL_ACCELERATION: f32 = 0.05;
const AUTOSCROLL_FASTEST: f32 = 12000.0;
/// The marker at the click: its side, the dot at its middle, its arrows,
/// its shadow and the room its layer leaves for it.
const AUTOSCROLL_MARKER: f32 = 30.0;
const MARKER_DOT: f32 = 4.0;
const MARKER_ARROW: f32 = 10.0;
const MARKER_SHADOW: iced::Shadow = iced::Shadow {
    color: Color {
        a: 0x60 as f32 / 255.0,
        ..Color::BLACK
    },
    offset: Vector::new(0.0, 2.0),
    blur_radius: 8.0,
};
const MARKER_ROOM: f32 = 12.0;

/// How fast the pointer `distance` px from where it clicked scrolls, in px
/// a second.
pub fn autoscroll_speed(distance: f32) -> f32 {
    let past = distance.abs() - AUTOSCROLL_STILL;
    if past <= 0.0 {
        return 0.0;
    }
    (past * AUTOSCROLL_SPEED + past * past * AUTOSCROLL_ACCELERATION)
        .min(AUTOSCROLL_FASTEST)
        .copysign(distance)
}

/// The marker's look: its ground, edge and dot, and the arrows' pictures.
pub struct AutoscrollLook {
    pub ground: Color,
    pub edge: Color,
    pub dot: Color,
    pub up: Handle,
    pub down: Handle,
}

/// Around the whole window, as in browsers: a middle click on a list that
/// has more than fits scrolls it towards the pointer, faster the farther
/// the pointer is from the click, until the next press, a key or the
/// wheel; held and moved away, until it's let go. Meanwhile the pointer's
/// events go to nothing under it, the press that ends it included, and a
/// marker shows at the click. It scrolls the list found at the press on
/// each frame it asks for, asking for frames only while the pointer is far
/// enough from the click to scroll. Lists anchored to their foot
/// (`anchored`, by id) scroll the other way round.
pub struct Autoscroll {
    anchored: &'static [&'static str],
    look: AutoscrollLook,
}

impl Autoscroll {
    pub fn new<'a, M>(
        child: impl Into<Element<'a, M>>,
        anchored: &'static [&'static str],
        look: AutoscrollLook,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(child, Self { anchored, look })
    }
}

#[derive(Default)]
pub struct AutoscrollState {
    scroll: Option<Scrolling>,
}

pub struct Scrolling {
    origin: Point,
    pointer: Point,
    /// The middle button is still down; moved away with it down, letting
    /// go ends it.
    held: bool,
    dragged: bool,
    /// The button whose press ended it, until it's let go.
    ending: Option<mouse::Button>,
    /// The last frame it scrolled on, while it's scrolling.
    last: Option<std::time::Instant>,
    /// The list it scrolls, by its place in the traversal, found at the
    /// press.
    target: usize,
}

/// Finds the innermost list under a point that has more than fits, by its
/// place in the traversal, and with `by` set scrolls it.
struct ScrollAt<'a> {
    at: Point,
    anchored: &'a [&'static str],
    seen: usize,
    found: Option<usize>,
    by: Option<(usize, f32)>,
    /// How far what's inside the scrollables passed is moved, and the
    /// last one's own, for what it holds.
    moved: Vector,
    inside: Option<Vector>,
}

impl<'a> ScrollAt<'a> {
    fn new(at: Point, anchored: &'a [&'static str], by: Option<(usize, f32)>) -> Self {
        Self {
            at,
            anchored,
            seen: 0,
            found: None,
            by,
            moved: Vector::ZERO,
            inside: None,
        }
    }
}

impl Operation for ScrollAt<'_> {
    /// A scrollable's content is laid out unscrolled: it's moved by the
    /// scroll of every scrollable it's in, and only within them.
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let moved = self.moved;
        if let Some(inside) = self.inside.take() {
            self.moved = moved - inside;
        }
        operate(self);
        self.moved = moved;
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        let index = self.seen;
        self.seen += 1;
        self.inside = Some(translation);
        let bounds = bounds + self.moved;
        if bounds.contains(self.at) && content_bounds.height > bounds.height + 1.0 {
            self.found = Some(index);
        }
        if let Some((target, by)) = self.by
            && target == index
        {
            let anchored = id.is_some_and(|id| {
                self.anchored
                    .iter()
                    .any(|name| iced::advanced::widget::Id::new(name) == *id)
            });
            let by = if anchored { -by } else { by };
            state.scroll_by(
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: by },
                bounds,
                content_bounds,
            );
        }
    }
}

impl<M> Hooks<M> for Autoscroll {
    type State = AutoscrollState;

    fn update(
        &mut self,
        state: &mut AutoscrollState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        let going = state.scroll.is_some();
        if let Some(scroll) = state.scroll.as_mut() {
            let mut take = true;
            match event {
                iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    if let Some(position) = moved_to(event, at.cursor) {
                        scroll.pointer = position;
                    }
                    if scroll.held && scroll.pointer.distance(scroll.origin) > AUTOSCROLL_STILL {
                        scroll.dragged = true;
                    }
                    shell.request_redraw();
                }
                iced::Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                    scroll.ending = Some(*button);
                    shell.request_redraw();
                }
                iced::Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                    if scroll.ending == Some(*button) {
                        state.scroll = None;
                    } else if *button == mouse::Button::Middle {
                        scroll.held = false;
                        if scroll.dragged {
                            state.scroll = None;
                        }
                    }
                    shell.request_redraw();
                }
                iced::Event::Mouse(mouse::Event::WheelScrolled { .. })
                | iced::Event::Keyboard(_)
                | iced::Event::Window(window::Event::Unfocused) => {
                    state.scroll = None;
                    take = false;
                    shell.request_redraw();
                }
                iced::Event::Window(window::Event::RedrawRequested(now))
                    if scroll.ending.is_none() =>
                {
                    // Near the click it's still, and asks for no frames
                    // until the pointer moves.
                    let speed = autoscroll_speed(scroll.pointer.y - scroll.origin.y);
                    if speed == 0.0 {
                        scroll.last = None;
                    } else {
                        let elapsed = scroll
                            .last
                            .map_or(0.0, |last| now.duration_since(last).as_secs_f32().min(0.05));
                        scroll.last = Some(*now);
                        let by = speed * elapsed;
                        if by != 0.0 {
                            let mut scroll = ScrollAt::new(
                                scroll.origin,
                                self.anchored,
                                Some((scroll.target, by)),
                            );
                            child.operate(&mut scroll);
                        }
                        shell.request_redraw();
                    }
                    take = false;
                }
                _ => take = false,
            }
            if take {
                shell.capture_event();
                return;
            }
        }
        child.update(shell);
        if !going
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) = event
            && !shell.is_event_captured()
            && let Some(origin) = at.cursor.position()
        {
            let mut find = ScrollAt::new(origin, self.anchored, None);
            child.operate(&mut find);
            if let Some(target) = find.found {
                state.scroll = Some(Scrolling {
                    origin,
                    pointer: origin,
                    held: true,
                    dragged: false,
                    ending: None,
                    last: None,
                    target,
                });
                shell.capture_event();
                shell.request_redraw();
            }
        }
    }

    fn interaction(
        &self,
        state: &AutoscrollState,
        _: At,
        child: mouse::Interaction,
    ) -> mouse::Interaction {
        match &state.scroll {
            Some(scroll) if scroll.ending.is_some() => mouse::Interaction::default(),
            Some(_) => mouse::Interaction::ResizingVertically,
            None => child,
        }
    }

    fn draw(
        &self,
        state: &AutoscrollState,
        renderer: &mut Renderer,
        _: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        use iced::advanced::Renderer as _;
        child(renderer);
        let Some(scroll) = state
            .scroll
            .as_ref()
            .filter(|scroll| scroll.ending.is_none())
        else {
            return;
        };
        // A circle at the click with the arrows the list can go.
        let side = AUTOSCROLL_MARKER;
        let marker = Rectangle::new(
            scroll.origin - Vector::new(side / 2.0, side / 2.0),
            Size::new(side, side),
        );
        let look = &self.look;
        renderer.with_layer(marker.expand(MARKER_ROOM), |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: marker,
                    border: crate::style::edge(look.edge, side / 2.0),
                    shadow: MARKER_SHADOW,
                    ..Default::default()
                },
                look.ground,
            );
            let centre = marker.center();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        centre - Vector::new(MARKER_DOT / 2.0, MARKER_DOT / 2.0),
                        Size::new(MARKER_DOT, MARKER_DOT),
                    ),
                    border: Border::default().rounded(MARKER_DOT / 2.0),
                    ..Default::default()
                },
                look.dot,
            );
            // Each arrow 3 px in from its edge.
            for (handle, y) in [
                (&look.up, marker.y + 3.0),
                (&look.down, marker.y + side - 3.0 - MARKER_ARROW),
            ] {
                let at = Rectangle::new(
                    Point::new(centre.x - MARKER_ARROW / 2.0, y),
                    Size::new(MARKER_ARROW, MARKER_ARROW),
                );
                draw_picture(renderer, handle, at, [0.0; 4], 1.0);
            }
        });
    }
}

#[cfg(test)]
mod autoscroll_tests {
    use iced::{
        Point, Settings, Size,
        advanced::image::Handle,
        mouse::{Button, Event as Mouse},
        widget::{Id, Space, column},
    };
    use iced_test::{Simulator, selector::Candidate};

    use super::{Autoscroll, AutoscrollLook, autoscroll_speed};
    use crate::testing::Message;
    use crate::widgets::{scroll_view, swipe};

    /// A scrollable's offset in `ui`.
    fn offset(ui: &mut Simulator<'_, Message>, name: &'static str) -> f32 {
        ui.find(move |candidate: Candidate<'_>| match candidate {
            Candidate::Scrollable {
                id: Some(id),
                translation,
                ..
            } if *id == Id::new(name) => Some(translation.y),
            _ => None,
        })
        .expect("the list")
    }

    #[test]
    fn a_middle_click_in_a_list_inside_a_scrolled_page_scrolls_that_list() {
        // A page 1000 px tall in 300, its list 200 px into it, 100 px tall
        // and holding 400.
        let space = |height: f32| Space::new().width(300).height(height);
        let list = scroll_view(space(400.0)).id("list").width(300).height(100);
        let page = scroll_view(column![space(200.0), list, space(700.0)])
            .id("page")
            .width(300)
            .height(300);
        let blank = || Handle::from_rgba(1, 1, vec![0; 4]);
        let look = AutoscrollLook {
            ground: iced::Color::BLACK,
            edge: iced::Color::BLACK,
            dot: iced::Color::BLACK,
            up: blank(),
            down: blank(),
        };
        let view: iced::Element<'_, Message> = Autoscroll::new(page, &[], look).into();
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 300.0), view);
        // The page scrolled 150 px down shows the list from 50 px.
        ui.point_at(Point::new(150.0, 150.0));
        ui.simulate(swipe(-150.0));
        assert_eq!(offset(&mut ui, "page"), 150.0);
        let at = Point::new(150.0, 100.0);
        ui.point_at(at);
        let start = std::time::Instant::now();
        let frame = |ms| {
            iced::Event::Window(iced::window::Event::RedrawRequested(
                start + std::time::Duration::from_millis(ms),
            ))
        };
        let below = Point::new(150.0, 180.0);
        ui.simulate([iced::Event::Mouse(Mouse::ButtonPressed(Button::Middle))]);
        ui.point_at(below);
        ui.simulate([
            iced::Event::Mouse(Mouse::CursorMoved { position: below }),
            frame(0),
            frame(16),
            frame(32),
        ]);
        assert!(offset(&mut ui, "list") > 0.0, "the list under the click");
        assert_eq!(offset(&mut ui, "page"), 150.0, "not the page");
    }

    #[test]
    fn autoscroll_is_still_near_the_click_and_faster_farther() {
        assert_eq!(autoscroll_speed(10.0), 0.0);
        assert_eq!(autoscroll_speed(-6.0), 0.0);
        // 10 px past: 80 px a second and 5 for the acceleration.
        assert!((autoscroll_speed(20.0) - 85.0).abs() < 0.001);
        assert!((autoscroll_speed(-20.0) + 85.0).abs() < 0.001);
        assert_eq!(autoscroll_speed(5000.0), 12000.0);
    }
}
