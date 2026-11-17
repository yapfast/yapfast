//! The dragged row's copy: the ground its owner shapes, the copy it builds,
//! laid out in that ground, and both as faint as the ground says.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree};
use iced::{Length, Rectangle};

use super::super::{Fill, Ground};
use super::*;

/// How a [`Probe`] was last drawn: where, and the alpha of the theme's text
/// colour and of the text colour it was handed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drawn {
    bounds: Rectangle,
    text: f32,
    style: f32,
}

/// A row, or a copy, that notes how it's drawn: as wide as it may be, and
/// `height` tall.
struct Probe<'a> {
    height: Length,
    drawn: &'a Cell<Option<Drawn>>,
}

impl widget::Meta for Probe<'_> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Probe<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, self.height, Size::ZERO);
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        self.drawn.set(Some(Drawn {
            bounds: layout.bounds(),
            text: theme.palette().text.a,
            style: style.text_color.a,
        }));
    }
}

fn draggable(rows: &Rows, motion: Motion) -> List<'_, Message> {
    rows.list(motion)
        .reorder(|index| index != 0, |_, _| true, Message::Dragged)
}

/// A category's ground: 36 px tall with 4 px corners (`ui/rail.slint:1629`).
const CATEGORY: Ground = Ground {
    fill: Fill::Active,
    left: 8.0,
    right: 8.0,
    radius: 4.0,
    height: Some(36.0),
    alpha: 0.6,
};

/// Picks up row 2 (60 to 90 px down), held 15 px below its top.
fn pick_up(ui: &mut Simulator<'_, Message, Theme, Renderer>) {
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 90.0)),
    ]);
}

/// Drags to the bottom edge until row 2's place scrolls out.
fn scroll_it_out(ui: &mut Simulator<'_, Message, Theme, Renderer>) {
    let _ = ui.simulate([moved_to(Point::new(100.0, VIEW.height - 5.0))]);
    let start = Instant::now();
    for step in 0..80 {
        frame(ui, start + Duration::from_millis(16 * step));
    }
    assert!(top(ui, 2).is_none(), "its place scrolled out");
}

#[test]
fn the_owner_shapes_the_ground_of_the_row_dragged_near_or_far() {
    let rows = Rows::even(200, 30.0);
    let asked = Cell::new(None);
    let person = |index| {
        asked.set(Some(index));
        Ground {
            left: 40.0,
            right: 8.0,
            radius: 4.0,
            height: Some(30.0),
            ..Ground::default()
        }
    };
    let theme = Theme::built_in(None);
    let mut ui = simulate(draggable(&rows, front()).drag_ground(person));
    pick_up(&mut ui);
    ui.draw(&theme);
    assert_eq!(asked.take(), Some(2), "the dragged row, built in view");
    scroll_it_out(&mut ui);
    ui.draw(&theme);
    assert_eq!(asked.take(), Some(2), "the dragged row, built apart");
}

#[test]
fn the_owners_copy_lies_in_its_ground_near_or_far_and_fades_with_it() {
    let rows = Rows::even(200, 30.0);
    let (asked, drawn) = (Cell::new(None), Cell::new(None));
    let copy = |index| {
        asked.set(Some(index));
        let drawn = &drawn;
        Probe {
            height: Length::Fill,
            drawn,
        }
        .boxed()
    };
    let theme = Theme::built_in(None);
    let list = draggable(&rows, front())
        .drag_ground(|_| CATEGORY)
        .drag_copy(copy);
    let mut ui = simulate(list);
    pick_up(&mut ui);
    ui.draw(&theme);
    assert_eq!(asked.take(), Some(2), "the dragged row's copy, in view");
    let held = drawn.take().expect("the copy is drawn");
    let ground = Rectangle::new(Point::new(8.0, 75.0), Size::new(VIEW.width - 16.0, 36.0));
    assert_eq!(held.bounds, ground, "in the ground, where the row is held");
    let text = theme.palette().text.a;
    assert!(
        (held.text - text * 0.6).abs() < 1e-6,
        "faint: {}",
        held.text
    );
    scroll_it_out(&mut ui);
    ui.draw(&theme);
    assert_eq!(asked.take(), Some(2), "and again with its row out of view");
    let far = drawn.take().expect("the copy is drawn");
    assert_eq!(far.bounds.y, VIEW.height - 5.0 - 15.0, "at the pointer");
    assert_eq!(far.bounds.size(), ground.size());
    let _ = ui.simulate([release(mouse::Button::Left)]);
    ui.draw(&theme);
    assert_eq!(asked.take(), None, "nothing is built once it drops");
}

#[test]
fn a_copy_without_a_height_of_its_own_is_as_tall_as_its_row() {
    let rows = Rows::even(20, 30.0);
    let drawn = Cell::new(None);
    let copy = |_| {
        let drawn = &drawn;
        Probe {
            height: Length::Fill,
            drawn,
        }
        .boxed()
    };
    let person = Ground {
        left: 40.0,
        ..Ground::default()
    };
    let list = draggable(&rows, still())
        .drop_line(false)
        .drag_ground(move |_| person)
        .drag_copy(copy);
    let mut ui = simulate(list);
    pick_up(&mut ui);
    ui.draw(&Theme::built_in(None));
    let bounds = drawn.take().expect("the copy is drawn").bounds;
    assert_eq!(
        (bounds.x, bounds.y),
        (40.0, 90.0 + 12.0),
        "onto targets it hangs below the pointer"
    );
    assert_eq!((bounds.width, bounds.height), (VIEW.width - 48.0, 30.0));
}

#[test]
fn the_row_dragged_as_its_own_copy_is_as_faint_as_its_ground() {
    let drawn = Cell::new(None);
    let keys: Vec<u64> = (0..20).collect();
    let row = |_| {
        let drawn = &drawn;
        Probe {
            height: Length::Fixed(30.0),
            drawn,
        }
    };
    let rows = list(keys.len(), |index| keys[index], row, still()).reorder(
        |_| true,
        |_, _| true,
        Message::Dragged,
    );
    let theme = Theme::built_in(Some("catppuccin-latte"));
    let mut ui = simulate(rows);
    ui.draw(&theme);
    let plain = drawn.take().expect("the rows are drawn");
    pick_up(&mut ui);
    ui.draw(&theme);
    let copy = drawn.take().expect("the copy is drawn last");
    let at = Rectangle::new(Point::new(0.0, 75.0), Size::new(VIEW.width, 30.0));
    assert_eq!(copy.bounds, at, "the row itself, held where grabbed");
    assert!((copy.text - plain.text * 0.6).abs() < 1e-6, "its colours");
    assert!((copy.style - plain.style * 0.6).abs() < 1e-6, "its text");
}
