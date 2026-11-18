//! A message's reactions, and the Reactions window: task C03's tests,
//! driving `anomi_chat::reactions::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::Widget as _;
use iced_test::Simulator;

use anomi_chat::reactions;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::reactions::{Dialog, Intent, Reactions};
use anomi_vm::fixtures::chat::reactions as sample;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(760.0, 420.0);

fn chips(reactions: &Reactions) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(
        settings(),
        SIZE,
        reactions::view(reactions, front()).boxed(),
    )
}

fn window(dialog: &Dialog) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(
        settings(),
        Size::new(900.0, 800.0),
        reactions::dialog(dialog, front()).boxed(),
    )
}

#[test]
fn a_chip_toggles_this_accounts_reaction() {
    let shown = sample::chips();
    let mut ui = chips(&shown);
    let at = ui
        .find("128")
        .expect("the heart's count")
        .visible_bounds()
        .expect("shown")
        .center();
    ui.point_at(at);
    // The pointer arriving on the chip asks for who reacted.
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
    let hovered: Vec<_> = ui.drain().collect();
    assert_eq!(
        hovered,
        vec![Intent::Hover(sample::ITEM, "\u{2764}\u{FE0F}".into())]
    );
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        vec![Intent::Toggle(sample::ITEM, "\u{2764}\u{FE0F}".into())]
    );
}

#[test]
fn moving_between_chips_names_the_one_left() {
    let shown = sample::chips();
    let mut ui = chips(&shown);
    let mut centre = |text: &str| {
        ui.find(text)
            .expect("a count")
            .visible_bounds()
            .expect("shown")
            .center()
    };
    let (heart, joy) = (centre("128"), centre("2"));
    for position in [heart, joy] {
        ui.point_at(position);
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position,
        })]);
    }
    // The card of who reacted holds 200 ms after the pointer left its chip
    // (`Tip::holding`): a frame past that, the heart's goes.
    let later = std::time::Instant::now() + std::time::Duration::from_millis(300);
    ui.simulate([iced::Event::Window(iced::window::Event::RedrawRequested(
        later,
    ))]);
    // Going left, the next chip's Hover comes before this one's Left: the
    // Left names the heart, so the app keeps the joy's card.
    let asks = asked(ui);
    assert!(asks.contains(&Intent::Left(sample::ITEM, "\u{2764}\u{FE0F}".into())));
    assert!(asks.contains(&Intent::Hover(sample::ITEM, "\u{1F602}".into())));
    assert!(!asks.contains(&Intent::Left(sample::ITEM, "\u{1F602}".into())));
}

#[test]
fn a_right_click_on_a_chip_opens_the_window_there() {
    use iced::mouse::{Button, Event};
    let shown = sample::chips();
    let mut ui = chips(&shown);
    let at = ui
        .find("2")
        .expect("the joy count")
        .visible_bounds()
        .expect("shown")
        .center();
    ui.point_at(at);
    ui.simulate([
        iced::Event::Mouse(Event::ButtonPressed(Button::Right)),
        iced::Event::Mouse(Event::ButtonReleased(Button::Right)),
    ]);
    assert!(asked(ui).contains(&Intent::Open(sample::ITEM, "\u{1F602}".into())));
}

#[test]
fn a_chip_where_reacting_is_refused_asks_nothing() {
    let shown = sample::locked();
    let mut ui = chips(&shown);
    ui.click("128").expect("the heart's count");
    let asks = asked(ui);
    assert!(!asks.iter().any(|ask| matches!(ask, Intent::Toggle(..))));
}

#[test]
fn no_reactions_draw_nothing() {
    let shown = Reactions::default();
    let mut ui = chips(&shown);
    assert!(ui.find("1").is_err());
}

#[test]
fn a_tab_chooses_a_reaction() {
    let shown = sample::dialog();
    let mut ui = window(&shown);
    ui.click("2").expect("the joy tab's count");
    assert_eq!(asked(ui), vec![Intent::Select("\u{1F602}".into())]);
}

#[test]
fn the_window_lists_who_reacted() {
    let shown = sample::dialog();
    let mut ui = window(&shown);
    for name in ["Alice", "Bob", "Carol", "Erin", "Reactions"] {
        assert!(ui.find(name).is_ok(), "no {name:?}");
    }
}

#[test]
fn a_poll_lists_its_voters_by_answer() {
    let shown = sample::voters();
    let mut ui = window(&shown);
    assert!(ui.find("Votes").is_ok());
    ui.click("Pizza").expect("the first answer");
    assert_eq!(asked(ui), vec![Intent::Select("1".into())]);
}

#[test]
fn the_next_page_shows_that_it_loads_and_a_failure_its_words() {
    let loading = sample::loading();
    assert!(window(&loading).find("Loading...").is_ok());
    let failed = sample::failed();
    assert!(window(&failed).find("Couldn't load the reactions").is_ok());
}

#[test]
fn the_next_page_is_asked_for_near_the_end_but_not_while_one_comes() {
    let more = Dialog {
        more: true,
        ..sample::dialog()
    };
    let frame = || {
        iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))
    };
    let mut ui = window(&more);
    ui.simulate([frame()]);
    assert!(asked(ui).contains(&Intent::Scrolled { near_end: true }));
    let loading = sample::loading();
    let mut ui = window(&loading);
    ui.simulate([frame()]);
    assert!(
        !asked(ui)
            .iter()
            .any(|ask| matches!(ask, Intent::Scrolled { .. }))
    );
}

#[test]
fn a_removable_reaction_is_taken_away_from_its_row() {
    let shown = sample::dialog();
    let mut ui = window(&shown);
    let name = ui
        .find("Alice")
        .expect("Alice")
        .visible_bounds()
        .expect("shown");
    let user = ui
        .find("alice_user")
        .expect("her username")
        .visible_bounds()
        .expect("shown");
    // The row's middle, then its X at its end, 16 px in from the card's.
    let y = (name.y + user.y + user.height) / 2.0;
    ui.point_at(iced::Point::new(name.x, y));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: iced::Point::new(name.x, y),
    })]);
    let x = 600.0 - 1.0 - 16.0 - 14.0;
    ui.point_at(iced::Point::new(x, y));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: iced::Point::new(x, y),
    })]);
    ui.simulate(iced_test::simulator::click());
    assert!(asked(ui).contains(&Intent::Remove("alice".into())));
}

#[test]
fn the_window_closes_from_its_x() {
    let shown = sample::dialog();
    let mut ui = window(&shown);
    // The X: 28 px, 16 px in from the right, in the 16 px over the title.
    let at = iced::Point::new(600.0 - 16.0 - 14.0, 16.0 + 14.0);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), vec![Intent::Close]);
}

#[test]
fn who_reacted_shows_over_the_chip_once_the_names_came() {
    let shown = sample::chips();
    let mut ui = chips(&shown);
    let who = "Alice, Bob, and 10 others reacted by :thumbsup:";
    assert!(ui.find(who).is_err(), "shown before the pointer came");
    let at = ui
        .find("12")
        .expect("the thumbs' count")
        .visible_bounds()
        .expect("shown")
        .center();
    ui.point_at(at);
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
    assert!(ui.find(who).is_ok(), "no card of who reacted");
}
