//! The composer's slash commands: task C16's tests, driving
//! `anomi_chat::composer::commands::view` over its fixtures in the simulator
//! (`super::support`).
//!
//! The list floats over the composer, so it sits in a window 800 wide and
//! 700 tall with its slot 100 px above the bottom, where the composer's
//! frame would start; the list's bottom edge ends 8 px over it, its rows
//! start under 6 px of padding and a 24 px heading, and each is 44 tall.

use iced::widget::{column, container, space};
use iced::{Fill, Point, Size};
use iced_test::{Simulator, simulator};

use anomi_chat::composer::commands::{command, pills, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::commands::{Commands, Intent};
use anomi_vm::chat::{self, Intent as Chat};
use anomi_vm::fixtures::chat::commands as sample;
use anomi_widgets::composer::Command;

use super::support::{asked, front, settings};

const WINDOW: Size = Size::new(800.0, 700.0);

/// The list in its column: its slot, then the composer's room.
fn above(commands: &Commands) -> iced::Element<'_, Chat, Theme, Renderer> {
    use iced::Widget as _;
    container(
        column![
            space().height(Fill),
            view(commands, front()).map(Chat::from).boxed(),
            space().height(100)
        ]
        .width(Fill),
    )
    .boxed()
}

fn ui(commands: &Commands) -> Simulator<'_, Chat, Theme, Renderer> {
    Simulator::with_size(settings(), WINDOW, above(commands))
}

/// The pills alone, at the window's top.
fn written(commands: &Commands) -> Simulator<'_, Intent, Theme, Renderer> {
    use iced::Widget as _;
    Simulator::with_size(
        settings(),
        Size::new(400.0, 200.0),
        container(pills(commands, front()).boxed()).width(Fill),
    )
}

/// A click on row `index` of a list of `rows` rows: the frame ends 8 px over
/// the slot (at 600) and is 12 px of padding, a 24 px heading and the rows
/// showing, 44 each.
fn click(commands: &Commands, index: usize) -> Vec<Chat> {
    let rows = commands.choices.len().min(8) as f32;
    let top = 600.0 - 8.0 - (12.0 + 24.0 + rows * 44.0);
    let mut ui = ui(commands);
    ui.point_at(Point::new(
        400.0,
        top + 6.0 + 24.0 + 44.0 * index as f32 + 22.0,
    ));
    ui.simulate(simulator::click());
    asked(ui)
}

fn said(intent: Intent) -> Vec<Chat> {
    vec![Chat::Commands(intent)]
}

#[test]
fn a_click_on_a_row_picks_it() {
    let commands = sample::listing();
    assert_eq!(click(&commands, 1), said(Intent::Pick(1)));
    assert_eq!(click(&commands, 6), said(Intent::Pick(6)));
    assert_eq!(click(&commands, 7), said(Intent::Pick(7)));
}

#[test]
fn a_row_shows_its_name_description_and_app() {
    let commands = sample::listing();
    let mut ui = ui(&commands);
    for text in [
        "COMMANDS",
        "/ping",
        "Checks that Status Bot is up.",
        "Status Bot",
        "Notes",
        "Keeps a note.",
    ] {
        assert!(ui.find(text).is_ok(), "{text} shows");
    }
}

#[test]
fn a_value_has_no_slash() {
    let commands = sample::values();
    let mut ui = ui(&commands);
    assert!(ui.find("COLOUR").is_ok());
    assert!(ui.find("green").is_ok());
    assert!(ui.find("/green").is_err());
    assert_eq!(click(&commands, 2), said(Intent::Pick(2)));
}

#[test]
fn a_list_that_is_not_shown_draws_nothing() {
    let commands = Commands {
        shown: false,
        ..sample::listing()
    };
    let mut ui = ui(&commands);
    assert!(ui.find("/ping").is_err());
    assert!(ui.find("COMMANDS").is_err());
}

#[test]
fn the_heading_says_why_there_are_no_rows() {
    let loading = sample::loading();
    assert!(ui(&loading).find("Loading commands…").is_ok());
    let none = sample::none();
    assert!(ui(&none).find("No commands match").is_ok());
}

#[test]
fn only_eight_rows_show_and_the_chosen_one_is_among_them() {
    let commands = sample::long();
    let mut long = ui(&commands);
    assert!(long.find("/note 2").is_ok());
    // Eight rows: the first four are above the ones shown.
    assert!(long.find("/ping").is_err());
    let few = sample::listing();
    let mut all = ui(&few);
    assert!(all.find("/ping").is_ok() && all.find("/note").is_ok());
}

#[test]
fn the_pills_show_the_name_and_each_option() {
    let commands = sample::written();
    let mut ui = written(&commands);
    for text in [
        "/config set",
        "key:",
        "theme",
        "note:",
        "limit:",
        "many",
        "Must be a number",
        "log:",
    ] {
        assert!(ui.find(text).is_ok(), "{text} shows");
    }
}

#[test]
fn a_click_on_a_pill_takes_the_text_to_it_or_picks_its_file() {
    let commands = sample::written();
    let mut ui = written(&commands);
    ui.click("note:").expect("the optional pill");
    assert_eq!(asked(ui), vec![Intent::Option(1)]);
    let mut ui = written(&commands);
    ui.click("log:").expect("the file's pill");
    assert_eq!(asked(ui), vec![Intent::File(3)]);
}

#[test]
fn a_command_with_no_options_is_only_its_name() {
    let commands = sample::bare();
    let mut ui = written(&commands);
    assert!(ui.find("/ping").is_ok());
}

#[test]
fn no_command_written_no_pills() {
    let commands = sample::listing();
    let mut ui = written(&commands);
    assert!(ui.find("/ping").is_err());
    assert_eq!(command(&commands), None);
}

#[test]
fn the_keys_know_the_command() {
    // The focused option is the second, an optional one with no file.
    assert_eq!(
        command(&sample::written()),
        Some(Command {
            options: 4,
            focus: 1,
            wants_file: false,
        })
    );
    // A file option with none chosen: Enter picks one.
    let mut file = sample::written();
    let written = file.written.as_mut().unwrap();
    written.options[3].value.clear();
    written.focused = 3;
    assert_eq!(
        command(&file),
        Some(Command {
            options: 4,
            focus: 3,
            wants_file: true,
        })
    );
    assert_eq!(
        command(&sample::bare()),
        Some(Command {
            options: 0,
            focus: 0,
            wants_file: false,
        })
    );
}

#[test]
fn the_frame_wires_the_list_above_the_composer() {
    // F01's wiring: the composer's view holds the list's, which draws only
    // while shown.
    let _ = chat::Intent::Commands(Intent::Pick(0));
}
