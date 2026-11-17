//! The emoji picker: task C17's tests, driving
//! `anomi_chat::pickers::emoji::view` over its fixtures in the simulator
//! (`super::support`).
//!
//! The popup lies at the window's top left, 408 by 432: the search over
//! its first 50 px, the rail down its first 48, the grid beside it, and the
//! foot over its last 48. The samples' grid starts with Favourites' heading
//! (28 px), her two favourites (56), Frequently Used's heading and six
//! emoji, Server's heading and two, then Filler 2's, locked.

use iced::advanced::widget::operation::focusable;
use iced::keyboard::key::Named;
use iced::{Event, Point, mouse};
use iced_test::{Simulator, simulator};

use anomi_chat::pickers::emoji::{FIELD, view};
use anomi_theme::fonts::twemoji;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::emoji_picker::{Emoji, EmojiPicker, Hovered, Intent, Purpose};
use anomi_vm::fixtures::chat::emoji_picker as sample;

use super::support::{self, SIZE};

/// Where the grid starts, and the middle of the emoji in column `column`
/// of the row that starts `top` px down it.
const GRID: Point = Point::new(48.0, 50.0);
fn emoji_at(column: f32, top: f32) -> Point {
    Point::new(GRID.x + 8.0 + 56.0 * column + 28.0, GRID.y + top + 28.0)
}

fn simulate(picker: &EmojiPicker) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(support::settings(), SIZE, view(picker, support::front()))
}

/// What a click at `at` asks for, but the hover it brings and the rows it
/// shows.
fn clicked(picker: &EmojiPicker, at: Point) -> Vec<Intent> {
    let mut ui = simulate(picker);
    ui.point_at(at);
    ui.simulate(simulator::click());
    acts(support::asked(ui))
}

fn acts(asked: Vec<Intent>) -> Vec<Intent> {
    asked
        .into_iter()
        .filter(|intent| !matches!(intent, Intent::Hover(_) | Intent::Shown(..)))
        .collect()
}

#[test]
fn typing_in_the_field_searches() {
    let picker = sample::reaction();
    let mut ui = simulate(&picker);
    ui.click("Find an emoji").unwrap();
    ui.typewrite("par");
    assert_eq!(
        acts(support::asked(ui)).last(),
        Some(&Intent::Search("par".into()))
    );
}

#[test]
fn enter_picks_the_first_match() {
    let picker = sample::searched("thumbs");
    let mut ui = simulate(&picker);
    ui.operate(&mut focusable::focus(FIELD.into()));
    ui.tap_key(Named::Enter);
    let first = picker.found[0].clone();
    assert_eq!(first.key(), "👍");
    assert_eq!(acts(support::asked(ui)), vec![Intent::Pick(first)]);
    // A locked one offers Nitro, as a click on it does.
    let locked = sample::searched("emoji_1");
    let mut ui = simulate(&locked);
    ui.operate(&mut focusable::focus(FIELD.into()));
    ui.tap_key(Named::Enter);
    assert_eq!(acts(support::asked(ui)), vec![Intent::Locked]);
}

#[test]
fn an_emoji_clicked_is_picked() {
    let picker = sample::reaction();
    let mut ui = simulate(&picker);
    ui.click(twemoji::text("🔥").as_ref()).unwrap();
    let fire = Emoji::unicode("🔥").unwrap();
    assert_eq!(acts(support::asked(ui)), vec![Intent::Pick(fire)]);
    // Server's :party:, the first of its row.
    let party = picker.sections[2].emoji[0].clone();
    assert_eq!(party.key(), "party:1400000000000000301");
    assert_eq!(
        clicked(&picker, emoji_at(0.0, 196.0)),
        vec![Intent::Pick(party)]
    );
}

#[test]
fn a_locked_emoji_offers_nitro() {
    let picker = sample::reaction();
    assert!(!picker.sections[3].emoji[0].usable());
    assert_eq!(clicked(&picker, emoji_at(0.0, 280.0)), vec![Intent::Locked]);
}

