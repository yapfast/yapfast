//! A server's settings frame and its profile page: task X01's tests, driving
//! `anomi_settings::server::window` over its fixtures in the simulator
//! (`super::support`).

use iced::Point;
use iced_test::Simulator;

use anomi_settings::server::window;
use anomi_vm::fixtures::settings::server::{edited, owner, server};
use anomi_vm::settings::server::{Intent as Server, Section};
use anomi_vm::settings::{Intent, Settings};

use super::support::{SIZE, asked, front, settings};

fn with(server: anomi_vm::settings::server::Server) -> Settings {
    Settings {
        server,
        ..Settings::default()
    }
}

fn ask(intent: Server) -> Vec<Intent> {
    vec![Intent::Server(intent)]
}

fn click(vm: &Settings, text: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, window(vm, front()));
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

#[test]
fn a_section_is_chosen_from_the_sections() {
    let vm = with(server());
    assert_eq!(
        click(&vm, "Engagement"),
        ask(Server::Choose(Section::Engagement))
    );
    assert_eq!(click(&vm, "Bans"), ask(Server::Choose(Section::Bans)));
    assert_eq!(click(&vm, "Emoji"), ask(Server::Choose(Section::Emoji)));
}

#[test]
fn delete_server_is_there_for_its_owner_only() {
    let vm = with(owner());
    assert_eq!(click(&vm, "Delete Server"), ask(Server::Delete));
    let vm = with(server());
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Delete Server").is_err());
}

#[test]
fn a_section_the_account_may_not_open_is_not_listed() {
    let mut roles_only = server();
    roles_only.sections = vec![Section::Roles];
    roles_only.section = Section::Roles;
    let vm = with(roles_only);
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Roles").is_ok());
    assert!(ui.find("Server Profile").is_err());
}

#[test]
fn the_icon_is_changed_or_removed() {
    let vm = with(server());
    assert_eq!(click(&vm, "Change Server Icon"), ask(Server::PickIcon));
    assert_eq!(click(&vm, "Remove Icon"), ask(Server::RemoveIcon));
    let mut none = server();
    none.profile.has_icon = false;
    let vm = with(none);
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Remove Icon").is_err());
}

#[test]
fn nothing_is_asked_while_discord_takes_a_change() {
    let mut busy = server();
    busy.saving = true;
    assert!(click(&with(busy), "Change Server Icon").is_empty());
}

#[test]
fn a_picture_being_read_says_so() {
    let mut reading = server();
    reading.profile.reading = true;
    let vm = with(reading);
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Reading the picture...").is_ok());
    assert!(
        ui.find("We recommend an image of at least 512x512.")
            .is_err()
    );
}

#[test]
fn the_name_is_typed() {
    let vm = with(server());
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    ui.click("Server").expect("the name field");
    ui.typewrite("!");
    assert_eq!(asked(ui), ask(Server::Name("Server!".into())));
}

#[test]
fn the_description_is_typed() {
    let vm = with(server());
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    // The area's placeholder is no text to find: a press under its heading.
    let heading = ui.find("DESCRIPTION").expect("its heading").bounds();
    ui.point_at(Point::new(
        heading.x + 40.0,
        heading.y + heading.height + 30.0,
    ));
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("!");
    assert_eq!(asked(ui), ask(Server::Description("!".into())));
}

#[test]
fn the_shell_draws_the_profile_before_it_has_the_window() {
    let vm = server();
    let mut ui = Simulator::with_size(settings(), SIZE, anomi_settings::server::view(&vm, front()));
    ui.click("Change Server Icon").expect("the profile page");
    assert_eq!(asked(ui), ask(Server::PickIcon));
}

#[test]
fn the_bar_for_unsaved_changes_resets_and_saves() {
    let vm = with(edited());
    assert_eq!(click(&vm, "Save Changes"), ask(Server::Save));
    assert_eq!(click(&vm, "Reset"), ask(Server::Undo));
}

#[test]
fn the_close_button_closes() {
    let vm = with(server());
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    // The close button's centre (`frame.rs`'s): a press there, not on the
    // centre of whatever holds the point.
    ui.point_at(Point::new(1320.0, 80.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), ask(Server::Close));
}
