//! The header's popovers and the search beside the conversation.

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
                thread: None,
            }],
            ..Default::default()
        });
    });
    assert!(shows(&app, "1 Result"));
    assert!(shows(&app, "The rules, found"));
    // A server's results are headed with their channel's name.
    assert!(shows(&app, "general"));
    assert!(
        !crate::header::members_shown(&app),
        "the results take the member list's place"
    );
    escape(&mut app);
    assert!(app.client.state().search_shown().is_none());
    assert!(!shows(&app, "The rules, found"));
}

/// A server's #general open, which its search box searches.
fn in_general() -> (crate::app::App, tempfile::TempDir) {
    use yapfast_core::{
        application::{Action, Navigation},
        yapfast_discord::ready::permission,
    };
    let (mut app, folder) =
        super::server::discord_server(permission::READ_MESSAGE_HISTORY, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.client
        .dispatch(Action::SelectRoom("discord:10:11".into()));
    let _ = app.sync();
    (app, folder)
}

#[test]
fn ctrl_f_goes_to_the_search_filled_with_the_channel_and_ctrl_shift_f_keeps_it() {
    use crate::search::{Event, FIELD};
    let (mut app, _folder) = in_general();
    assert!(
        app.client.state().search_place().is_some(),
        "a box to search"
    );
    // Ctrl+F: the box takes the keys, with Discord's `in:` in place of what
    // it held.
    app.searching.text = "rules".into();
    let focused = app.update(Message::Search(Event::Focus { channel: true }));
    assert_eq!(app.searching.text, "in:#general ");
    assert!(focused.units() > 0, "the box focused");
    // Ctrl+Shift+F keeps what's typed.
    app.searching.text = "rules".into();
    let focused = app.update(Message::Search(Event::Focus { channel: false }));
    assert_eq!(app.searching.text, "rules");
    assert!(focused.units() > 0, "the box focused");
    // Typing goes on after it.
    interact(&mut app, |ui| {
        ui.click(iced::widget::Id::new(FIELD)).expect("the box");
        ui.typewrite(" now");
    });
    assert_eq!(app.searching.text, "rules now");
    // Not over the settings.
    let _ = app.update(Message::Settings(crate::settings::Event::Open(
        crate::settings::Section::Appearance,
    )));
    let over = app.update(Message::Search(Event::Focus { channel: true }));
    assert_eq!(over.units(), 0, "nothing focused");
    assert_eq!(app.searching.text, "rules now");
}

#[test]
fn escape_in_the_search_box_clears_it_keeping_the_keys_then_lets_go() {
    use crate::search::{Event, FIELD};
    let (mut app, _folder) = in_general();
    let mut escapes = 0;
    let messages = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.click(iced::widget::Id::new(FIELD)).expect("the box");
        ui.typewrite("rules");
        ui.tap_key(keyboard::Key::Named(key::Named::Escape));
        ui.into_messages().collect::<Vec<_>>()
    };
    for message in messages {
        if matches!(message, Message::Search(Event::Escape)) {
            escapes += 1;
            assert_eq!(app.searching.text, "rules");
            let task = app.update(message);
            assert_eq!(task.units(), 0, "the box keeps the keys");
            assert_eq!(app.searching.text, "");
        } else {
            let _ = app.update(message);
        }
    }
    assert_eq!(escapes, 1, "the box handed its Escape on");
    // In the empty box, Escape lets go of the keys.
    let task = app.update(Message::Search(Event::Escape));
    assert!(task.units() > 0, "the box lets go");
    // Escape anywhere else isn't the box's.
    let mut ui =
        iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    ui.tap_key(keyboard::Key::Named(key::Named::Escape));
    assert!(
        !ui.into_messages()
            .any(|message| matches!(message, Message::Search(Event::Escape)))
    );
}

/// Clicks the header's search box, does `keys` in it, and hands the app
/// what came of them.
fn in_search_box(app: &mut crate::app::App, keys: &dyn Fn(&mut iced_test::Simulator<'_, Message>)) {
    let messages = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.click(iced::widget::Id::new(crate::search::FIELD))
            .expect("the box");
        keys(&mut ui);
        ui.into_messages().collect::<Vec<_>>()
    };
    for message in messages {
        let _ = app.update(message);
    }
}

