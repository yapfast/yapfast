//! The theme editor: task S07's tests, driving
//! `anomi_settings::appearance::editor::view` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::appearance::editor::view;
use anomi_vm::fixtures::settings::theme_editor as sample;
use anomi_vm::settings::theme_editor::{Intent, ThemeEditor};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    editor: &ThemeEditor,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(editor, front()).boxed())
}

/// A window tall enough for the colours and the delete button after them.
fn tall(editor: &ThemeEditor) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(
        settings(),
        iced::Size::new(900.0, 1600.0),
        view(editor, front()).boxed(),
    )
}

#[test]
fn the_buttons_ask_for_their_actions() {
    let editor = sample::editor();
    let mut ui = simulate(&editor);
    ui.click("Save").expect("save is there");
    ui.click("Cancel").expect("cancel is there");
    ui.click("Export").expect("export is there");
    assert_eq!(asked(ui), [Intent::Save, Intent::Cancel, Intent::Export]);
}

#[test]
fn a_new_theme_picks_its_start_and_a_saved_one_has_none() {
    let new = sample::editor();
    assert!(simulate(&new).find("START FROM").is_ok());
    assert!(simulate(&new).find("NEW THEME").is_ok());
    let saved = sample::saved();
    let mut ui = simulate(&saved);
    assert!(ui.find("START FROM").is_err());
    assert!(ui.find("EDIT THEME").is_ok());
}

#[test]
fn a_wrong_draft_says_so_and_turns_save_and_export_off() {
    let mut editor = sample::saved();
    assert!(simulate(&editor).find("Not a colour").is_err());
    editor.apply(&Intent::Colour(2, "#zz".into()));
    let mut ui = simulate(&editor);
    assert!(ui.find("Not a colour").is_ok());
    let _ = ui.click("Save");
    let _ = ui.click("Export");
    assert!(asked(ui).is_empty());
    editor.apply(&Intent::Colour(2, "#123456".into()));
    assert!(simulate(&editor).find("Not a colour").is_err());
    assert!(editor.can_save());
}

#[test]
fn delete_asks_before_it_deletes() {
    let mut editor = sample::saved();
    let mut ui = tall(&editor);
    ui.click("Delete theme").expect("delete is there");
    assert_eq!(asked(ui), [Intent::Delete]);
    editor.apply(&Intent::Delete);
    let mut ui = tall(&editor);
    ui.click("Delete it for good").expect("it asks again");
    assert_eq!(asked(ui), [Intent::ConfirmDelete]);
}

#[test]
fn the_dark_switch_and_a_colour_field_send_what_was_set() {
    let editor = sample::editor();
    let mut ui = simulate(&editor);
    ui.click("Dark").expect("the switch is there");
    assert_eq!(asked(ui), [Intent::Dark(false)]);
    let mut ui = simulate(&editor);
    ui.click("#5865f2").expect("accent's field");
    ui.typewrite("0");
    let asked = asked(ui);
    assert!(
        matches!(asked.last(), Some(Intent::Colour(20, text)) if text.len() == 8 && text.contains('0')),
        "{asked:?}"
    );
}
