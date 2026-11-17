//! The opener and the hand-laid row in the simulator: where a menu opens
//! from inside a scrolled list, and a row's click, keys and focus. The
//! context menu itself is tested over the window, in anomi-screens'
//! `tests/areas/menu.rs`.

use iced::advanced::widget::Id;
use iced::advanced::widget::operation::Animation;
use iced::advanced::widget::operation::focusable::focus;
use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, scroll_to};
use iced::keyboard::key::Named;
use iced::widget::{column, scrollable, space, text};
use iced::{Point, Rectangle, Settings, Size, mouse};

use crate::icon::Icon;
use iced_test::Simulator;

use anomi_theme::{Element, Motion, Renderer, Theme};
use anomi_vm::menu::Row;

use super::{item, opener};

#[derive(Clone, Debug, PartialEq)]
enum Asked {
    From(Rectangle),
    At(Point),
    Chose(&'static str),
}

fn simulate(view: Element<'_, Asked>) -> Simulator<'_, Asked, Theme, Renderer> {
    Simulator::with_size(Settings::default(), Size::new(400.0, 300.0), view)
}

fn list() -> Element<'static, Asked> {
    use iced::Widget as _;
    let row = opener(text("Bob"))
        .on_press(Asked::From)
        .on_right_press(Asked::At);
    scrollable(column![space().height(400), row, space().height(400)])
        .id(Id::new("list"))
        .smooth_scroll(false)
        .height(200)
        .boxed()
}

#[test]
fn an_opener_in_a_scrolled_list_tells_where_it_is_in_the_window() {
    let mut ui = simulate(list());
    let mut scroll = scroll_to(
        Id::new("list"),
        AbsoluteOffset {
            x: None,
            y: Some(320.0),
        },
        Animation::Instant,
    );
    ui.operate(&mut scroll);
    // The scrollable takes the offset at its next frame.
    ui.draw(&Theme::default());
    let shown = ui
        .find("Bob")
        .expect("Bob")
        .visible_bounds()
        .expect("shown");
    assert!(shown.y < 200.0, "scrolled into view: {shown:?}");
    let at = shown.center();
    ui.point_at(at);
    ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Right,
    ))]);
    ui.simulate(iced_test::simulator::click());
    let asked: Vec<_> = ui.into_messages().collect();
    assert_eq!(asked[0], Asked::At(at));
    let Asked::From(bounds) = asked[1] else {
        panic!("{asked:?}")
    };
    assert_eq!(bounds.position(), shown.position());
}

fn rows() -> Vec<Row<Asked>> {
    vec![
        Row::new("Upload a File", Asked::Chose("upload")).icon(Icon::File),
        Row::new("Create Poll", Asked::Chose("poll"))
            .icon(Icon::ChartBar)
            .disabled(true),
    ]
}

fn menu(rows: &[Row<Asked>]) -> Element<'_, Asked> {
    use iced::Widget as _;
    let motion = Motion::default();
    column![
        item(&rows[0], motion).id(Id::new("upload")),
        item(&rows[1], motion)
    ]
    .width(224)
    .boxed()
}

#[test]
fn a_row_sends_its_intent_when_clicked_or_chosen_with_the_keys() {
    let rows = rows();
    let mut ui = simulate(menu(&rows));
    ui.click("Upload a File").expect("the row");
    ui.click("Create Poll").expect("the disabled row");
    ui.tap_key(Named::Enter);
    ui.operate(&mut focus(Id::new("upload")));
    ui.tap_key(Named::Enter);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [Asked::Chose("upload"), Asked::Chose("upload")]
    );
}
