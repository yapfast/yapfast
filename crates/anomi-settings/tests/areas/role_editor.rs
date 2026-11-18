//! A server's role editor: task X09's tests, driving
//! `anomi_settings::server::roles::editor::view` over its fixtures in the
//! simulator (`super::support`).

use iced::keyboard::{Key, key::Named};
use iced::{Point, Widget as _};
use iced_test::simulator::Simulator;

use anomi_settings::server::roles::editor::{adding, confirm, view};
use anomi_vm::fixtures::settings::role_editor as sample;
use anomi_vm::settings::role_editor::{Confirm, Intent, RoleEditor, Tab, hex_color};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    editor: &RoleEditor,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(editor, front()).boxed())
}

#[test]
fn a_locked_role_says_why_and_its_switches_ask_nothing() {
    let editor = sample::locked();
    let mut ui = simulate(&editor);
    assert!(
        ui.find("Role is locked because it is a higher rank than your highest role.")
            .is_ok()
    );
    ui.click("Display role members separately from online members")
        .expect("the switch is there");
    ui.click("Choose Image").expect("the button is there");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_tabs_and_back_ask_for_their_places() {
    let editor = sample::editable();
    let mut ui = simulate(&editor);
    ui.click("Permissions").expect("the tab is there");
    ui.click("Manage Members (3)").expect("the tab is there");
    ui.click("Back").expect("the link is there");
    assert_eq!(
        asked(ui),
        [
            Intent::Tab(Tab::Permissions),
            Intent::Tab(Tab::Members),
            Intent::Back
        ]
    );
}

#[test]
fn everyone_has_only_its_permissions() {
    let mut editor = sample::editable();
    editor.everyone = true;
    let mut ui = simulate(&editor);
    ui.click("Display").expect("the tab is there");
    assert!(asked(ui).is_empty());
}

#[test]
fn an_editable_role_toggles_and_chooses_an_image() {
    let editor = sample::editable();
    let mut ui = simulate(&editor);
    ui.click("Display role members separately from online members")
        .expect("the switch is there");
    ui.click("Allow anyone to @mention this role")
        .expect("the switch is there");
    ui.click("Choose Image").expect("the button is there");
    ui.click("Remove Icon").expect("the button is there");
    assert_eq!(
        asked(ui),
        [
            Intent::Hoist,
            Intent::Mentionable,
            Intent::PickIcon,
            Intent::RemoveIcon
        ]
    );
}

#[test]
fn a_role_whose_icon_is_locked_says_when_it_opens() {
    let editor = sample::custom();
    let mut ui = simulate(&editor);
    assert!(ui.find("Unlocks at Level 2").is_ok());
    ui.click("Choose Image").expect("the button is there");
    assert!(asked(ui).is_empty());
}

#[test]
fn permissions_toggle_by_bit_and_clear() {
    let editor = sample::window(sample::editable(), Tab::Permissions)
        .settings
        .role_editor;
    let mut ui = simulate(&editor);
    ui.click("View Channels").expect("the switch is there");
    ui.click("Clear permissions").expect("the button is there");
    // A permission Alice lacks asks nothing.
    ui.click("Manage Roles").expect("the switch is there");
    assert_eq!(
        asked(ui),
        [Intent::Permission(10), Intent::ClearPermissions]
    );
}

#[test]
fn members_are_listed_and_removed_through_the_dialog() {
    let editor = sample::window(sample::editable(), Tab::Members)
        .settings
        .role_editor;
    let mut ui = simulate(&editor);
    assert!(ui.find("Bob").is_ok());
    ui.click("Add Members").expect("the button is there");
    assert_eq!(asked(ui), [Intent::AddMembers]);
    let mut empty = editor.clone();
    empty.members.clear();
    assert!(simulate(&empty).find("No members were found.").is_ok());
    empty.members_loading = true;
    assert!(simulate(&empty).find("Loading...").is_ok());
}

#[test]
fn a_managed_role_cannot_have_members_added() {
    let mut editor = sample::window(sample::editable(), Tab::Members)
        .settings
        .role_editor;
    editor.may_add = false;
    let mut ui = simulate(&editor);
    ui.click("Add Members").expect("the button is there");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_confirm_dialog_asks_in_red_and_cancels() {
    let asked_for = Confirm::Delete {
        role: "Mods".into(),
    };
    let mut ui = Simulator::with_size(settings(), SIZE, confirm(&asked_for, front()));
    assert!(
        ui.find("Are you sure you want to delete the Mods role? This action cannot be undone.")
            .is_ok()
    );
    ui.click("Cancel").expect("the button is there");
    ui.click("Delete").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Cancel, Intent::Confirm]);
    let remove = Confirm::Remove {
        role: "Mods".into(),
        member: "Bob".into(),
    };
    let mut ui = Simulator::with_size(settings(), SIZE, confirm(&remove, front()));
    assert!(ui.find("Remove Bob from role Mods?").is_ok());
}