/// Discord's popout under the box: a filter's answers with the first lit,
/// which Enter takes and searches; the search kept in the place's
/// history, which the empty box offers.
#[test]
fn the_search_popout_answers_a_filter_and_keeps_the_search_in_history() {
    use crate::search_popout;
    let (mut app, _folder) = in_general();
    in_search_box(&mut app, &|ui| {
        ui.typewrite("has:");
    });
    assert!(search_popout::open(&app.searching), "open while typing");
    assert!(search_popout::has_rows(&app.searching, app.client.state()));
    assert_eq!(app.searching.lit, Some(0), "the first answer lit");
    in_search_box(&mut app, &|ui| {
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert_eq!(app.searching.text, "has: image ");
    assert_eq!(
        app.client
            .state()
            .search
            .as_ref()
            .map(|search| search.query.as_str()),
        Some("has: image")
    );
    assert!(!search_popout::open(&app.searching), "closed by the search");
    let place = app.client.state().search_place().expect("a place");
    assert_eq!(
        app.layout.search_history.get(&place),
        Some(&vec!["has: image".to_owned()])
    );
}

/// The box's filters as Discord's pills: keys and list-picked answers go
/// whole under Backspace; a person's name is edited a letter at a time.
#[test]
fn the_search_box_takes_a_filter_whole_and_edits_a_name_by_letters() {
    use crate::search::Event;
    let (mut app, _folder) = in_general();
    let run = |app: &mut crate::app::App, keys: &dyn Fn(&mut iced_test::Simulator<'_, Message>)| {
        in_search_box(app, keys);
    };
    let backspace = |ui: &mut iced_test::Simulator<'_, Message>| {
        ui.tap_key(keyboard::Key::Named(key::Named::Backspace));
    };
    run(&mut app, &|ui| {
        ui.typewrite("plan has: link");
    });
    assert_eq!(app.searching.text, "plan has: link");
    // The answer, then the key, each whole.
    run(&mut app, &|ui| {
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        backspace(ui);
    });
    assert_eq!(app.searching.text, "plan has:");
    run(&mut app, &|ui| {
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        backspace(ui);
    });
    assert_eq!(app.searching.text, "plan ");
    // A person's name goes a letter at a time.
    run(&mut app, &|ui| {
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.typewrite("from: bob");
        backspace(ui);
    });
    assert_eq!(app.searching.text, "plan from: bo");
    let _ = app.update(Message::Search(Event::Clear));
    assert_eq!(app.searching.text, "");
}

#[test]
fn add_people_counts_the_room_left_in_the_group() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        // A group of 8, of Discord's 10.
        for each in state.rooms.iter_mut().filter(|each| each.id == room) {
            std::sync::Arc::make_mut(each).members = 8;
        }
        state.add_people = Some(yapfast_core::application::AddPeople {
            room,
            ..Default::default()
        });
    });
    assert!(shows(&app, "Select Friends"));
    assert!(shows(&app, "You can add 2 more friends."));
    // Typing in its search leaves the focus, and so the caret, alone.
    let typed = crate::header_popovers::popover_event(
        &mut app,
        crate::header_popovers::Event::PeopleQuery("ri".into()),
    );
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
    assert!(crate::header_popovers::popover_open(&app));
    assert!(
        shows(&app, "Plans for game night.\nBring snacks."),
        "its own lines"
    );
    escape(&mut app);
    assert!(!crate::header_popovers::popover_open(&app));
    let _ = app.update(Message::Popover(crate::header_popovers::Event::Topic));
    assert!(crate::header_popovers::popover_open(&app));
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert!(
        !crate::header_popovers::popover_open(&app),
        "it goes with its room"
    );
}

#[test]
fn a_dms_call_buttons_do_nothing_while_a_call_cant_be_joined() {
    use std::sync::Arc;
    use yapfast_core::application::{Action, Navigation};
    // Whether a click anywhere along the rows the name "Riley" shows on
    // right of the sidebar, the header's among them, starts a call.
    let starts = |app: &mut crate::app::App| {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        let left = crate::rail::WIDTH + crate::sidebar::width(&app.layout);
        let mut rows = Vec::new();
        let _ = ui.find(|candidate: iced_test::selector::Candidate<'_>| {
            if let iced_test::selector::Candidate::Text {
                content,
                visible_bounds: Some(bounds),
                ..
            } = candidate
                && content == "Riley"
                && bounds.x > left
            {
                rows.push(bounds.center_y());
            }
            None::<()>
        });
        assert!(!rows.is_empty(), "the header names Riley");
        for y in rows {
            let mut x = left;
            while x < app.shell.frame.width {
                ui.point_at(iced::Point::new(x, y));
                ui.simulate(iced_test::simulator::click());
                x += 4.0;
            }
        }
        ui.into_messages()
            .any(|message| matches!(message, Message::Call(crate::call::Event::Start(_))))
    };
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let riley = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "riley")
            .expect("Riley's DM");
        Arc::make_mut(riley).capabilities.dm_calls = true;
    });
    app.client
        .dispatch(Action::Navigate(Navigation::DirectMessages));
    app.client.dispatch(Action::Open("riley".into()));
    let _ = app.sync();
    assert!(app.header.can_call, "Riley's DM has a call");
    assert!(starts(&mut app), "Start Voice Call");
    // Another call joining, this one can't be.
    change(&mut app, |state| {
        state.voice.phase = yapfast_core::voice::Phase::Joining;
    });
    assert!(app.header.can_call);
    assert!(!starts(&mut app), "both buttons greyed");
    // Joined, with nothing else about the lists changed, they're lit again.
    app.client.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
    let _ = app.sync();
    assert!(starts(&mut app), "lit once the other call is joined");
}

