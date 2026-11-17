//! The Requests screen: task P02's tests, driving
//! `anomi_people::friends::requests::view` over its fixtures in the simulator
//! (`super::support`). The screen is laid out alone, 1000 by 700.

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_people::friends::requests::view;
use anomi_theme::Theme;
use anomi_vm::fixtures::people::requests as samples;
use anomi_vm::people::requests::{Intent, Requests};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(1000.0, 700.0);

fn simulate(requests: &Requests) -> Simulator<'_, Intent, Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(requests, front()))
}

fn click(requests: &Requests, words: &str) -> Vec<Intent> {
    let mut ui = simulate(requests);
    ui.click(words)
        .unwrap_or_else(|_| panic!("{words} on screen"));
    asked(ui)
}

/// The first row's buttons: it starts below the padding (8), the heading (36)
/// and the gap (2), and its top line is 40 tall inside the row's padding.
/// Ignore ends 10 inside the list's right padding, Accept stands before it.
const IGNORE: Point = Point::new(940.0, 76.0);
const ACCEPT: Point = Point::new(830.0, 76.0);

fn press(requests: &Requests, at: Point) -> Vec<Intent> {
    let mut ui = simulate(requests);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

fn shows(requests: &Requests, words: &str) -> bool {
    simulate(requests).find(words).is_ok()
}

#[test]
fn accept_answers_the_friend_request() {
    assert_eq!(
        press(&samples::waiting(), ACCEPT),
        [Intent::Accept("1400000000000000005".into())]
    );
}

#[test]
fn ignore_removes_the_friend_request() {
    assert_eq!(
        press(&samples::waiting(), IGNORE),
        [Intent::Remove("1400000000000000005".into())]
    );
}

#[test]
fn a_request_this_account_sent_can_only_be_cancelled() {
    let requests = Requests {
        sent: vec![samples::erin()],
        ..Requests::default()
    };
    let mut sent = requests.clone();
    sent.sent[0].friend = anomi_vm::people::requests::Friend::Sent;
    assert!(!shows(&sent, "Accept"));
    assert!(shows(&sent, "You sent a friend request"));
    assert_eq!(
        press(&sent, IGNORE),
        [Intent::Remove("1400000000000000005".into())]
    );
    assert!(shows(&sent, "SENT — 1"));
}

#[test]
fn the_message_opens_its_conversation() {
    assert_eq!(
        click(
            &samples::waiting(),
            "Hi Alice, Bob gave me your name. Can I ask about the trip?"
        ),
        [Intent::Open("discord:erin".into())]
    );
}

#[test]
fn the_row_says_what_was_asked_and_marks_the_new() {
    let requests = samples::waiting();
    assert!(shows(&requests, "REQUESTS — 1"));
    assert!(shows(&requests, "Sent you a friend request and a message"));
    assert!(shows(&requests, "New"));
    assert!(shows(&requests, "2 October 2026"));
}

#[test]
fn spam_stays_folded_until_the_heading_is_clicked() {
    let folded = samples::waiting();
    assert!(shows(&folded, "SPAM — 1"));
    assert!(!shows(&folded, "Promo Bot"));
    assert_eq!(click(&folded, "SPAM — 1"), [Intent::ToggleSpam]);
    let open = samples::spam_open();
    assert!(shows(&open, "Promo Bot"));
    assert!(shows(&open, "Maybe spam"));
    assert_eq!(click(&open, "SPAM — 1"), [Intent::ToggleSpam]);
}

#[test]
fn spam_can_still_be_answered() {
    // Spam alone: its fold heading takes the place of the first heading.
    let requests = Requests {
        spam: vec![samples::spammer()],
        spam_open: true,
        ..Requests::default()
    };
    assert_eq!(
        press(&requests, IGNORE),
        [Intent::Remove("1400000000000000009".into())]
    );
}

#[test]
fn a_message_with_nothing_known_invites_to_open_it() {
    let requests = samples::everything();
    assert!(shows(&requests, "Open to read their message"));
    assert!(shows(&requests, "Wants to message you"));
    assert_eq!(
        click(&requests, "Open to read their message"),
        [Intent::Open("discord:quinn".into())]
    );
}

#[test]
fn nothing_waiting_shows_the_empty_screen() {
    let requests = samples::none();
    assert!(shows(&requests, "No requests"));
    assert!(shows(
        &requests,
        "Friend requests and messages from people you don't know show up here."
    ));
}

#[test]
fn headings_count_each_group() {
    let requests = samples::everything();
    assert!(shows(&requests, "REQUESTS — 4"));
    assert!(shows(&requests, "SENT — 1"));
}
