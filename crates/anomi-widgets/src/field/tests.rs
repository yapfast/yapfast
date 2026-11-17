//! The field driven through iced's simulator: typing asks for the whole
//! text, Enter submits, a disabled field takes nothing, and a refused draft
//! shows its reason and keeps its text.

use iced::advanced::widget::operation::focusable;
use iced::{Settings, Size};
use iced_test::Simulator;

use anomi_theme::{Element, Renderer, Theme};

use super::*;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Input(String),
    Submit,
    Focus(bool),
}

type Ui<'a> = Simulator<'a, Msg, Theme, Renderer>;

fn simulate<'a>(view: Element<'a, Msg>) -> Ui<'a> {
    Simulator::with_size(Settings::default(), Size::new(320.0, 120.0), view)
}

fn asked(ui: Ui<'_>) -> Vec<Msg> {
    ui.into_messages().collect()
}

fn editable<'a>(value: &'a str) -> Field<'a, Msg> {
    field("Name", value)
        .id("name")
        .on_input(Msg::Input)
        .on_submit(Msg::Submit)
}

#[test]
fn typing_asks_for_the_whole_text_and_enter_submits() {
    let mut ui = simulate(editable("bo").build());
    ui.operate(&mut focusable::focus::<()>("name".into()));
    ui.typewrite("b");
    ui.tap_key(iced::keyboard::Key::Named(
        iced::keyboard::key::Named::Enter,
    ));
    assert_eq!(asked(ui), [Msg::Input("bbo".to_string()), Msg::Submit]);
}

#[test]
fn a_field_without_on_input_takes_no_text() {
    let mut ui = simulate(field("Name", "bo").id("name").build());
    ui.operate(&mut focusable::focus::<()>("name".into()));
    ui.typewrite("b");
    assert!(asked(ui).is_empty());
}

#[test]
fn a_refused_draft_shows_why_and_still_holds_its_text() {
    let mut draft = Draft::new("123456");
    draft.reject("That code is wrong");
    let mut ui = simulate(editable("").draft(&draft).build());
    assert!(ui.find("That code is wrong").is_ok());
    // The text is still there to edit: a key goes in at the caret.
    ui.operate(&mut focusable::focus::<()>("name".into()));
    ui.typewrite("7");
    assert_eq!(asked(ui), [Msg::Input("7123456".to_string())]);
}

#[test]
fn an_italic_placeholder_shows_only_while_the_field_is_empty() {
    let styled = |value| editable(value).placeholder_style(None, true).build();
    let mut empty = simulate(styled(""));
    assert!(empty.find("Name").is_ok());
    let mut filled = simulate(styled("bob"));
    assert!(filled.find("Name").is_err());
}

#[test]
fn every_option_draws() {
    let theme = Theme::default();
    let build = || {
        editable("")
            .password(true)
            .fill(iced::Color::TRANSPARENT)
            .ring(false)
            .rim(true)
            .ink(iced::Color::WHITE)
            .text_size(14.0)
            .pad(8.0)
            .trailing(Icon::X, None)
            .placeholder_style(Some(iced::Color::WHITE), true)
            .build()
    };
    simulate(build()).draw(&theme);
    simulate(field("Name", "off").build()).draw(&theme);
}

fn height_of(field: Field<'_, Msg>) -> f32 {
    let mut ui = simulate(field.id("h").build());
    ui.find(iced_test::selector::id("h"))
        .unwrap()
        .bounds()
        .height
}

#[test]
fn the_wells_height_is_a_buttons() {
    let height = height_of(field("", ""));
    assert!((height - metrics::CONTROL_HEIGHT).abs() < 0.01, "{height}");
}

#[test]
fn a_height_sets_the_wells() {
    let tall = height_of(field("", "").height(40.0));
    assert!((tall - 40.0).abs() < 0.01, "{tall}");
    // The 52 px well of a dialog's 20 px text.
    let big = height_of(field("", "").height(52.0).text_size(20.0));
    assert!((big - 52.0).abs() < 0.01, "{big}");
}

#[test]
fn an_error_given_as_text_shows_without_a_draft() {
    let mut ui = simulate(editable("x").error(Some("Too short")).build());
    assert!(ui.find("Too short").is_ok());
    let mut ui = simulate(editable("x").error(None).build());
    assert!(ui.find("Too short").is_err());
}

#[test]
fn on_focus_tells_gaining_and_losing_the_keys() {
    let mut ui = simulate(editable("").on_focus(Msg::Focus).build());
    ui.operate(&mut focusable::focus::<()>("name".into()));
    // A press below the well, outside it.
    ui.point_at(iced::Point::new(10.0, 110.0));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
        iced::mouse::Button::Left,
    ))]);
    assert_eq!(asked(ui), [Msg::Focus(true), Msg::Focus(false)]);
}

/// Types `first`, rebuilds as the app would with the new value, types
/// `second`: the field must still have the keys.
fn type_across_rebuild<'a>(
    before: Element<'a, Msg>,
    after: Element<'a, Msg>,
    first: &str,
    second: &str,
) -> Vec<Msg> {
    let mut ui = simulate(before);
    ui.operate(&mut focusable::focus::<()>("name".into()));
    ui.typewrite(first);
    let mut ui = ui.rebuild(after);
    ui.typewrite(second);
    asked(ui)
}

#[test]
fn the_first_key_into_an_italic_placeholder_field_keeps_the_keys() {
    let got = type_across_rebuild(
        editable("").placeholder_style(None, true).build(),
        editable("a").placeholder_style(None, true).build(),
        "a",
        "b",
    );
    assert_eq!(got, [Msg::Input("a".into()), Msg::Input("ab".into())]);
}

#[test]
fn an_error_appearing_keeps_the_keys() {
    let got = type_across_rebuild(
        editable("12").build(),
        editable("312").error(Some("Wrong")).build(),
        "3",
        "4",
    );
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0], Msg::Input("312".into()));
}

#[test]
fn an_error_clearing_keeps_the_keys() {
    let got = type_across_rebuild(
        editable("12").error(Some("Wrong")).build(),
        editable("312").build(),
        "3",
        "4",
    );
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0], Msg::Input("312".into()));
}
