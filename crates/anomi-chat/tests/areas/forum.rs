//! A forum: task C26's tests, driving `anomi_chat::forum::view` over its
//! fixtures in the simulator (`super::support`).

use iced::Widget as _;
use iced::{Event, Point, keyboard, mouse};
use iced_test::Simulator;

use anomi_chat::forum::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::forum::{Forum, Intent as Ask};
use anomi_vm::chat::new_post::NewPost;
use anomi_vm::chat::{self, Intent};
use anomi_vm::fixtures::chat::forum as sample;

use super::support::{SIZE, asked, front, settings};

fn simulate(forum: &Forum) -> Simulator<'_, chat::Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(forum, front()).boxed())
}

fn open(room: &str, beside: bool) -> Intent {
    Intent::Forum(Ask::Open {
        room: room.into(),
        beside,
    })
}

#[test]
fn a_cards_click_opens_the_post_beside_the_forum() {
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    ui.click("Dark theme for the sidebar").unwrap();
    assert_eq!(asked(ui), [open("discord:p4", true)]);
}

#[test]
fn a_click_with_shift_opens_it_in_the_forums_place() {
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    let target = ui.find("Crash when pasting a GIF").unwrap();
    ui.point_at(target.visible_bounds().unwrap().center());
    ui.simulate([Event::Keyboard(keyboard::Event::ModifiersChanged(
        keyboard::Modifiers::SHIFT,
    ))]);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [open("discord:p3", false)]);
}

#[test]
fn a_press_that_ends_outside_the_card_opens_nothing() {
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    let target = ui.find("Dark theme for the sidebar").unwrap();
    ui.point_at(target.visible_bounds().unwrap().center());
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    ui.point_at(Point::new(700.0, 20.0));
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
    assert_eq!(asked(ui), []);
}

#[test]
fn the_card_shows_what_discords_does() {
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    for shown in [
        "Welcome to #ideas",
        "Bob",
        ": ",
        "Read the guidelines before posting.",
        "Question",
        "Bug",
        "NEW",
        "OLDER POSTS",
        "Old idea 1",
        ">30d ago",
    ] {
        assert!(ui.find(shown).is_ok(), "{shown} is not shown");
    }
    // Only the first older post has the heading.
    assert_eq!(
        forum
            .rows
            .iter()
            .filter(|post| !post.heading.is_empty())
            .count(),
        1
    );
}

#[test]
fn new_post_asks_for_the_form() {
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    ui.point_at(Point::new(SIZE.width - 60.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Forum(Ask::NewPost)]);
}

#[test]
fn without_the_right_to_post_there_is_no_button() {
    let forum = sample::read_only();
    let mut ui = simulate(&forum);
    ui.point_at(Point::new(SIZE.width - 60.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), []);
    let mut ui = simulate(&forum);
    assert!(ui.find("Search posts").is_ok());
}

#[test]
fn without_the_right_to_search_there_is_no_field() {
    let forum = sample::plain();
    let mut ui = simulate(&forum);
    assert!(ui.find("Search posts").is_err());
    ui.point_at(Point::new(SIZE.width - 60.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Forum(Ask::NewPost)]);
}

