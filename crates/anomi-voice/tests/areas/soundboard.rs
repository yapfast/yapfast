//! The soundboard: task V12's tests, driving `anomi_voice::soundboard::view`
//! over its fixtures in the simulator (`super::support`).
//!
//! A chip has no text to click by, so a click goes by position: the card
//! sits at the window's top left, 372 wide, its title and heading above the
//! first line of sounds, whose chips are 112 wide, 6 apart, 12 from the edge.

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;
use iced_test::simulator::click;

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::soundboard as samples;
use anomi_vm::voice::soundboard::{Intent, Soundboard};
use anomi_voice::soundboard;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(400.0, 600.0);
/// The first line of sounds: a title of 39, then a heading's 42, and
/// the chip's middle 3 + 18 into its row.
const FIRST: f32 = 39.0 + 42.0 + 21.0;
const CHIP: f32 = 112.0;
const LEFT: f32 = 12.0;

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn simulate(vm: &Soundboard) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, soundboard::view(vm, front()))
}

fn move_to(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

/// Where the chip `n` of the first line's `part` is: its middle, its hear
/// button (at its start) or its star (at its end).
fn chip(n: usize, part: Part) -> Point {
    let left = LEFT + n as f32 * (CHIP + 6.0);
    let x = match part {
        Part::Middle => left + CHIP / 2.0,
        Part::Hear => left + 4.0 + 13.0,
        Part::Star => left + CHIP - 4.0 - 13.0,
    };
    Point::new(x, FIRST)
}

#[derive(Clone, Copy)]
enum Part {
    Middle,
    Hear,
    Star,
}

fn press(ui: &mut Ui<'_>, at: Point) {
    move_to(ui, at);
    ui.simulate(click());
}

#[test]
fn a_click_plays_the_sound() {
    let vm = samples::full();
    let mut ui = simulate(&vm);
    press(&mut ui, chip(0, Part::Middle));
    press(&mut ui, chip(1, Part::Middle));
    assert_eq!(
        asked(ui),
        [Intent::Play("1".into()), Intent::Play("2".into())]
    );
}

#[test]
fn hovering_offers_hearing_it_first() {
    let vm = samples::full();
    let mut ui = simulate(&vm);
    // The pointer first on the chip, which shows the buttons, then on one.
    move_to(&mut ui, chip(0, Part::Middle));
    press(&mut ui, chip(0, Part::Hear));
    assert_eq!(asked(ui), [Intent::Preview("1".into())]);
}

#[test]
fn a_favourites_star_takes_it_away() {
    let vm = samples::full();
    let mut ui = simulate(&vm);
    press(&mut ui, chip(0, Part::Star));
    assert_eq!(asked(ui), [Intent::Favorite("1".into(), false)]);
}

#[test]
fn another_sound_is_starred_after_hovering_it() {
    let vm = samples::waiting();
    let mut ui = simulate(&vm);
    // "no emoji" is the second chip of the first line, not a favourite.
    move_to(&mut ui, chip(1, Part::Middle));
    press(&mut ui, chip(1, Part::Star));
    assert_eq!(asked(ui), [Intent::Favorite("9".into(), true)]);
}

#[test]
fn while_it_cannot_play_a_click_does_nothing_but_a_favourite_changes() {
    let vm = samples::deafened();
    assert!(!vm.playable());
    let mut ui = simulate(&vm);
    // The note takes a line more above the sounds.
    let at = |point: Point| Point::new(point.x, point.y + 21.0);
    press(&mut ui, at(chip(0, Part::Middle)));
    press(&mut ui, at(chip(0, Part::Star)));
    assert_eq!(asked(ui), [Intent::Favorite("1".into(), false)]);
}

#[test]
fn a_chooser_offers_neither_hearing_nor_stars() {
    let vm = samples::chooser();
    let mut ui = simulate(&vm);
    // Where hearing is on a call's soundboard, a click chooses the sound.
    press(&mut ui, chip(0, Part::Hear));
    press(&mut ui, chip(0, Part::Star));
    assert_eq!(
        asked(ui),
        [Intent::Play("1".into()), Intent::Play("1".into())]
    );
}

#[test]
fn before_its_sounds_come_it_says_so() {
    let vm = samples::loading();
    assert!(vm.loading());
    let mut ui = simulate(&vm);
    assert!(ui.find("Loading sounds...").is_ok());
    assert!(ui.find("Soundboard").is_ok());
    assert!(asked(ui).is_empty());
}

#[test]
fn it_keeps_the_title_and_note_it_is_given() {
    let vm = samples::chooser();
    let mut ui = simulate(&vm);
    assert!(ui.find("Choose an entrance sound").is_ok());
    let vm = samples::deafened();
    let mut ui = simulate(&vm);
    assert!(ui.find("Soundboard disabled when deafened").is_ok());
}

#[test]
fn sections_come_three_to_a_row_under_capitalised_headings() {
    let board = samples::full();
    assert_eq!(board.rows.len(), 7);
    assert!(
        matches!(&board.rows[0], anomi_vm::voice::soundboard::Row::Heading(h) if *h == h.to_uppercase())
    );
}
