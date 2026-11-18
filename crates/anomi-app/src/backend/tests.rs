//! The gate on the window: who's typing elsewhere and the member list while
//! it doesn't show make no message, and the window shows them as they are
//! the moment it shows them.

use yapfast_core::application::Action;
use yapfast_core::chat::{Event, Presence};

use anomi_vm::people::members::Row;

use crate::app::App;
use crate::message::Message;
use crate::project::people::tests::member;
use crate::tests::start;

fn members(names: &[&str]) -> Event {
    let list = (names.iter().enumerate())
        .map(|(n, name)| member(&n.to_string(), name, Presence::Online, "Online"))
        .collect();
    Event::Members {
        room: "general".into(),
        members: Ok(list),
    }
}

fn typing(room: &str, who: &str) -> Event {
    Event::Typing {
        room: room.into(),
        names: vec![who.into()],
    }
}

/// The names the member list shows.
fn listed(app: &App) -> Vec<&str> {
    (app.vm.people.members.rows.iter())
        .filter_map(|row| match row {
            Row::Member(member) => Some(member.name.as_str()),
            Row::Heading(_) => None,
        })
        .collect()
}

/// What the gate lets through as a message.
fn passes(app: &App, event: Event) -> bool {
    !app.backend.gate.sift(vec![event]).is_empty()
}

fn toggle_members() -> Message {
    Message::Intent(anomi_vm::Intent::Header(anomi_vm::header::Intent::Members))
}

#[test]
fn a_hidden_member_list_takes_no_message_and_shows_the_latest_as_it_opens() {
    let mut window = start("gate-members", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    drop(app.update(toggle_members()));
    assert!(app.layout.members_hidden);
    assert!(!passes(app, members(&["Ann"])));
    assert!(!passes(app, members(&["Ann", "Bob"])));
    // Any message hands the core what waited; nothing shows it yet.
    drop(app.update(Message::AskPictures));
    assert!(app.vm.people.members.rows.is_empty());
    assert!(!passes(app, members(&["Ann", "Bob", "Cat"])));
    drop(app.update(toggle_members()));
    assert_eq!(listed(app), ["Ann", "Bob", "Cat"]);
    // Shown, a list is a message again.
    assert!(passes(app, members(&["Ann"])));
}

#[test]
fn behind_another_window_typing_and_the_list_wait_for_the_front() {
    let mut window = start("gate-front", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    drop(app.update(Message::AskPictures));
    // In front: typing where it shows passes, typing elsewhere waits.
    assert!(passes(app, typing("general", "Sam")));
    assert!(!passes(app, typing("elsewhere", "Sam")));
    assert!(passes(app, members(&["Ann"])));
    drop(app.update(Message::Focused(false)));
    assert!(!passes(app, typing("general", "Sam")));
    assert!(!passes(app, members(&["Ann", "Bob"])));
    assert!(app.vm.chat.composer.typing.is_empty());
    drop(app.update(Message::Focused(true)));
    assert_eq!(app.vm.chat.composer.typing, "Sam is typing...");
    assert_eq!(listed(app), ["Ann", "Bob"]);
}

#[test]
fn in_the_tray_nothing_passes_and_the_window_shown_again_shows_the_latest() {
    let mut window = start("gate-tray", true);
    let app = &mut window.app;
    app.window = Some(iced::window::Id::unique());
    drop(app.dispatch(Action::Open("general".into())));
    drop(crate::platform::forget::window(app));
    drop(app.update(Message::AskPictures));
    assert!(app.hidden);
    assert!(!passes(app, typing("general", "Sam")));
    assert!(!passes(app, members(&["Ann", "Bob"])));
    drop(crate::platform::window::open(app, None));
    drop(app.update(Message::Focused(true)));
    assert_eq!(app.vm.chat.composer.typing, "Sam is typing...");
    assert_eq!(listed(app), ["Ann", "Bob"]);
}