#[test]
fn a_star_keeps_an_emoji_among_the_favourites_or_lets_it_go() {
    let picker = sample::reaction();
    // :talk:'s star, filled: its tile's top right corner.
    let corner = |at: Point| Point::new(at.x + 28.0 - 9.0, at.y - 28.0 + 9.0);
    assert_eq!(
        clicked(&picker, corner(emoji_at(0.0, 28.0))),
        vec![Intent::Favorite("talk:1400000000000000300".into())]
    );
    // The fire's, which shows under the pointer.
    assert_eq!(
        clicked(&picker, corner(emoji_at(0.0, 112.0))),
        vec![Intent::Favorite("🔥".into())]
    );
}

#[test]
fn the_rail_jumps_to_a_section() {
    let picker = sample::reaction();
    let button = |index: f32| Point::new(24.0, GRID.y + 40.0 * index + 22.0);
    assert_eq!(clicked(&picker, button(2.0)), vec![Intent::Jump(2)]);
    assert_eq!(clicked(&picker, button(5.0)), vec![Intent::Jump(5)]);
}

#[test]
fn what_the_pointer_is_over_is_named() {
    let picker = sample::reaction();
    let mut ui = simulate(&picker);
    let mut point = |at: Point| {
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    };
    point(emoji_at(1.0, 28.0));
    point(Point::new(24.0, GRID.y + 22.0));
    let hovered: Vec<Intent> = support::asked(ui)
        .into_iter()
        .filter(|intent| matches!(intent, Intent::Hover(_)))
        .collect();
    assert_eq!(
        hovered,
        vec![
            Intent::Hover(Hovered::Emoji(Emoji::unicode("☠️").unwrap())),
            Intent::Hover(Hovered::Section(0)),
        ]
    );
}

#[test]
fn the_foot_says_what_the_picker_is_for_until_something_is_pointed_at() {
    let mut picker = sample::reaction();
    for (purpose, hint) in [
        (Purpose::Reaction, "Pick an emoji to react with."),
        (Purpose::Composer, "Pick an emoji for your message."),
        (Purpose::Edit, "Pick an emoji for your message."),
        (Purpose::Status, "Pick an emoji for your status."),
    ] {
        picker.purpose = purpose;
        assert!(simulate(&picker).find(hint).is_ok(), "{hint}");
    }
    assert!(simulate(&sample::hovered()).find("Thumbs up").is_ok());
    assert!(simulate(&sample::waiting()).find("Server").is_ok());
}

#[test]
fn a_search_without_matches_says_so() {
    let picker = sample::nothing();
    assert!(simulate(&picker).find("No emoji match \"zzzz\".").is_ok());
}

#[test]
fn the_grid_tells_which_rows_show() {
    let picker = sample::reaction();
    let mut ui = simulate(&picker);
    ui.draw(&Theme::default());
    let shown: Vec<Intent> = support::asked(ui)
        .into_iter()
        .filter(|intent| matches!(intent, Intent::Shown(..)))
        .collect();
    assert_eq!(shown, vec![Intent::Shown(0, 7)]);
}

#[test]
fn an_emoji_button_sends_its_press_or_offers_nitro() {
    use anomi_chat::pickers::emoji::emoji_button;
    use anomi_vm::kind::Expected;
    let press = |emoji: &Emoji| {
        let button = emoji_button(emoji, support::front())
            .size(44.0)
            .glyph(24.0)
            .fill(|theme: &Theme| theme.palette().field)
            .tip("Thumbs up")
            .on_press(Intent::Pick(emoji.clone()))
            .on_locked(Intent::Locked)
            .view();
        let mut ui = Simulator::with_size(support::settings(), SIZE, button);
        ui.point_at(Point::new(22.0, 22.0));
        ui.simulate(simulator::click());
        support::asked(ui)
    };
    let thumbs = Emoji::unicode("👍").unwrap();
    assert_eq!(press(&thumbs), vec![Intent::Pick(thumbs.clone())]);
    let locked = Emoji::custom("emoji_1", "1500000000000000601", Expected::Waiting, false);
    assert_eq!(press(&locked), vec![Intent::Locked]);
}
