//! The header's popovers and the search beside the conversation, as the
//! Slint client's tests of them.

use iced::keyboard::{self, key};
use yapfast_core::{
    application::{Pins, Search},
    chat::{ChatMessage, SearchHit},
};

use super::{change, interact, preview, shows};
use crate::app::Message;

fn said(item: &str, body: &str) -> ChatMessage {
    ChatMessage {
        item: item.into(),
        author: "Sam".into(),
        body: body.into(),
        timestamp: 1,
        ..Default::default()
    }
}

fn escape(app: &mut crate::app::App) {
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
}

#[test]
fn pinned_messages_show_under_their_button_and_a_click_jumps_and_closes_them() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.pins = Some(Pins {
            room,
            messages: Some(Ok(vec![said("p0", "Pinned rules of the place")])),
        });
    });
    assert!(shows(&app, "Pinned Messages"));
    assert!(shows(&app, "Pinned rules of the place"));
    // A pin edited since shows as it reads now when they come again.
    change(&mut app, |state| {
        let pins = state.pins.as_mut().expect("open");
        pins.messages = Some(Ok(vec![said("p0", "Pinned rules, edited")]));
    });
    assert!(shows(&app, "Pinned rules, edited"));
    change(&mut app, |state| {
        let pins = state.pins.as_mut().expect("open");
        pins.messages = Some(Ok(vec![said("p0", "Pinned rules of the place")]));
    });
    interact(&mut app, |ui| {
        ui.click("Pinned rules of the place")
            .expect("the pin's card");
    });
    assert!(app.client.state().pins.is_none());
    assert!(!shows(&app, "Pinned Messages"));
}

#[test]
fn no_pins_says_so_and_escape_closes_them() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.pins = Some(Pins {
            room,
            messages: Some(Ok(vec![])),
        });
    });
    assert!(shows(
        &app,
        "This channel doesn't have any pinned messages yet."
    ));
    escape(&mut app);
    assert!(app.client.state().pins.is_none());
}

#[test]
fn a_search_shows_its_results_beside_and_escape_ends_it_letting_go_of_them() {
    use yapfast_core::{
        application::{Action, Navigation},
        yapfast_discord::ready::permission,
    };
    let (mut app, _folder) =
        super::server::discord_server(permission::READ_MESSAGE_HISTORY, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.client
        .dispatch(Action::SelectRoom("discord:10:11".into()));
    let _ = app.sync();
    let place = app
        .client
        .state()
        .search_place()
        .expect("a server to search");
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.search = Some(Search {
            place,
            query: "rules".into(),
            total: 1,
            results: vec![SearchHit {
                room,
                message: said("s0", "The rules, found"),
            }],
            next: 1,
            ..Default::default()
        });
    });
    assert!(shows(&app, "1 Result"));
    assert!(shows(&app, "The rules, found"));
    // A server's results are headed with their channel's name.
    assert!(shows(&app, "general"));
    assert!(
        !app.members_shown(),
        "the results take the member list's place"
    );
    escape(&mut app);
    assert!(app.client.state().search_shown().is_none());
    assert!(!shows(&app, "The rules, found"));
}

#[test]
fn add_people_counts_the_room_left_in_the_group() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.add_people = Some(yapfast_core::application::AddPeople {
            room,
            ..Default::default()
        });
    });
    assert!(shows(&app, "Select Friends"));
    let left = app.client.state().group_room();
    let detail = match left {
        0 => "This group has no room for more.".to_owned(),
        1 => "You can add 1 more friend.".to_owned(),
        left => format!("You can add {left} more friends."),
    };
    assert!(shows(&app, &detail));
    // Typing in its search leaves the focus, and so the caret, alone.
    let typed = app.popover_event(crate::popovers::Event::PeopleQuery("ri".into()));
    assert_eq!(typed.units(), 0, "nothing focused again");
    escape(&mut app);
    assert!(app.client.state().add_people.is_none());
}

#[test]
fn a_click_on_the_topic_shows_it_in_full_until_escape_or_another_room() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.topic = Some(std::sync::Arc::new(yapfast_core::chat::Topic {
            body: "Plans for game night.\nBring snacks.".into(),
            mentions: Vec::new(),
            emoji: Vec::new(),
        }));
    });
    assert!(shows(&app, "Plans for game night. Bring snacks."));
    interact(&mut app, |ui| {
        ui.click("Plans for game night. Bring snacks.")
            .expect("the topic's line");
    });
    assert!(app.popover_open());
    assert!(
        shows(&app, "Plans for game night.\nBring snacks."),
        "its own lines"
    );
    escape(&mut app);
    assert!(!app.popover_open());
    let _ = app.update(Message::Popover(crate::popovers::Event::Topic));
    assert!(app.popover_open());
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert!(!app.popover_open(), "it goes with its room");
}
