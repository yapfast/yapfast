//! The settings frame: task S01's tests, driving
//! `anomi_settings::frame::window` over its fixtures in the simulator
//! (`super::support`). The window is the golden one: the card is 1288 by 748
//! with 56 px of room each side (the simulator hands the view the whole
//! window, where the layer would take 8 px), the sections 266 wide from
//! x 56, the bar 48 tall from y 56 with its close button's centre at
//! (1320, 80).

use iced::Point;
use iced_test::Simulator;

use anomi_settings::frame::window;
use anomi_vm::fixtures::settings::frame::{frame, unsaved};
use anomi_vm::settings::frame::{Build, Intent as Frame, Page, Tone, Unsaved};
use anomi_vm::settings::{Intent, Settings};

use super::support::{SIZE, asked, front, settings};

fn with(frame: anomi_vm::settings::frame::Frame) -> Settings {
    Settings {
        frame,
        ..Settings::default()
    }
}

fn ask(frame: Frame) -> Vec<Intent> {
    vec![Intent::Frame(frame)]
}

fn click(vm: &Settings, text: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, window(vm, front()));
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

/// The unsaved bar's buttons: 12 px in from the card's right edge, each the
/// width of its words and 32 more, 8 px apart, the bar's middle at y 774.
const SAVE: Point = Point::new(1261.0, 774.0);
const RESET: Point = Point::new(1141.0, 774.0);

fn click_at(vm: &Settings, at: Point) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, window(vm, front()));
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn choosing_a_section_asks_to_open_its_page() {
    let vm = with(frame(Page::Account));
    assert_eq!(click(&vm, "Voice & Video"), ask(Frame::Choose(Page::Voice)));
    assert_eq!(click(&vm, "About"), ask(Frame::Choose(Page::About)));
    assert_eq!(click(&vm, "Discord"), ask(Frame::Choose(Page::Discord)));
}

#[test]
fn a_build_without_discord_leaves_out_its_pages() {
    let vm = with(anomi_vm::settings::frame::Frame {
        with_discord: false,
        ..frame(Page::Account)
    });
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Game Activity").is_err());
    assert!(ui.find("Discord").is_err());
    assert!(ui.find("Keybinds").is_ok());
}

#[test]
fn the_search_asks_and_narrows_the_sections() {
    let vm = with(frame(Page::Account));
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    ui.click("Search settings").expect("the search");
    ui.typewrite("so");
    assert_eq!(
        asked(ui).last(),
        Some(&Intent::Frame(Frame::Search("so".into())))
    );
    let found = with(anomi_vm::settings::frame::Frame {
        query: "so".into(),
        ..frame(Page::Account)
    });
    let mut ui = Simulator::with_size(settings(), SIZE, window(&found, front()));
    assert!(ui.find("Sounds").is_ok());
    assert!(ui.find("Voice & Video").is_err());
    // The headings go while something is searched.
    assert!(ui.find("Experience").is_err());
}

#[test]
fn edit_profile_asks_only_where_the_profile_can_be_edited() {
    let vm = with(frame(Page::Account));
    assert_eq!(click(&vm, "Edit Profile"), ask(Frame::EditProfile));
    let local = with(anomi_vm::settings::frame::Frame {
        may_edit_profile: false,
        ..frame(Page::Account)
    });
    let mut ui = Simulator::with_size(settings(), SIZE, window(&local, front()));
    assert!(ui.find("Edit Profile").is_err());
}

#[test]
fn the_close_button_and_the_gutter_ask_to_close() {
    let vm = with(frame(Page::Account));
    assert_eq!(click_at(&vm, Point::new(1320.0, 80.0)), ask(Frame::Close));
    assert_eq!(click_at(&vm, Point::new(20.0, 400.0)), ask(Frame::Close));
    // A press on the card's empty page asks nothing.
    assert!(click_at(&vm, Point::new(700.0, 500.0)).is_empty());
}

#[test]
fn the_build_copies_and_the_update_opens_about() {
    let vm = with(anomi_vm::settings::frame::Frame {
        update: Some("1.2.3".into()),
        ..frame(Page::Account)
    });
    let summary = vm.frame.build.as_ref().expect("a build").summary.clone();
    assert_eq!(click(&vm, "Anomi acd138"), ask(Frame::Copy(summary)));
    assert_eq!(
        click(&vm, "Update available · 1.2.3"),
        ask(Frame::Choose(Page::About))
    );
}

#[test]
fn a_copied_build_says_so_instead_of_its_channel() {
    let build = Build {
        tone: Tone::Release,
        ..frame(Page::Account).build.expect("a build")
    };
    let vm = with(anomi_vm::settings::frame::Frame {
        copied: true,
        build: Some(build),
        ..frame(Page::Account)
    });
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Copied").is_ok());
    assert!(ui.find("Development build").is_err());
}

#[test]
fn the_unsaved_bar_resets_and_saves() {
    let vm = with(unsaved());
    assert_eq!(click_at(&vm, RESET), ask(Frame::Undo));
    assert_eq!(click_at(&vm, SAVE), ask(Frame::Save));
}

#[test]
fn a_saving_bar_takes_no_press() {
    let vm = with(anomi_vm::settings::frame::Frame {
        unsaved: Some(Unsaved {
            saving: true,
            ..Unsaved::default()
        }),
        ..frame(Page::Account)
    });
    assert!(click_at(&vm, SAVE).is_empty());
    assert!(click_at(&vm, RESET).is_empty());
}

#[test]
fn no_bar_shows_without_unsaved_changes() {
    let vm = with(frame(Page::Account));
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, front()));
    assert!(ui.find("Save Changes").is_err());
}
