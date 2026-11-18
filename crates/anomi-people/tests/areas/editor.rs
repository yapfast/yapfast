//! The profile editor: task P14's tests, driving `anomi_people::editor::view`
//! over its fixtures in the simulator (`super::support`). Clicks are found by
//! the code's English text.

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_people::editor::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::people::editor as samples;
use anomi_vm::people::Intent;
use anomi_vm::people::editor::{Editor, Field, Intent as Edit};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(1280.0, 800.0);

fn editor(editor: &Editor) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(editor, front()))
}

fn asks(edit: Edit) -> Intent {
    Intent::Editor(edit)
}

/// What `ui` asked besides ending the editing: a press the dialog's parts
/// don't take ends it (`Edit(None)`), as the card's touch area does in
/// Slint (`ui/server.slint:1737`).
fn asked_beside_ending(ui: Simulator<'_, Intent, Theme, Renderer>) -> Vec<Intent> {
    let ending = asks(Edit::Edit(None));
    asked(ui)
        .into_iter()
        .filter(|intent| *intent != ending)
        .collect()
}

/// What a click at `at` asks for. The main profile's dialog is 1199 by 670
/// in SIZE, its left edge at 40.5 and its top at 65.
fn click_at(sample: &Editor, at: Point) -> Vec<Intent> {
    let mut ui = editor(sample);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn the_main_profile_shows_its_panel_and_card() {
    let sample = samples::main();
    let mut ui = editor(&sample);
    for text in [
        "Main Profile",
        "Nameplate",
        "Avatar",
        "Decoration",
        "Display Name Style",
        "Theme",
        "Banner",
        "Effect",
        "Frame",
        "Alice",
        "alice \u{b7}",
        "Add pronouns",
        "Bio",
        "Member Since",
        "Connections",
        "Note (only visible to you)",
    ] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn a_folded_panel_leaves_the_card_alone() {
    let mut sample = samples::main();
    sample.side_open = false;
    let mut ui = editor(&sample);
    assert!(ui.find("Main Profile").is_err());
    ui.find("Alice").expect("the card stays");
}

#[test]
fn a_server_profile_has_no_panel_but_nitros_offer() {
    let sample = samples::server();
    let mut ui = editor(&sample);
    assert!(ui.find("Main Profile").is_err());
    ui.find("Make unique profiles for each server")
        .expect("the offer");
    ui.click("Unlock with Nitro").expect("its button");
    assert_eq!(asked(ui), [asks(Edit::UnlockNitro)]);
}

#[test]
fn a_lesser_nitro_is_asked_to_upgrade() {
    let mut sample = samples::server();
    sample.upgrade = true;
    let mut ui = editor(&sample);
    ui.find("Upgrade Nitro").expect("upgrade");
    assert!(ui.find("Unlock with Nitro").is_err());
}

#[test]
fn the_name_pronouns_and_bio_open_for_editing() {
    // The panel's name style tile also says the name: fold it away.
    let mut sample = samples::main();
    sample.side_open = false;
    let mut ui = editor(&sample);
    ui.click("Alice").expect("the name");
    ui.click("Add pronouns").expect("the pronouns");
    ui.click("Tell everyone a bit about yourself")
        .expect("the bio");
    assert_eq!(
        asked(ui),
        [
            asks(Edit::Edit(Some(Field::Name))),
            asks(Edit::Edit(Some(Field::Pronouns))),
            asks(Edit::Edit(Some(Field::Bio))),
        ]
    );
}

#[test]
fn a_name_this_account_may_not_change_does_not_open() {
    let mut sample = samples::main();
    sample.side_open = false;
    sample.may.name = false;
    let mut ui = editor(&sample);
    let _ = ui.click("Alice");
    assert_eq!(asked_beside_ending(ui), []);
}

#[test]
fn a_name_being_edited_is_a_field() {
    let mut sample = samples::main();
    sample.editing = Some(Field::Name);
    sample.name = "Alice W".into();
    let mut ui = editor(&sample);
    ui.find("Alice W")
        .expect("the field holds the written name");
}

#[test]
fn the_connections_and_the_note_ask_for_their_pages() {
    let sample = samples::main();
    let mut ui = editor(&sample);
    ui.click("+  Add Connection").expect("connections");
    ui.click("Click to add a note").expect("the note");
    assert_eq!(
        asked(ui),
        [
            asks(Edit::OpenLink(
                "https://discord.com/settings/connections".into()
            )),
            asks(Edit::EditNote),
        ]
    );
}

#[test]
fn a_change_asks_to_be_saved_or_undone() {
    let sample = samples::changed();
    let mut ui = editor(&sample);
    ui.find("Careful \u{2014} you have unsaved changes!")
        .expect("the bar");
    ui.click("Reset").expect("reset");
    ui.click("Save Changes").expect("save");
    assert_eq!(asked(ui), [asks(Edit::Undo), asks(Edit::Save)]);
}

#[test]
fn nothing_changed_shows_no_bar() {
    let sample = samples::main();
    let mut ui = editor(&sample);
    assert!(ui.find("Save Changes").is_err());
}

#[test]
fn a_text_too_long_cannot_be_saved() {
    let mut sample = samples::changed();
    sample.too_long = "A display name can be up to 32 characters.".into();
    let mut ui = editor(&sample);
    ui.find("A display name can be up to 32 characters.")
        .expect("said");
    let _ = ui.click("Save Changes");
    assert_eq!(asked_beside_ending(ui), []);
}

#[test]
fn the_banner_colours_box_keeps_a_wrong_draft() {
    let mut sample = samples::main();
    sample.colour_open = true;
    sample.colour = "#zz".into();
    sample.colour_wrong = true;
    let mut ui = editor(&sample);
    ui.find("Banner color").expect("the box");
    ui.find("#zz").expect("the draft stays");
    ui.find("Not a colour").expect("and is refused");
}

#[test]
fn nitro_opens_the_banner_and_the_look() {
    let sample = samples::nitro();
    assert!(sample.may_save() && sample.banner_editable());
    assert!(!sample.banner_locked());
}

#[test]
fn the_written_name_goes_out_as_it_is_typed() {
    let mut sample = samples::main();
    sample.side_open = false;
    sample.editing = Some(Field::Name);
    sample.name = "Alice".into();
    let mut ui = editor(&sample);
    ui.click("Alice").expect("the field");
    ui.typewrite("!");
    let asked = asked(ui);
    assert_eq!(
        asked.last(),
        Some(&asks(Edit::Write(Field::Name, "Alice!".into())))
    );
}

#[test]
fn the_tag_chip_opens_its_chooser() {
    let sample = samples::main();
    let mut ui = editor(&sample);
    ui.click("Server Tag").expect("the chip");
    assert!(matches!(
        asked_beside_ending(ui).as_slice(),
        [Intent::Editor(Edit::TagMenu(_))]
    ));
}

#[test]
fn the_panel_hides_and_the_dialog_closes() {
    let sample = samples::main();
    assert_eq!(
        click_at(&sample, Point::new(257.0, 88.0)),
        [asks(Edit::Side(false))]
    );
    assert_eq!(
        click_at(&sample, Point::new(1206.0, 88.0)),
        [asks(Edit::Close)]
    );
}

#[test]
fn the_banner_opens_its_colour_and_the_avatar_its_picker() {
    let sample = samples::main();
    assert_eq!(
        click_at(&sample, Point::new(600.0, 120.0)),
        [asks(Edit::ColourBox(true))]
    );
    assert_eq!(
        click_at(&sample, Point::new(370.0, 280.0)),
        [asks(Edit::Pick(anomi_vm::people::editor::Picture::Avatar))]
    );
}

#[test]
fn the_thought_changes_the_status() {
    let sample = samples::main();
    // The bubble: 18 px past the avatar's ring, 17 px under the banner.
    assert_eq!(
        click_at(&sample, Point::new(520.0, 290.0)),
        [asks(Edit::EditStatus)]
    );
}

#[test]
fn the_bio_is_written_over_lines() {
    let mut sample = samples::main();
    sample.side_open = false;
    sample.editing = Some(Field::Bio);
    let mut ui = editor(&sample);
    // The box, under the bio's heading in the card 169 px in from the left.
    ui.point_at(Point::new(300.0, 540.0));
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("Hi");
    let asked = asked(ui);
    assert_eq!(
        asked.last(),
        Some(&asks(Edit::Write(Field::Bio, "Hi".into())))
    );
}