#[test]
fn a_link_in_a_pinned_messages_text_opens_and_the_card_doesnt_jump() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.pins = Some(Pins {
            room,
            messages: Some(Ok(vec![ChatMessage {
                formatted: true,
                ..said("p0", "[docs](https://elsewhere.example/page)")
            }])),
        });
    });
    // The text is as wide as the card: the link is at its start.
    interact(&mut app, |ui| {
        let text = ui.find("docs").expect("the text");
        let text = text.visible_bounds().expect("in view");
        ui.point_at(iced::Point::new(text.x + 10.0, text.center_y()));
        ui.simulate([
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
        ]);
    });
    // A masked link asks before it opens.
    assert_eq!(
        app.leaving.as_deref(),
        Some("https://elsewhere.example/page")
    );
    assert!(app.client.state().pins.is_some(), "not jumped");
}

#[test]
fn search_results_show_their_count_channel_pages_and_a_sort_menu() {
    use crate::search::Event;
    use yapfast_core::chat::{ChatMessage, Event as Core, SearchHit, SearchSort, Searched};
    let (mut app, _folder) = in_general();
    app.searching.text = "plan".into();
    let _ = app.update(Message::Search(Event::Search));
    assert!(shows(&app, "Searching…"));
    let hit = |n: u32, bot: bool| SearchHit {
        room: "discord:10:11".into(),
        message: ChatMessage {
            item: format!("d:{n}"),
            body: "The plan for tonight".into(),
            author: "Bob".into(),
            bot,
            ..ChatMessage::default()
        },
        thread: None,
    };
    let page = |sort, offset, hits: Vec<SearchHit>| Core::Found {
        place: "discord:10".into(),
        query: "plan".into(),
        sort,
        offset,
        result: Ok(Searched::Page {
            total: 60,
            hits,
            words: "plan".into(),
            indexing_older: false,
        }),
    };
    app.client.apply(page(
        SearchSort::Newest,
        0,
        (0..25).map(|n| hit(n, false)).collect(),
    ));
    let _ = app.sync();
    assert!(shows(&app, "60 Results"));
    assert!(shows(&app, "general"), "the channel's heading");
    for button in ["Back", "1", "2", "3", "Next"] {
        assert!(super::present(&app, button), "{button}");
    }
    assert!(
        !super::present(&app, "Want to filter out search results from bots?"),
        "people's results"
    );
    let _ = app.update(Message::Search(Event::Page(1)));
    let search = app.client.state().search.clone().unwrap();
    assert!(search.loading && search.page == 1);
    // Most of a page from bots brings Discord's hint, which adds the filter.
    app.client.apply(page(
        SearchSort::Newest,
        25,
        (25..50).map(|n| hit(n, true)).collect(),
    ));
    let _ = app.sync();
    assert!(
        !super::present(&app, "Want to filter out search results from bots?"),
        "only on the first page"
    );
    let _ = app.update(Message::Search(Event::Page(0)));
    app.client.apply(page(
        SearchSort::Newest,
        0,
        (0..25).map(|n| hit(n, true)).collect(),
    ));
    let _ = app.sync();
    assert!(shows(&app, "Want to filter out search results from bots?"));
    interact(&mut app, |ui| {
        ui.click("Try adding authorType: user to your search.")
            .expect("the hint's link");
    });
    assert_eq!(app.searching.text, "plan authorType: user ");
    let search = app.client.state().search.clone().unwrap();
    assert_eq!(search.query, "plan authorType: user");
    app.client.apply(Core::Found {
        place: "discord:10".into(),
        query: "plan authorType: user".into(),
        sort: SearchSort::Newest,
        offset: 0,
        result: Ok(Searched::Page {
            total: 2,
            hits: vec![hit(1, false), hit(2, false)],
            words: "plan".into(),
            indexing_older: false,
        }),
    });
    let _ = app.sync();
    assert!(shows(&app, "2 Results"));
    assert!(!super::present(&app, "Next"), "one page has no pages");

    // The Sort menu searches again in its order.
    let _ = app.update(Message::Search(Event::SortMenu));
    assert!(shows(&app, "Most Relevant"));
    interact(&mut app, |ui| {
        ui.click("Oldest").expect("Oldest");
    });
    let search = app.client.state().search.clone().unwrap();
    assert_eq!((search.sort, search.page), (SearchSort::Oldest, 0));
    assert!(search.loading);
}