#[test]
fn add_members_picks_and_adds_up_to_its_limit() {
    let mut open = sample::adding();
    let mut ui = Simulator::with_size(settings(), SIZE, adding(&open, front()));
    ui.click("Carol").expect("the row is there");
    ui.click("Add").expect("the button is there");
    ui.click("Cancel").expect("the button is there");
    assert_eq!(
        asked(ui),
        [
            Intent::AddingPick("1400000000000000203".into()),
            Intent::AddingAdd,
            Intent::AddingClose
        ]
    );
    // Nobody picked: Add waits.
    open.candidates
        .iter_mut()
        .for_each(|member| member.chosen = false);
    assert!(!open.may_add());
    let mut ui = Simulator::with_size(settings(), SIZE, adding(&open, front()));
    ui.click("Add").expect("the button is there");
    assert!(asked(ui).is_empty());
}

#[test]
fn hex_is_read_with_or_without_its_hash() {
    assert_eq!(hex_color("#99aab5"), Some(0x99aab5));
    assert_eq!(hex_color(" 12AB34 "), Some(0x12ab34));
    assert_eq!(hex_color("#12ab3"), None);
    assert_eq!(hex_color("#12ab3g"), None);
}

#[test]
fn colours_are_picked_by_place_and_by_hex() {
    let editor = sample::editable();
    let mut ui = simulate(&editor);
    let label = ui
        .find("Members use the color of the highest role they have on the roles list.")
        .expect("the label is there")
        .bounds();
    let top = label.y + label.height + 8.0;
    // The grey, then the first bright swatch and the first dark one.
    for at in [
        Point::new(label.x + 33.0, top + 25.0),
        Point::new(label.x + 86.0, top + 10.0),
        Point::new(label.x + 86.0, top + 40.0),
    ] {
        ui.point_at(at);
        let _ = ui.simulate(iced_test::simulator::click());
    }
    ui.click("#3498db").expect("the hex field is there");
    let _ = ui.tap_key(Key::Named(Named::End));
    let _ = ui.typewrite("0");
    let _ = ui.tap_key(Key::Named(Named::Enter));
    assert_eq!(
        asked(ui),
        [
            Intent::Color(None),
            Intent::Color(Some(0)),
            Intent::Color(Some(10)),
            Intent::HexText("#3498db0".into()),
            Intent::Hex("#3498db".into()),
        ]
    );
}

#[test]
fn a_member_is_taken_off_and_the_searches_ask() {
    let editor = sample::window(sample::editable(), Tab::Members)
        .settings
        .role_editor;
    let mut ui = simulate(&editor);
    // The cross at the end of Alice's line, under Add Members' end.
    let add = ui
        .find("Add Members")
        .expect("the button is there")
        .bounds();
    let alice = ui.find("Alice").expect("her line is there").bounds();
    ui.point_at(Point::new(add.x + add.width - 20.0, alice.center_y()));
    let _ = ui.simulate(iced_test::simulator::click());
    ui.click("Search Members").expect("the search is there");
    let _ = ui.typewrite("B");
    assert_eq!(
        asked(ui),
        [
            Intent::RemoveMember("1400000000000000201".into()),
            Intent::MemberSearch("B".into()),
        ]
    );
}
