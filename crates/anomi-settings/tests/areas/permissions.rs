//! A channel's permissions: task X14's tests, driving
//! `anomi_settings::channel::permissions::view` over its fixtures in the
//! simulator (`super::support`). The page alone fills the window (1400 by
//! 860): its lines end 12 px short of the right edge, each with its three
//! buttons (108 px, a third each) on the title's line.

use iced::Point;
use iced_test::Simulator;

use anomi_settings::channel::permissions::{add, confirm, view};
use anomi_vm::fixtures::settings::permissions as sample;
use anomi_vm::settings::permissions::{Confirm, Intent, Permissions};

use super::support::{SIZE, asked, front, settings};

/// A third of the three buttons' box: where each one's centre is from its
/// left edge.
fn button(line: Point, state: usize) -> Point {
    Point::new(
        SIZE.width - 12.0 - 108.0 + 1.0 + 17.67 + 35.33 * state as f32,
        line.y,
    )
}

fn page(
    permissions: &Permissions,
    act: impl FnOnce(&mut Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>),
) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(permissions, front()));
    act(&mut ui);
    asked(ui)
}

fn click_text(permissions: &Permissions, text: &str) -> Vec<Intent> {
    page(permissions, |ui| {
        ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    })
}

fn click_button(permissions: &Permissions, title: &str, state: usize) -> Vec<Intent> {
    page(permissions, |ui| {
        let at = ui.find(title).expect("the permission").bounds().center();
        ui.point_at(button(at, state));
        ui.simulate(iced_test::simulator::click());
    })
}

fn shows(permissions: &Permissions, text: &str) -> bool {
    let mut ui = Simulator::with_size(settings(), SIZE, view(permissions, front()));
    ui.find(text).is_ok()
}

#[test]
fn each_button_sets_its_state() {
    let permissions = sample::permissions();
    assert_eq!(
        click_button(&permissions, "View Channel", 0),
        [Intent::Set(10, 0)]
    );
    assert_eq!(
        click_button(&permissions, "View Channel", 1),
        [Intent::Set(10, 1)]
    );
    assert_eq!(
        click_button(&permissions, "View Channel", 2),
        [Intent::Set(10, 2)]
    );
}

#[test]
fn a_locked_permission_or_a_page_that_may_not_edit_asks_nothing() {
    let permissions = sample::permissions();
    assert!(click_button(&permissions, "Manage Permissions", 2).is_empty());
    let permissions = sample::locked();
    assert!(click_button(&permissions, "View Channel", 2).is_empty());
    let permissions = sample::saving();
    assert!(click_button(&permissions, "View Channel", 2).is_empty());
}

#[test]
fn the_private_switch_asks_for_the_flip() {
    let permissions = sample::permissions();
    assert_eq!(
        click_text(&permissions, "Private Channel"),
        [Intent::Private(true)]
    );
    assert_eq!(
        click_text(&sample::apart(), "Private Channel"),
        [Intent::Private(false)]
    );
}

#[test]
fn choosing_an_overwrite_shows_it() {
    let permissions = sample::apart();
    assert_eq!(
        click_text(&permissions, "Moderators"),
        [Intent::Show("2".into())]
    );
    assert_eq!(click_text(&permissions, "Bob"), [Intent::Show("3".into())]);
}

#[test]
fn add_and_remove_ask_for_their_dialogs() {
    let permissions = sample::apart();
    let asked = page(&permissions, |ui| {
        let heading = ui.find("ROLES/MEMBERS").expect("its heading").bounds();
        ui.point_at(Point::new(heading.x + 220.0 - 14.0, heading.center_y()));
        ui.simulate(iced_test::simulator::click());
    });
    assert_eq!(asked, [Intent::Add]);
    assert_eq!(
        click_text(&permissions, "Remove Moderators"),
        [Intent::Remove]
    );
}

#[test]
fn everyone_cannot_be_removed() {
    assert!(!shows(&sample::permissions(), "Remove @everyone"));
    assert!(shows(&sample::permissions(), "@everyone"));
}

#[test]
fn the_notice_offers_sync_now_only_when_apart() {
    assert!(shows(
        &sample::permissions(),
        "Permissions synced with category: Channels"
    ));
    assert!(!shows(&sample::permissions(), "Sync Now"));
    let apart = sample::apart();
    assert!(shows(
        &apart,
        "Permissions not synced with category: Channels"
    ));
    assert_eq!(click_text(&apart, "Sync Now"), [Intent::SyncNow]);
    let mut none = sample::permissions();
    none.sync = anomi_vm::settings::permissions::Sync::None;
    assert!(!shows(&none, "Permissions synced with category: Channels"));
}

fn dialog(permissions: &Permissions, text: &str) -> Vec<Intent> {
    let element = confirm(permissions, front()).expect("a question");
    let mut ui = Simulator::with_size(settings(), SIZE, element);
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

#[test]
fn the_questions_answer_with_their_buttons() {
    let remove = sample::confirming(Confirm::Remove);
    assert_eq!(dialog(&remove, "Delete"), [Intent::Confirm]);
    assert_eq!(dialog(&remove, "Cancel"), [Intent::Cancel]);
    let sync = sample::confirming(Confirm::Sync);
    assert_eq!(dialog(&sync, "Sync Permissions"), [Intent::Confirm]);
    assert_eq!(dialog(&sync, "Keep Current Permissions"), [Intent::Cancel]);
    let refused = sample::confirming(Confirm::Refused);
    assert_eq!(dialog(&refused, "Okay"), [Intent::Cancel]);
}

#[test]
fn a_refusal_has_no_second_button_and_no_question_no_dialog() {
    let refused = sample::confirming(Confirm::Refused);
    let element = confirm(&refused, front()).expect("a notice");
    let mut ui = Simulator::with_size(settings(), SIZE, element);
    assert!(ui.find("Delete").is_err() && ui.find("Sync Permissions").is_err());
    assert!(confirm(&sample::permissions(), front()).is_none());
}

fn picker(
    act: impl FnOnce(&mut Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>),
) -> Vec<Intent> {
    let adding = sample::adding();
    let mut ui = Simulator::with_size(settings(), SIZE, add(&adding, front()));
    act(&mut ui);
    asked(ui)
}

#[test]
fn the_picker_adds_a_role_or_a_member() {
    assert_eq!(
        picker(|ui| {
            ui.click("Moderators").unwrap();
        }),
        [Intent::AddRole("10".into())]
    );
    assert_eq!(
        picker(|ui| {
            ui.click("Bob").unwrap();
        }),
        [Intent::AddMember("202".into())]
    );
    assert_eq!(
        picker(|ui| {
            ui.click("Cancel").unwrap();
        }),
        [Intent::CloseAdd]
    );
}

#[test]
fn typing_in_the_picker_asks_to_search() {
    let asked = picker(|ui| {
        ui.click("Role/Member").unwrap();
        ui.typewrite("a");
    });
    assert!(asked.contains(&Intent::Search("a".into())), "{asked:?}");
}
