//! The pointer each part of the stack is given when the window is drawn,
//! which iced's widgets take their hover from: none under a layer with a
//! backdrop, nor in a layer that's covered or on its way out.

use std::cell::Cell;
use std::rc::Rc;

use iced::Widget as _;
use iced::advanced::mouse::Cursor;
use iced::widget::{button, container, text};

use anomi_theme::{Element, Theme};

use super::super::{Backdrop, Layer, layer, layers};
use super::{Asked, base, card, front, probe, simulate};

type Seen = Rc<Cell<Option<Cursor>>>;

/// What the base gets for a pointer on its button, outside any card, when
/// the window is drawn under `over`.
fn base_sees(over: Option<Layer<'static, Asked>>) -> Option<Cursor> {
    let seen = Seen::default();
    let base = probe::probe(base(), Rc::default()).pointer(seen.clone());
    let mut ui = simulate(layers(base, front()).push_maybe(over).boxed());
    let under = ui.find("under").expect("the base's button");
    ui.point_at(under.visible_bounds().expect("shown").center());
    ui.draw(&Theme::default());
    seen.get()
}

#[test]
fn what_lies_under_a_backdrop_is_drawn_without_the_pointer() {
    let on_it = |seen: Option<Cursor>| seen.and_then(Cursor::position).is_some();
    assert!(on_it(base_sees(None)), "nothing over it");
    let note = layer("note", card("note")).backdrop(Backdrop::None);
    assert!(
        on_it(base_sees(Some(note))),
        "around a card without backdrop"
    );

    let dialog = layer("dialog", card("dialog")).dialog();
    assert_eq!(base_sees(Some(dialog)), Some(Cursor::Unavailable));
    let menu = layer("menu", card("menu"));
    assert_eq!(base_sees(Some(menu)), Some(Cursor::Unavailable));
}

/// A dialog with a button in it, noting in `seen` the pointer it's drawn
/// with, open as `open` says, with a menu over it if `menu`.
fn dialog(seen: Seen, open: bool, menu: bool) -> Element<'static, Asked> {
    let inside = button(text("inside")).on_press(Asked::Base);
    let inside = probe::probe(container(inside).padding(20), Rc::default()).pointer(seen);
    layers(base(), front())
        .push(layer("dialog", inside).open(open).dialog())
        .push_maybe(menu.then(|| layer("menu", card("menu"))))
        .boxed()
}

#[test]
fn a_covered_layer_and_one_on_its_way_out_are_drawn_without_the_pointer() {
    let theme = Theme::default();
    let seen = Seen::default();
    let mut ui = simulate(dialog(seen.clone(), true, false));
    let inside = ui.find("inside").expect("the dialog's button");
    let at = inside.visible_bounds().expect("shown").center();
    ui.point_at(at);
    ui.draw(&theme);
    assert_eq!(seen.get(), Some(Cursor::Available(at)), "open on top");

    let mut ui = ui.rebuild(dialog(seen.clone(), true, true));
    ui.draw(&theme);
    assert_eq!(seen.get(), Some(Cursor::Unavailable), "under a menu");

    let mut ui = ui.rebuild(dialog(seen.clone(), false, false));
    ui.draw(&theme);
    assert_eq!(seen.get(), Some(Cursor::Unavailable), "on its way out");
}
