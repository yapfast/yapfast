//! A server's Emoji settings: task X10's tests, driving
//! `anomi_settings::server::emoji::{view, dialog}` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced::keyboard::{Key, key::Named};
use iced::{Event, Rectangle, mouse};
use iced_test::selector::Candidate;
use iced_test::simulator::{self, Simulator};

use anomi_settings::server::emoji::{dialog, view};
use anomi_vm::fixtures::settings::emoji as sample;
use anomi_vm::settings::emoji::{Emoji, Intent};

use super::support::{SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(page: &Emoji) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

/// Where the uploader's name `words` is drawn, in the row's middle.
fn words(ui: &mut Ui<'_>, words: &str, nth: usize) -> Rectangle {
    let mut seen = 0;
    ui.find(|candidate: Candidate<'_>| match candidate {
        Candidate::Text {
            content, bounds, ..
        } if content == words => {
            seen += 1;
            (seen > nth).then_some(bounds)
        }
        _ => None,
    })
    .expect("the words are there")
}

/// The hovered row's Delete: an icon button, 32 px square, its name only in
/// its tooltip.
fn delete(ui: &mut Ui<'_>) -> Result<Rectangle, iced_test::Error> {
    ui.find(|candidate: Candidate<'_>| match candidate {
        Candidate::Container { bounds, .. } if bounds.width == 32.0 && bounds.height == 32.0 => {
            Some(bounds)
        }
        _ => None,
    })
}

#[test]
fn the_tables_have_their_headings_and_upload_asks_to_open() {
    let page = sample::emoji();
    let mut ui = simulate(&page);
    assert!(ui.find("Animated Emoji").is_ok());
    assert!(ui.find("IMAGE").is_ok());
    ui.click("Upload Emoji").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Upload]);
}

#[test]
fn a_reader_has_no_upload() {
    let mut page = sample::emoji();
    page.may_upload = false;
    assert!(simulate(&page).find("Upload Emoji").is_err());
}

#[test]
fn no_emoji_says_so() {
    let page = Emoji::default();
    assert!(simulate(&page).find("No Emoji").is_ok());
}

#[test]
fn the_dialog_uploads_or_cancels() {
    let upload = sample::upload();
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&upload, front()).boxed());
    ui.click("Upload").expect("the button is there");
    ui.click("Cancel").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Finish, Intent::Cancel]);
}

#[test]
fn an_unfinished_dialog_cannot_upload_and_shows_an_error() {
    let mut upload = sample::upload();
    upload.may_finish = false;
    upload.error = "Too big".into();
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&upload, front()).boxed());
    assert!(ui.find("Too big").is_ok());
    let _ = ui.click("Upload");
    assert!(asked(ui).is_empty());
}

#[test]
fn delete_shows_on_the_hovered_row_and_deletes_it() {
    let mut page = sample::emoji();
    assert!(delete(&mut simulate(&page)).is_err());
    page.hovered = Some("2".into());
    let mut ui = simulate(&page);
    let at = delete(&mut ui).expect("the button is there").center();
    ui.point_at(at);
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Delete("2".into())]);
}

#[test]
fn the_pointer_crossing_rows_names_each_it_enters_and_leaves() {
    let page = sample::emoji();
    let mut ui = simulate(&page);
    let first = words(&mut ui, "Bob", 0).center();
    let second = words(&mut ui, "Bob", 1).center();
    for position in [second, first] {
        ui.point_at(position);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position })]);
    }
    let asked = asked(ui);
    assert!(asked.contains(&Intent::Hover("1".into(), true)));
    assert!(asked.contains(&Intent::Hover("2".into(), false)));
    assert_eq!(asked[0], Intent::Hover("2".into(), true));
}

#[test]
fn a_name_is_typed_in_place_and_enter_renames() {
    let page = sample::emoji();
    let mut ui = simulate(&page);
    ui.click("talk").expect("the name field");
    ui.typewrite("y");
    ui.tap_key(Key::Named(Named::Enter));
    let asked = asked(ui);
    assert!(asked.contains(&Intent::Edit("1".into(), "talky".into())));
    assert_eq!(asked.last(), Some(&Intent::Rename("1".into())));
}

#[test]
fn a_typed_name_shows_while_it_is_edited() {
    let mut page = sample::emoji();
    page.editing = Some(("2".into(), "parrot".into()));
    let mut ui = simulate(&page);
    assert!(ui.find("parrot").is_ok());
    assert!(ui.find("party").is_err());
}
