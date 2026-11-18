//! The stack in the simulator: Escape and clicks close the top layer only,
//! what's under a layer gets neither, nor a paste; focus stays inside, what
//! had it under a layer loses it and has it back when it closes; and a
//! layer leaves over `LEAVE`, at once while still; what lies under a layer
//! is drawn without the pointer; and a layer at a spot keeps the room under
//! it that its opener asks for.

mod place;
mod pointer;
mod probe;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use iced::advanced::clipboard;
use iced::advanced::widget::operation::focusable::focus;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::key::Named;
use iced::widget::{button, container, text, text_input};
use iced::{Event, Length, Point, Settings, Size};
use iced_test::Simulator;

use anomi_theme::motion::LEAVE;
use anomi_theme::{Element, Motion, Renderer, Theme};

use super::{Backdrop, layer, layers};

#[derive(Clone, Debug, PartialEq)]
enum Asked {
    Base,
    Close(&'static str),
    Gone(&'static str),
    Composer(String),
    Field(String),
}

const SIZE: Size = Size::new(400.0, 300.0);

fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

fn simulate(view: Element<'_, Asked>) -> Simulator<'_, Asked, Theme, Renderer> {
    Simulator::with_size(Settings::default(), SIZE, view)
}

/// A button in the base's corner, under whatever opens.
fn base<'a>() -> Element<'a, Asked> {
    use iced::Widget as _;
    container(button(text("under")).on_press(Asked::Base))
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

fn card(name: &'static str) -> Element<'static, Asked> {
    use iced::Widget as _;
    container(text(name)).padding(20).boxed()
}

fn two_layers() -> Element<'static, Asked> {
    use iced::Widget as _;
    layers(base(), front())
        .push(layer("a", card("first")).on_close(Asked::Close("a")))
        .push(layer("b", card("second")).on_close(Asked::Close("b")))
        .boxed()
}

#[test]
fn escape_closes_the_top_layer_only() {
    let mut ui = simulate(two_layers());
    ui.tap_key(Named::Escape);
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Close("b")]);
}

/// A layer with a list to drop down, as the authorize dialog's servers.
fn dropping() -> Element<'static, Asked> {
    use iced::Widget as _;
    let list = iced::widget::pick_list(Some("one"), ["one", "two"], |name| name.to_string())
        .on_select(|_| Asked::Base);
    layers(base(), front())
        .push(layer("a", container(list).padding(20).boxed()).on_close(Asked::Close("a")))
        .boxed()
}

#[test]
fn escape_closes_a_dropped_list_before_its_layer() {
    let mut ui = simulate(dropping());
    ui.click("one").expect("the list's face");
    ui.tap_key(Named::Escape);
    assert_eq!(ui.drain().count(), 0, "the list closed, the layer stays");
    ui.tap_key(Named::Escape);
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Close("a")]);
}

#[test]
fn a_press_outside_the_card_closes_it_and_reaches_nothing_under_it() {
    let mut ui = simulate(two_layers());
    ui.click("under").expect("the base's button");
    ui.click("second").expect("the top card");
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Close("b")]);
}

#[test]
fn a_layer_without_backdrop_lets_the_pointer_through_around_it() {
    use iced::Widget as _;
    let view = layers(base(), front())
        .push(
            layer("note", card("note"))
                .backdrop(Backdrop::None)
                .on_close(Asked::Close("note")),
        )
        .boxed();
    let mut ui = simulate(view);
    ui.click("under").expect("the base's button");
    ui.tap_key(Named::Escape);
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Base]);
}

fn with_dialog(dialog: bool) -> Element<'static, Asked> {
    dialog_over(Rc::default(), dialog)
}

/// The composer, noting in `focused` whether it has the focus, under a
/// dialog with a field 20 px inside its card.
fn dialog_over(focused: Rc<Cell<bool>>, dialog: bool) -> Element<'static, Asked> {
    use iced::Widget as _;
    let composer = text_input("", "")
        .id(Id::new("composer"))
        .on_input(Asked::Composer);
    let field = text_input("", "")
        .id(Id::new("field"))
        .on_input(Asked::Field);
    let field = container(field).width(200).padding(20);
    layers(probe::probe(composer, focused), front())
        .push_maybe(dialog.then(|| layer("dialog", field).dialog()))
        .boxed()
}

/// Presses and lets go at `at`.
fn click_at(ui: &mut Simulator<'_, Asked, Theme, Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
}