#[test]
fn search_results_highlight_the_words_found_but_not_in_code() {
    use yapfast_core::search_words::Words;
    let mut body = crate::markdown::plain("Plans are planned");
    crate::markdown::highlight(&mut body, &Words::of("plan"));
    let marked: Vec<_> = body.parts[0]
        .runs
        .iter()
        .map(|run| (run.text.as_str(), run.highlight))
        .collect();
    assert_eq!(
        marked,
        [("Plans", true), (" are ", false), ("planned", true)]
    );
    let mut code = crate::markdown::parse("```\nplan\n```", &[], 0, "d:1", |_| true);
    crate::markdown::highlight(&mut code, &Words::of("plan"));
    assert!(
        code.parts
            .iter()
            .all(|part| part.runs.iter().all(|run| !run.highlight))
    );
}

#[test]
fn search_pages_go_as_discords_do() {
    use crate::search::{PageButton::*, page_buttons};
    assert_eq!(
        page_buttons(1, 3, 5),
        [
            Back { disabled: true },
            Page(1, true),
            Page(2, false),
            Page(3, false),
            Next { disabled: false },
        ]
    );
    // Far from both ends: the first and last pages, gaps, and the shown
    // one, as Discord's pages squeeze five in.
    assert_eq!(
        page_buttons(10, 400, 5),
        [
            Back { disabled: false },
            Page(1, false),
            Gap,
            Page(10, true),
            Gap,
            Page(400, false),
            Next { disabled: false },
        ]
    );
    assert_eq!(
        page_buttons(1, 400, 5),
        [
            Back { disabled: true },
            Page(1, true),
            Page(2, false),
            Page(3, false),
            Gap,
            Page(400, false),
            Next { disabled: false },
        ]
    );
    assert_eq!(
        page_buttons(400, 400, 5),
        [
            Back { disabled: false },
            Page(1, false),
            Gap,
            Page(398, false),
            Page(399, false),
            Page(400, true),
            Next { disabled: true },
        ]
    );
}

#[test]
fn results_from_someone_blocked_hide_behind_a_row_that_shows_them() {
    use crate::search::Event;
    use yapfast_core::{
        application::Search,
        chat::{Blocked, ChatMessage, SearchHit},
        discord,
    };
    let (mut app, _folder) = in_general();
    let place = app.client.state().search_place().expect("a server");
    let said = |item: &str, user: &str, body: &str| SearchHit {
        room: "discord:10:11".into(),
        message: ChatMessage {
            item: item.into(),
            body: body.into(),
            author: user.into(),
            sender: discord::sender_id(user),
            ..ChatMessage::default()
        },
        thread: None,
    };
    change(&mut app, |state| {
        state.discord_blocked.push(Blocked {
            id: "7".into(),
            name: "Mallory".into(),
            username: None,
            avatar: None,
        });
        state.search = Some(Search {
            place,
            query: "plan".into(),
            total: 2,
            results: vec![
                said("s1", "7", "Mallory's plan"),
                said("s2", "8", "Trent's plan"),
            ],
            ..Default::default()
        });
    });
    assert!(shows(
        &app,
        "We've hidden 1 result from users you've blocked."
    ));
    assert!(!super::present(&app, "Mallory's plan"));
    assert!(super::present(&app, "Trent's plan"));
    let _ = app.update(Message::Search(Event::ShowHidden(true)));
    assert!(super::present(&app, "Mallory's plan"));
    assert!(shows(&app, "Hide 1 message from blocked users"));
}

/// Discord's Filters dialog reads the box, and Apply writes its filters
/// back in the client's order, the words last, and searches.
#[test]
fn the_filters_dialog_writes_the_box_in_discords_order_and_searches() {
    use crate::search::Event;
    use crate::search_filters::{Field, FiltersEvent};
    let (mut app, _folder) = in_general();
    app.searching.text = "plan has: link".into();
    let mut filters = |event| {
        let _ = app.update(Message::Search(Event::Filters(event)));
    };
    filters(FiltersEvent::Open);
    filters(FiltersEvent::Toggle(Field::AuthorType));
    filters(FiltersEvent::Add(0));
    filters(FiltersEvent::Pinned(1));
    filters(FiltersEvent::Apply);
    assert!(!crate::search_filters::is_open(&app));
    let want = "has: link authorType: user pinned: false plan";
    assert_eq!(app.searching.text, want);
    assert_eq!(
        app.client
            .state()
            .search
            .as_ref()
            .map(|search| search.query.as_str()),
        Some(want)
    );
}