#[test]
fn typing_in_the_search_sends_the_text_and_enter_searches() {
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    ui.click("Search posts").unwrap();
    ui.typewrite("b");
    assert_eq!(asked(ui), [Intent::Forum(Ask::Query("b".into()))]);

    let forum = sample::found();
    let mut ui = simulate(&forum);
    ui.point_at(Point::new(300.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    ui.tap_key(keyboard::key::Named::Enter);
    assert!(asked(ui).contains(&Intent::Forum(Ask::Search("crash".into()))));
}

#[test]
fn the_cross_clears_a_search() {
    let forum = Forum {
        can_post: false,
        ..sample::found()
    };
    let mut ui = simulate(&forum);
    // The cross sits at the field's right end, 6 px in.
    ui.point_at(Point::new(SIZE.width - 16.0 - 6.0 - 12.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Forum(Ask::Clear)]);
}

#[test]
fn a_search_shows_its_posts_and_none_of_the_rest() {
    let forum = sample::found();
    let mut ui = simulate(&forum);
    assert!(ui.find("Keyboard shortcut list?").is_ok());
    assert!(ui.find("Welcome to #ideas").is_err());
    assert!(ui.find("OLDER POSTS").is_err());
}

#[test]
fn nothing_found_and_no_posts_say_so() {
    let forum = sample::nothing_found();
    let mut ui = simulate(&forum);
    assert!(ui.find("No results found").is_ok());
    let forum = sample::empty();
    let mut ui = simulate(&forum);
    assert!(ui.find("There are no posts yet.").is_ok());
    assert!(ui.find("No results found").is_err());
}

#[test]
fn a_note_stands_in_for_the_list() {
    for (forum, note) in [
        (sample::loading(), "Loading posts..."),
        (sample::failed(), "429 Too Many Requests"),
    ] {
        let mut ui = simulate(&forum);
        assert!(ui.find(note).is_ok());
        assert!(ui.find("Old idea 1").is_err());
        assert!(ui.find("There are no posts yet.").is_err());
    }
}

#[test]
fn an_older_page_on_its_way_shows_under_the_cards() {
    let forum = sample::loading_more();
    let mut ui = simulate(&forum);
    assert!(ui.find("Loading...").is_ok());
    let forum = sample::forum();
    let mut ui = simulate(&forum);
    assert!(ui.find("Loading...").is_err());
}

#[test]
fn a_form_being_written_takes_the_lists_place() {
    let forum = Forum {
        composing: Some(NewPost::default()),
        ..sample::forum()
    };
    let mut ui = simulate(&forum);
    assert!(ui.find("Welcome to #ideas").is_err());
    assert!(ui.find("Search posts").is_err());
}

#[test]
fn scrolling_a_long_list_tells_how_near_its_end_it_is() {
    let mut forum = sample::forum();
    let rows = forum.rows.clone();
    for round in 0..8 {
        for post in &rows {
            let mut post = post.clone();
            post.heading.clear();
            post.room = format!("{}-{round}", post.room);
            forum.rows.push(post);
        }
    }
    let mut ui = simulate(&forum);
    ui.point_at(Point::new(700.0, 400.0));
    for _ in 0..40 {
        ui.simulate(iced_test::simulator::scroll(mouse::ScrollDelta::Lines {
            x: 0.0,
            y: -3.0,
        }));
    }
    let start = std::time::Instant::now();
    for frame in 1..=40 {
        let now = start + std::time::Duration::from_millis(frame * 25);
        ui.simulate([Event::Window(iced::window::Event::RedrawRequested(now))]);
    }
    let seen = asked(ui);
    assert!(
        seen.iter()
            .any(|intent| matches!(intent, Intent::Forum(Ask::Scrolled { by_hand: true, .. }))),
        "{seen:?}"
    );
}

#[test]
fn the_field_keeps_the_keys_while_the_cross_comes_and_goes() {
    let empty = sample::forum();
    let mut ui = simulate(&empty);
    ui.click("Search posts").unwrap();
    ui.typewrite("b");
    let typed = Forum {
        query: "b".into(),
        ..sample::forum()
    };
    let mut ui = ui.rebuild(view(&typed, front()).boxed());
    ui.typewrite("o");
    assert_eq!(
        asked(ui),
        [
            Intent::Forum(Ask::Query("b".into())),
            Intent::Forum(Ask::Query("bo".into())),
        ]
    );
}

#[test]
fn escape_in_a_field_with_text_clears_the_search() {
    let forum = sample::found();
    let mut ui = simulate(&forum);
    ui.point_at(Point::new(300.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    ui.tap_key(keyboard::key::Named::Escape);
    assert!(asked(ui).contains(&Intent::Forum(Ask::Clear)));

    let forum = sample::forum();
    let mut ui = simulate(&forum);
    ui.point_at(Point::new(300.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    ui.tap_key(keyboard::key::Named::Escape);
    assert!(!asked(ui).contains(&Intent::Forum(Ask::Clear)));
}
