//! The kit's parts in the simulator: what a click on each asks for.

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_theme::{Motion, Renderer, Theme, fonts, typography};
use anomi_vm::icon::Icon;

use super::{choices, link, nav_row, switch};

const SIZE: Size = Size::new(600.0, 200.0);

fn settings() -> iced::Settings {
    fonts::install();
    iced::Settings {
        font: typography::UI,
        ..iced::Settings::default()
    }
}

fn asked<M>(ui: Simulator<'_, M, Theme, Renderer>) -> Vec<M> {
    ui.into_messages().collect()
}

#[test]
fn a_click_on_a_switch_or_its_label_asks_for_the_flipped_value() {
    for on in [false, true] {
        let view = switch("Mute", on, Motion::default())
            .description("Silences it")
            .on_toggle(|value| value)
            .view();
        let mut ui = Simulator::with_size(settings(), SIZE, view);
        ui.click("Mute").expect("the label");
        assert_eq!(asked(ui), [!on]);
    }
}

#[test]
fn a_click_on_the_track_flips_it_and_a_locked_switch_takes_none() {
    let view = switch("Mute", false, Motion::default())
        .on_toggle(|value| value)
        .view();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    ui.point_at(Point::new(576.0, 12.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [true]);
    let locked = switch("Mute", true, Motion::default())
        .enabled(false)
        .locked("Discord decides")
        .on_toggle(|value| value)
        .view();
    let mut ui = Simulator::with_size(settings(), SIZE, locked);
    ui.point_at(Point::new(576.0, 12.0));
    ui.simulate(iced_test::simulator::click());
    assert!(asked(ui).is_empty());
}

#[test]
fn a_click_on_a_choice_asks_for_its_place() {
    let options = ["Off", "Quiet", "Loud"];
    let view = choices(&options, 0, Motion::default())
        .on_pick(|index| index)
        .view();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    // The well is as wide as its words need, at least 360 px: the third
    // option lies in its last third.
    ui.point_at(Point::new(330.0, 18.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [2]);
}

#[test]
fn a_choice_is_named_by_its_words_and_sent_on_the_release() {
    let options = ["Off", "Quiet", "Loud"];
    let view = choices(&options, 0, Motion::default())
        .on_pick(|index| index)
        .view();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    ui.click("Quiet").expect("an option named by its words");
    assert_eq!(asked(ui), [1]);
    // A press on one option released over another asks nothing.
    let view = choices(&options, 0, Motion::default())
        .on_pick(|index| index)
        .view();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    ui.point_at(Point::new(330.0, 18.0));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
        iced::mouse::Button::Left,
    ))]);
    ui.point_at(Point::new(30.0, 18.0));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
        iced::mouse::Button::Left,
    ))]);
    assert!(asked(ui).is_empty());
}

#[test]
fn a_choice_set_wraps_onto_more_lines_in_a_narrow_row() {
    let options = ["Always on", "Only in a call", "Never at all"];
    let view = choices(&options, 1, Motion::default())
        .wrap(true)
        .on_pick(|index| index)
        .view();
    let mut ui = Simulator::with_size(settings(), Size::new(200.0, 200.0), view);
    // The third option sits on a line of its own, 36 px lower.
    ui.point_at(Point::new(60.0, 4.0 + 28.0 * 2.0 + 8.0 + 14.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [2]);
}

#[test]
fn a_choice_set_wraps_in_a_narrow_row_without_being_asked() {
    let options = [
        "Always on everywhere",
        "Only in a call with friends",
        "Never at all in any case",
    ];
    let view = choices(&options, 0, Motion::default())
        .on_pick(|index| index)
        .view();
    let mut ui = Simulator::with_size(settings(), Size::new(400.0, 200.0), view);
    ui.point_at(Point::new(60.0, 50.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui).len(), 1);
}

#[test]
fn a_link_and_a_section_row_ask_when_pressed() {
    let view = link("Remove", Motion::default()).on_press(1).view();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    ui.click("Remove").expect("the link");
    assert_eq!(asked(ui), [1]);
    let row = nav_row(
        Icon::User,
        "Roles",
        false,
        false,
        true,
        7,
        Motion::default(),
    );
    let mut ui = Simulator::with_size(settings(), SIZE, row);
    ui.click("Roles").expect("the row");
    assert_eq!(asked(ui), [7]);
}

#[test]
fn a_disabled_link_asks_nothing() {
    let view = link("Remove", Motion::default())
        .enabled(false)
        .on_press(1)
        .view();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    ui.click("Remove").expect("the link");
    assert!(asked(ui).is_empty());
}