#[test]
fn focus_stays_inside_a_layer_and_comes_back_when_it_closes() {
    let mut ui = simulate(with_dialog(false));
    ui.operate(&mut focus(Id::new("composer")));
    ui.typewrite("a");
    assert_eq!(
        ui.drain().collect::<Vec<_>>(),
        [Asked::Composer("a".into())]
    );

    let mut ui = ui.rebuild(with_dialog(true));
    ui.typewrite("b");
    // Nothing in the dialog has the focus yet, and the composer under it
    // doesn't get the keys.
    assert_eq!(ui.drain().count(), 0);
    ui.operate(&mut focus(Id::new("field")));
    ui.typewrite("c");
    assert_eq!(ui.drain().collect::<Vec<_>>(), [Asked::Field("c".into())]);
    // A click inside the dialog's card, off its field, and a press outside
    // it reach nothing under it.
    let field = ui
        .find(Id::new("field"))
        .expect("the field")
        .visible_bounds()
        .expect("shown");
    click_at(&mut ui, Point::new(field.center_x(), field.y - 10.0));
    click_at(&mut ui, Point::new(5.0, 5.0));
    assert_eq!(ui.drain().count(), 0);
    // The composer can't take the focus from under the dialog: asking for
    // it finds nothing (and, as iced's `focus` does, unfocuses the rest).
    ui.operate(&mut focus(Id::new("composer")));
    ui.typewrite("d");
    assert!(ui.drain().all(|asked| !matches!(asked, Asked::Composer(_))));

    let mut ui = ui.rebuild(with_dialog(false));
    ui.typewrite("e");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [Asked::Composer("e".into())]
    );
}

#[test]
fn a_paste_goes_to_the_field_in_the_dialog() {
    let mut ui = simulate(with_dialog(false));
    ui.operate(&mut focus(Id::new("composer")));
    let mut ui = ui.rebuild(with_dialog(true));
    ui.operate(&mut focus(Id::new("field")));
    let pasted = clipboard::Content::Text("x".into());
    ui.simulate([Event::Clipboard(clipboard::Event::Read(Ok(Arc::new(
        pasted,
    ))))]);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [Asked::Field("x".into())]
    );
}

/// The dialog with its field, which has no id, and a menu over it.
fn menu_over_dialog(menu: bool) -> Element<'static, Asked> {
    use iced::Widget as _;
    let field = text_input("name", "").on_input(Asked::Field);
    layers(base(), front())
        .push(layer("dialog", container(field).width(200)).dialog())
        .push_maybe(menu.then(|| layer("menu", card("menu")).on_close(Asked::Close("menu"))))
        .boxed()
}

#[test]
fn a_menu_over_a_dialog_gives_its_field_the_focus_back() {
    let mut ui = simulate(menu_over_dialog(false));
    ui.click("name").expect("the field");
    ui.typewrite("a");
    assert_eq!(ui.drain().collect::<Vec<_>>(), [Asked::Field("a".into())]);

    let mut ui = ui.rebuild(menu_over_dialog(true));
    ui.click("menu").expect("the menu's card");
    ui.typewrite("b");
    assert_eq!(ui.drain().count(), 0, "the keys go to the menu");

    let mut ui = ui.rebuild(menu_over_dialog(false));
    ui.typewrite("c");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [Asked::Field("c".into())]
    );
}

/// An operation that only walks the tree, so the probe looks.
struct Look;

impl Operation for Look {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

#[test]
fn what_had_the_focus_under_a_dialog_loses_it_until_it_closes() {
    let focused = Rc::new(Cell::new(false));
    let mut ui = simulate(dialog_over(focused.clone(), false));
    ui.operate(&mut focus(Id::new("composer")));
    assert!(focused.get());

    let mut ui = ui.rebuild(dialog_over(focused.clone(), true));
    ui.operate(&mut Look);
    assert!(!focused.get(), "no caret under the dialog");
    // Nor does a press under it give it back.
    click_at(&mut ui, Point::new(5.0, 5.0));
    ui.operate(&mut Look);
    assert!(!focused.get());

    let mut ui = ui.rebuild(dialog_over(focused.clone(), false));
    ui.operate(&mut Look);
    assert!(focused.get(), "the focus is back");
    ui.typewrite("f");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [Asked::Composer("f".into())]
    );
}

fn leaving(open: bool, motion: Motion) -> Element<'static, Asked> {
    use iced::Widget as _;
    layers(base(), motion)
        .push(
            layer("a", card("card"))
                .open(open)
                .on_close(Asked::Close("a"))
                .on_gone(Asked::Gone("a")),
        )
        .boxed()
}

#[test]
fn a_layer_leaves_over_leave_and_says_when_it_has_gone() {
    let theme = Theme::default();
    let mut ui = simulate(leaving(true, front()));
    ui.draw(&theme);
    let mut ui = ui.rebuild(leaving(false, front()));
    ui.draw(&theme);
    assert_eq!(ui.drain().count(), 0, "still on its way out");
    // On its way out it takes no clicks: they reach the base.
    ui.click("under").expect("the base's button");
    assert_eq!(ui.drain().collect::<Vec<_>>(), [Asked::Base]);
    std::thread::sleep(LEAVE + Duration::from_millis(30));
    ui.draw(&theme);
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Gone("a")]);
}

#[test]
fn a_still_window_lets_a_layer_go_at_once() {
    let still = Motion::default();
    let ui = simulate(leaving(true, still));
    let mut ui = ui.rebuild(leaving(false, still));
    ui.draw(&Theme::default());
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Gone("a")]);
}
