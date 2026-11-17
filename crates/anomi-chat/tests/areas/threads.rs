//! The threads popover: task C23's tests, driving
//! `anomi_chat::threads::popover::view` over its fixtures in the simulator
//! (`super::support`).

use iced::keyboard::{self, Modifiers};
use iced::{Event, Point, Size, mouse};
use iced_test::simulator::{self, Simulator};

use anomi_chat::threads::popover::{query_id, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::threads::{Intent, Row, ThreadFrom, Threads};
use anomi_vm::fixtures::chat::threads as sample;

use super::support::{asked, front, settings};

/// The window the popover sits in, tall enough for all its rows.
const WINDOW: Size = Size::new(520.0, 760.0);

fn simulate(threads: &Threads) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), WINDOW, view(threads, front()))
}

/// A press of the left button at a point of the window.
fn press(ui: &mut Simulator<'_, Intent, Theme, Renderer>, x: f32, y: f32) {
    ui.point_at(Point::new(x, y));
    let _ = ui.simulate(simulator::click());
}

#[test]
fn it_lists_the_sections_with_their_threads() {
    let threads = sample::sections();
    let mut ui = simulate(&threads);
    for row in &threads.rows {
        if let Row::Heading(heading) = row {
            assert!(ui.find(heading.as_str()).is_ok(), "{heading}");
        }
    }
    assert!(ui.find("Threads").is_ok());
    assert!(ui.find("Plans").is_ok());
    assert!(ui.find("Book club").is_ok());
    // A joined thread's line: who said what, and when.
    assert!(ui.find("Carol").is_ok());
    assert!(ui.find(": Count me in").is_ok());
    // How many are in it.
    assert!(ui.find("3").is_ok());
}

#[test]
fn an_archived_thread_names_who_started_it() {
    let threads = sample::sections();
    let mut ui = simulate(&threads);
    assert!(ui.find("Started by ").is_ok());
    assert!(ui.find(" • Last active >30d ago").is_ok());
}

#[test]
fn a_click_opens_a_thread_in_place_of_the_channel() {
    let threads = sample::sections();
    let mut ui = simulate(&threads);
    ui.click("Book club").expect("its card");
    assert_eq!(
        asked(ui),
        [Intent::Open {
            room: "discord:t-Book club".into(),
            beside: false
        }]
    );
}

#[test]
fn a_click_with_shift_opens_it_beside_the_channel() {
    let threads = sample::sections();
    let mut ui = simulate(&threads);
    let _ = ui.simulate([Event::Keyboard(keyboard::Event::ModifiersChanged(
        Modifiers::SHIFT,
    ))]);
    ui.click("Plans").expect("its card");
    assert_eq!(
        asked(ui),
        [Intent::Open {
            room: "discord:t-Plans".into(),
            beside: true
        }]
    );
}

#[test]
fn create_starts_a_thread_and_close_closes() {
    let threads = sample::sections();
    let mut ui = simulate(&threads);
    // The buttons draw their own text, so they are pressed where they are:
    // Create left of Close, which is 10 px from the card's right edge.
    press(&mut ui, 415.0, 24.0);
    press(&mut ui, 466.0, 24.0);
    assert_eq!(asked(ui), [Intent::Create(ThreadFrom::List), Intent::Close]);
}

#[test]
fn without_the_right_to_start_one_there_is_no_create() {
    let threads = sample::unsearchable();
    let mut ui = simulate(&threads);
    assert!(ui.find("Create").is_err());
    // Nor a search, but the page on its way shows.
    assert!(ui.find("Search for Thread Name").is_err());
    assert!(ui.find("Loading...").is_ok());
}

#[test]
fn typing_edits_the_query_and_enter_searches_it() {
    let threads = sample::sections();
    let mut ui = simulate(&threads);
    ui.click(query_id()).expect("the field");
    let _ = ui.typewrite("pl");
    let edits = asked(ui);
    assert!(edits.contains(&Intent::Edit("p".into())));
    // The app stores what was typed; the view is rebuilt with it.
    let typed = Threads {
        query: "pl".into(),
        ..sample::sections()
    };
    let mut ui = simulate(&typed);
    ui.click(query_id()).expect("the field");
    let _ = ui.tap_key(keyboard::key::Named::Enter);
    assert_eq!(asked(ui).last(), Some(&Intent::Search("pl".into())));
}

#[test]
fn scrolling_the_list_tells_how_near_its_end_it_is() {
    let mut threads = sample::sections();
    let rows = threads.rows.clone();
    for round in 0..3 {
        for row in &rows {
            if let Row::Thread(thread) = row {
                let mut thread = thread.clone();
                thread.room = format!("{}-{round}", thread.room);
                threads.rows.push(Row::Thread(thread));
            }
        }
    }
    let mut ui = simulate(&threads);
    ui.point_at(Point::new(250.0, 400.0));
    for _ in 0..40 {
        let _ = ui.simulate(simulator::scroll(mouse::ScrollDelta::Lines {
            x: 0.0,
            y: -3.0,
        }));
    }
    let start = std::time::Instant::now();
    for frame in 1..=40 {
        let now = start + std::time::Duration::from_millis(frame * 25);
        let _ = ui.simulate([Event::Window(iced::window::Event::RedrawRequested(now))]);
    }
    let seen = asked(ui);
    assert!(
        seen.iter().any(|intent| matches!(
            intent,
            Intent::Scrolled { offset, near_end: true } if *offset > 0.0
        )),
        "{seen:?}"
    );
}

#[test]
fn a_search_shows_its_results_without_headings() {
    let threads = sample::found();
    let mut ui = simulate(&threads);
    assert!(ui.find("Plans").is_ok());
    assert!(ui.find("1 JOINED THREAD").is_err());
}

#[test]
fn a_note_stands_in_for_the_list() {
    for (threads, words) in [
        (sample::loading(), "Loading threads..."),
        (sample::searching(), "Searching..."),
    ] {
        let mut ui = simulate(&threads);
        assert!(ui.find(words).is_ok(), "{words}");
        assert!(ui.find("Plans").is_err());
    }
}

#[test]
fn an_empty_list_invites_the_first_thread() {
    let threads = sample::empty();
    let mut ui = simulate(&threads);
    assert!(ui.find("There are no threads.").is_ok());
    press(&mut ui, 245.0, 290.0);
    assert_eq!(asked(ui), [Intent::Create(ThreadFrom::ListEmpty)]);
}

#[test]
fn the_popover_grows_with_its_rows() {
    let short = Threads {
        rows: sample::sections().rows[..2].to_vec(),
        ..sample::sections()
    };
    assert!(short.fit() < sample::sections().fit());
    // The first section's heading and its card, under the header.
    assert_eq!(short.fit(), 49.0 + 32.0 + 80.0);
    // A note, an empty flag or the loading line do not hide the rows.
    let noted = Threads {
        note: "Searching...".into(),
        ..short.clone()
    };
    assert_eq!(noted.fit(), short.fit());
    let loading = Threads {
        loading: true,
        ..short.clone()
    };
    assert_eq!(loading.fit(), short.fit() + 34.0);
}
