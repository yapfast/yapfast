//! A bot's forms and select menus: task C20's tests, driving
//! `anomi_chat::bot_forms::view` over its fixtures in the simulator
//! (`super::support`).

use iced_test::Simulator;

use anomi_chat::bot_forms::{select, view};
use anomi_vm::chat::bot_forms::{BotForms, Intent};
use anomi_vm::fixtures::chat::bot_forms as sample;

use super::support::{self, SIZE};

fn form(form: anomi_vm::chat::bot_forms::Form) -> BotForms {
    BotForms {
        form,
        ..BotForms::default()
    }
}

fn asked(forms: &BotForms, click: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(forms, support::front()));
    ui.click(click).unwrap();
    support::asked(ui)
}

fn picked(select_menu: anomi_vm::chat::bot_forms::Select, click: &str) -> Vec<Intent> {
    let forms = BotForms {
        select: select_menu,
        ..BotForms::default()
    };
    let mut ui = Simulator::with_size(support::settings(), SIZE, select(&forms, support::front()));
    ui.click(click).unwrap();
    support::asked(ui)
}

/// The form's own field, alone, so it is in view.
fn alone(index: usize) -> BotForms {
    let mut form = sample::form();
    form.fields = vec![form.fields.swap_remove(index)];
    BotForms {
        form,
        ..BotForms::default()
    }
}

/// A press at a point of the card, where its buttons sit: the card fills the
/// window's height, 20 px in from the right edge and the bottom, the buttons
/// are what is left of the row.
fn pressed(forms: &BotForms, x: f32) -> Vec<Intent> {
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(forms, support::front()));
    ui.point_at(iced::Point::new(x, SIZE.height - 54.0));
    ui.simulate(iced_test::simulator::click());
    support::asked(ui)
}

const SUBMIT: f32 = 460.0;
const CANCEL: f32 = 380.0;

#[test]
fn submit_and_cancel_ask() {
    let forms = form(sample::form());
    assert_eq!(pressed(&forms, SUBMIT), vec![Intent::Submit]);
    assert_eq!(pressed(&forms, CANCEL), vec![Intent::Close]);
}

#[test]
fn a_radio_pick_sends_its_value_and_a_checkbox_toggles() {
    let forms = form(sample::form());
    assert_eq!(
        asked(&forms, "Annoying"),
        vec![Intent::Edit(4, "annoying".into())]
    );
    assert_eq!(asked(&forms, "Logs"), vec![Intent::Edit(5, "logs".into())]);
    assert_eq!(
        asked(&forms, "Contact me *"),
        vec![Intent::Edit(6, String::new())]
    );
}

#[test]
fn typing_in_an_input_sends_the_whole_text() {
    let forms = form(sample::form());
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    ui.click("What is it about?").unwrap();
    ui.typewrite("Hi");
    let asked = support::asked(ui);
    assert_eq!(asked.last(), Some(&Intent::Edit(1, "Hi".into())));
}

#[test]
fn a_select_box_opens_its_dropdown_and_files_are_chosen() {
    assert!(matches!(
        asked(&alone(3), "Voice, Chat").as_slice(),
        [Intent::OpenSelect(0, _)]
    ));
    assert_eq!(
        asked(&alone(7), "Choose files"),
        vec![Intent::ChooseFiles(0)]
    );
}

#[test]
fn the_files_box_hides_once_the_upload_is_full() {
    let mut full = sample::form();
    full.fields[7].max_files = 1;
    let forms = form(full);
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    assert!(ui.find("Choose files").is_err());
    assert!(ui.find("crash.log").is_ok());
}

#[test]
fn nothing_can_be_changed_while_it_is_sent() {
    let forms = form(sample::saving());
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    ui.click("Annoying").unwrap();
    assert!(support::asked(ui).is_empty());
    assert!(pressed(&forms, SUBMIT).is_empty());
    assert!(pressed(&forms, CANCEL).is_empty());
}

#[test]
fn a_form_with_a_part_it_cannot_fill_cannot_be_submitted() {
    let forms = form(sample::unsupported());
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    assert!(
        ui.find("This form has a part Yapfast can't fill yet. Use it in Discord.")
            .is_ok()
    );
    drop(ui);
    assert!(pressed(&forms, SUBMIT).is_empty());
}

#[test]
fn an_option_of_the_dropdown_toggles() {
    assert_eq!(picked(sample::several(), "Chat"), vec![Intent::Toggle(1)]);
    assert_eq!(picked(sample::single(), "High"), vec![Intent::Toggle(2)]);
}

#[test]
fn the_dropdown_searches_and_says_when_nothing_is_left() {
    let forms = BotForms {
        select: sample::people(),
        ..BotForms::default()
    };
    let mut ui = Simulator::with_size(support::settings(), SIZE, select(&forms, support::front()));
    ui.click("Search").unwrap();
    ui.typewrite("b");
    assert_eq!(support::asked(ui).last(), Some(&Intent::Search("b".into())));
    let empty = BotForms {
        select: sample::nothing(),
        ..BotForms::default()
    };
    let mut ui = Simulator::with_size(support::settings(), SIZE, select(&empty, support::front()));
    assert!(ui.find("No matching options.").is_ok());
}

#[test]
fn a_chosen_file_and_a_paragraphs_label_show() {
    let forms = alone(7);
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    assert!(ui.find("crash.log").is_ok());
    let area = alone(2);
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&area, support::front()));
    assert!(ui.find("Details").is_ok());
}

#[test]
fn a_chosen_files_x_removes_it() {
    let forms = alone(7);
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    let row = ui.find("crash.log").unwrap().visible_bounds().unwrap();
    // The text fills the row up to the X, which sits 4 px in from the end.
    ui.point_at(iced::Point::new(
        SIZE.width.min(row.x + row.width + 14.0),
        row.center_y(),
    ));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(support::asked(ui), vec![Intent::RemoveFile(0, 0)]);
}

#[test]
fn typing_in_a_paragraph_sends_the_whole_text() {
    let forms = alone(2);
    let text = forms.form.fields[0].value.clone();
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&forms, support::front()));
    let label = ui.find("Details").unwrap().visible_bounds().unwrap();
    ui.point_at(iced::Point::new(
        label.x + 300.0,
        label.y + label.height + 24.0,
    ));
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("!");
    assert_eq!(
        support::asked(ui).last(),
        Some(&Intent::Edit(0, format!("{text}!")))
    );
}
