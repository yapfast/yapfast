//! The timeline: a channel's messages and header, the history's edges,
//! the unread bar, a message's menu, cards under a message, where the view
//! stays as messages come, and the clocks.

use std::sync::Arc;

use iced::{
    keyboard::{self, key},
    widget::Id,
};
use iced_test::simulator::click;
use yapfast_core::application::Compose;

use super::{
    at_top, change, drawn, interact, long_history, numbered, present, preview, right_click,
    scroll_up, shows, starting,
};
use crate::{app::Message, composer::COMPOSER};

#[test]
fn a_messages_menu_replies_to_it() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    interact(&mut app, |ui| right_click(ui, text));
    assert!(app.win.menu.is_some(), "the menu opened");
    assert!(
        !shows(&app, "Delete message"),
        "Riley's message isn't ours to delete"
    );
    interact(&mut app, |ui| {
        ui.click("Reply").expect("Reply");
    });
    assert!(app.win.menu.is_none());
    match app.client.state().compose() {
        Some(Compose::Reply { author, .. }) => assert_eq!(author, "Riley"),
        _ => panic!("not replying"),
    }
    assert!(shows(&app, &format!("Replying to Riley: {text}")));
}

#[test]
fn deleting_ones_own_message_asks_first() {
    let (mut app, _folder) = preview();
    let text = "Nice, let's get a game going.";
    interact(&mut app, |ui| right_click(ui, text));
    interact(&mut app, |ui| {
        ui.click("Delete message").expect("Delete message");
    });
    assert!(app.client.state().confirm_delete.is_some());
    assert!(shows(&app, "Are you sure you want to delete this message?"));
    interact(&mut app, |ui| {
        ui.click("Delete").expect("the dialog's Delete");
    });
    assert!(app.client.state().confirm_delete.is_none());
    assert!(!shows(&app, text), "the message went");
}

#[test]
fn shift_and_a_click_on_the_menus_delete_deletes_without_asking() {
    let (mut app, _folder) = preview();
    let text = "Nice, let's get a game going.";
    interact(&mut app, |ui| right_click(ui, text));
    let _ = app.update(Message::Modifiers(keyboard::Modifiers::SHIFT));
    interact(&mut app, |ui| {
        ui.click("Delete message").expect("Delete message");
    });
    assert!(app.client.state().confirm_delete.is_none(), "it didn't ask");
    assert!(!shows(&app, text), "the message went");
}

#[test]
fn switching_channels_shows_each_ones_messages_header_and_topic() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.topic = Some(Arc::new(yapfast_core::chat::Topic {
            body: "Plans for **game night**".into(),
            mentions: Vec::new(),
            emoji: Vec::new(),
        }));
    });
    let general = app.header.name.clone();
    assert!(
        shows(&app, "Plans for game night"),
        "the topic, on one line"
    );
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.header.name, "games");
    assert!(shows(&app, "What are we playing next?"));
    interact(&mut app, |ui| {
        ui.click(general.as_str()).expect("the first channel");
    });
    assert_eq!(app.header.name, general);
    assert!(!shows(&app, "What are we playing next?"));
    assert_eq!(
        app.timeline.from_end, 0.0,
        "a channel opens at its newest message"
    );
}

#[test]
fn the_history_edge_offers_earlier_messages_then_shows_them_loading() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.history_start = false;
    });
    assert!(shows(&app, "Earlier messages"));
    let _ = app.update(Message::Timeline(crate::timeline::Event::Earlier));
    assert!(
        !app.client.state().loading_history,
        "the preview has no earlier history"
    );
    change(&mut app, |state| state.loading_history = true);
    assert!(shows(&app, "Loading earlier messages..."));
    // The page comes: older messages above, the newest still in view.
    let newest = app.timeline.messages.last().expect("messages").body.clone();
    change(&mut app, |state| {
        let mut older = (*state.messages[0]).clone();
        older.id = "older".into();
        older.item = "older".into();
        older.body = "From long ago".into();
        older.timestamp -= 3600;
        let mut messages = vec![Arc::new(older)];
        messages.extend(state.messages.iter().cloned());
        state.messages = Arc::new(messages);
        state.loading_history = false;
        state.history_start = true;
    });
    assert!(shows(&app, "From long ago"));
    assert!(shows(&app, &newest));
    assert!(shows(&app, "This is the start of #general."));
}

#[test]
fn the_preview_says_so_and_a_notice_shows_until_dismissed() {
    let (mut app, _folder) = preview();
    assert!(shows(
        &app,
        "Local preview: sample data. Nothing leaves this computer."
    ));
    change(&mut app, |state| {
        state.notice = Some("Discord asked Yapfast to slow down.".into());
    });
    assert!(shows(&app, "Discord asked Yapfast to slow down."));
    interact(&mut app, |ui| {
        ui.click("Dismiss").expect("Dismiss");
    });
    assert!(app.client.state().notice.is_none());
    assert!(!shows(&app, "Discord asked Yapfast to slow down."));
}

#[test]
fn a_window_of_older_messages_ends_with_newer_messages_loading_or_failed() {
    let (mut app, _folder) = preview();
    let height = *app.timeline.heights.last().expect("rows");
    assert!(!shows(&app, "Newer messages"));
    change(&mut app, |state| state.detached = true);
    assert!(shows(&app, "Newer messages"));
    assert_eq!(
        *app.timeline.heights.last().expect("rows"),
        height + crate::timeline::EDGE,
        "the last row makes room for it"
    );
    change(&mut app, |state| state.loading_later = true);
    assert!(shows(&app, "Loading newer messages..."));
    change(&mut app, |state| {
        state.loading_later = false;
        state.later_failed = true;
    });
    assert!(shows(&app, "Couldn't load newer messages. Retry"));
    // Back at the present: the edge goes, and its room with it.
    change(&mut app, |state| {
        state.later_failed = false;
        state.detached = false;
    });
    assert!(!shows(&app, "Newer messages"));
    assert_eq!(*app.timeline.heights.last().expect("rows"), height);
}

#[test]
fn the_unread_bar_jumps_to_the_new_divider_out_of_view() {
    let (mut app, _folder) = preview();
    let items = long_history(&mut app, 80);
    change(&mut app, |state| {
        state.first_unread = Some(items[10].clone());
        state.caught_up = false;
    });
    assert!(
        !shows(&app, "Message number 10"),
        "the divider is above the view"
    );
    assert!(
        shows(&app, "Mark as read"),
        "the banner offers to mark the room read"
    );
    interact(&mut app, |ui| {
        let bar = starting(ui, "70 new messages since ").expect("the bar counts what's new");
        ui.point_at(bar);
        ui.simulate(click());
    });
    let _ = app.sync();
    // The rows round it are built; the scroll there is a task, which
    // doesn't run here, so the list itself stays where it was.
    assert!(present(&app, "Message number 10"), "the view went to it");
    assert!(present(&app, "NEW"));
    assert!(app.timeline.from_end > 0.0);
}

#[test]
fn a_link_preview_and_a_file_show_in_their_cards() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let mut message = (**state.messages.last().expect("a message")).clone();
        message.id = "card".into();
        message.item = "card".into();
        message.body = "Look at this".into();
        message.reply = None;
        message.reactions = Vec::new();
        message.embeds = vec![yapfast_core::chat::Embed {
            provider: "Example".into(),
            title: "Trip notes".into(),
            url: "https://example.com/notes".into(),
            description: "Everything for **Saturday**".into(),
            ..Default::default()
        }];
        message.attachments = vec![yapfast_core::chat::Attachment {
            name: "packing.txt".into(),
            size: Some(2048),
            image: false,
            dimensions: None,
            preview: None,
            preview_failed: false,
            placement: yapfast_core::chat::Placement::Message,
            spoiler: false,
            video: false,
            gif: None,
            url: None,
            description: None,
        }];
        let mut messages = (*state.messages).clone();
        messages.push(Arc::new(message));
        state.messages = Arc::new(messages);
    });
    assert!(shows(&app, "Look at this"), "the message shows");
    for text in [
        "Example",
        "Trip notes",
        "Everything for Saturday",
        "packing.txt",
        "2 KB",
    ] {
        assert!(shows(&app, text), "{text:?} shows");
    }
}

#[test]
fn new_messages_leave_the_earlier_ones_being_read_where_they_are() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    scroll_up(&mut app, 900.0);
    assert!(
        !app.timeline.at_end && app.timeline.from_end > 0.0,
        "the wheel went up"
    );
    let read = at_top(&app);
    // Three come; then three more as the oldest three go, as the core
    // keeps only so many.
    let new = numbered(&app, 80, 83);
    change(&mut app, |state| {
        let mut messages = (*state.messages).clone();
        messages.extend(new);
        state.messages = Arc::new(messages);
    });
    assert_eq!(at_top(&app), read, "the view stayed on the same message");
    let new = numbered(&app, 83, 86);
    change(&mut app, |state| {
        let mut messages = state.messages[3..].to_vec();
        messages.extend(new);
        state.messages = Arc::new(messages);
    });
    assert_eq!(at_top(&app), read, "and stays as the oldest go");
    assert!(
        !shows(&app, "Message number 85"),
        "the newest is out of view"
    );
}

#[test]
fn sending_while_reading_earlier_messages_goes_to_the_newest() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    scroll_up(&mut app, 1500.0);
    assert!(!app.timeline.at_end);
    assert!(!shows(&app, "Message number 79"));
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("back to the present");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(
        app.timeline.at_end && app.timeline.from_end == 0.0,
        "the view went to the end"
    );
    // The preview's room takes its own history back as it sends.
    assert!(shows(&app, "back to the present"));
    let (_, last) = app.timeline.in_view(0.0);
    assert_eq!(last, app.timeline.messages.len(), "the newest is in view");
}

#[test]
fn messages_the_core_makes_anew_keep_their_text_laid_out() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 40);
    let laid: Vec<_> = app
        .timeline
        .looks
        .iter()
        .map(|look| look.body.clone())
        .collect();
    // As when a name or a role changes: every message anew, one with
    // another author and one with other words.
    change(&mut app, |state| {
        let mut messages: Vec<_> = state
            .messages
            .iter()
            .map(|message| Arc::new((**message).clone()))
            .collect();
        Arc::make_mut(&mut messages[36]).author = "Someone else".into();
        Arc::make_mut(&mut messages[38]).body = "Other words".into();
        state.messages = Arc::new(messages);
    });
    for (index, (look, before)) in app.timeline.looks.iter().zip(&laid).enumerate() {
        assert_eq!(Arc::ptr_eq(&look.body, before), index != 38, "row {index}");
    }
    assert!(shows(&app, "Other words"));
}

#[test]
fn at_midnight_the_message_times_are_worked_out_again() {
    let (mut app, _folder) = preview();
    assert!(
        app.timeline.clocks.day_armed,
        "a timer to midnight while a timeline shows"
    );
    let time = app.timeline.looks[0].time.clone();
    app.timeline.looks[0].time = "as at noon yesterday".into();
    let _ = app.update(Message::Day);
    assert_eq!(app.timeline.looks[0].time, time, "worked out for now");
    assert!(
        app.timeline.clocks.day_armed,
        "and the next midnight's armed"
    );
}

#[test]
fn the_minute_timer_runs_only_while_something_shows_minutes() {
    let (mut app, _folder) = preview();
    // Nothing in the preview shows minutes: no timer.
    app.roster.activity_cards.clear();
    app.timeline.clocks = Default::default();
    let _ = crate::timeline::clocks::arm_clocks(&mut app);
    assert!(!app.timeline.clocks.minute_armed);
    // A call's length in the sidebar does.
    app.timeline.clocks.calls = true;
    let _ = crate::timeline::clocks::arm_clocks(&mut app);
    assert!(app.timeline.clocks.minute_armed);
    // The minute turns; with nothing left to show it, none is armed again.
    app.timeline.clocks.calls = false;
    let _ = app.update(Message::Minute);
    assert!(!app.timeline.clocks.minute_armed);
}

/// Gives the message reading `body` these reactions.
fn react_to(app: &mut crate::app::App, body: &str, reactions: Vec<yapfast_core::chat::Reaction>) {
    change(app, |state| {
        let messages = state
            .messages
            .iter()
            .map(|message| {
                let mut message = (**message).clone();
                if message.body == body {
                    message.reactions = reactions.clone();
                }
                Arc::new(message)
            })
            .collect();
        state.messages = Arc::new(messages);
    });
}

#[test]
fn a_click_on_a_reaction_adds_to_it_and_another_takes_it_back() {
    use yapfast_core::chat::Reaction;
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let reaction = |count, mine| Reaction {
        key: "wave".into(),
        count,
        mine,
    };
    react_to(&mut app, text, vec![reaction(1, false)]);
    let reactions = |app: &crate::app::App| {
        let state = app.client.state();
        let message = state.messages.iter().find(|message| message.body == text);
        message.expect("the message").reactions.clone()
    };
    // The preview keeps its own messages, which nobody reacted to.
    interact(&mut app, |ui| {
        ui.click("wave").expect("the reaction");
    });
    assert_eq!(reactions(&app), [reaction(1, true)]);
    interact(&mut app, |ui| {
        ui.click("wave").expect("the reaction");
    });
    assert_eq!(reactions(&app), []);
}

#[test]
fn reacting_follows_what_the_channel_allows() {
    use yapfast_core::chat::{Reaction, Reacts};
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    react_to(
        &mut app,
        text,
        vec![Reaction {
            key: "wave".into(),
            count: 5,
            mine: false,
        }],
    );
    // A Discord channel, which says what reacting its permissions leave.
    let allow = |app: &mut crate::app::App, reacts| {
        change(app, |state| {
            state.discord_online = true;
            let open = state.selected.clone();
            for room in &mut state.rooms {
                if Some(&room.id) == open.as_ref() {
                    let room = Arc::make_mut(room);
                    room.network = yapfast_core::network::Network::Discord;
                    room.reacts = reacts;
                }
            }
        });
    };
    let adds = |app: &mut crate::app::App| {
        interact(app, |ui| right_click(ui, text));
        let adds = shows(app, "Add reaction");
        let _ = app.update(Message::Menu(crate::menus::Event::Close));
        adds
    };
    let mine = |app: &crate::app::App| {
        let state = app.client.state();
        let message = state.messages.iter().find(|message| message.body == text);
        message.expect("the message").reactions[0].mine
    };
    allow(&mut app, Reacts::Any);
    assert!(adds(&mut app));
    // Timed out: none, and a click on a reaction does nothing.
    allow(&mut app, Reacts::None);
    assert!(!adds(&mut app));
    interact(&mut app, |ui| {
        let _ = ui.click("wave");
    });
    assert!(!mine(&app));
    // Without Add Reactions: those there already, and no new one.
    allow(&mut app, Reacts::Existing);
    assert!(!adds(&mut app), "no new reaction");
    interact(&mut app, |ui| {
        ui.click("wave").expect("the reaction");
    });
    assert!(mine(&app), "a reaction that's there takes this account's");
}

#[test]
fn many_reactions_go_onto_more_lines() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let reactions = |count: usize| {
        (0..count)
            .map(|index| yapfast_core::chat::Reaction {
                key: format!("reaction{index}"),
                count: 1,
                mine: false,
            })
            .collect::<Vec<_>>()
    };
    let places = |app: &mut crate::app::App, keys: [&str; 2]| {
        let mut found = [iced::Point::ORIGIN; 2];
        interact(app, |ui| {
            for (place, key) in found.iter_mut().zip(keys) {
                let chip = ui.find(key).expect("the reaction");
                *place = chip.visible_bounds().expect("in view").position();
            }
        });
        found
    };
    react_to(&mut app, text, reactions(2));
    let [first, second] = places(&mut app, ["reaction0", "reaction1"]);
    assert!(
        second.y == first.y && second.x > first.x,
        "two share a line"
    );
    react_to(&mut app, text, reactions(30));
    let [first, last] = places(&mut app, ["reaction0", "reaction29"]);
    assert!(last.y > first.y, "thirty don't fit one");
    assert!(
        last.x < app.window.width,
        "none runs past the window's edge"
    );
}

#[test]
fn the_typing_dots_hold_still_while_animations_dont_play() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.typing.insert(room, vec!["Sam".into()]);
    });
    // The dots' pixels, now and a moment later.
    let moved = |app: &mut crate::app::App| {
        let mut moved = false;
        interact(app, |ui| {
            let words = ui.find("Sam is typing...").expect("the typing line");
            let words = words.visible_bounds().expect("in view");
            let dots = |ui: &mut iced_test::Simulator<'_, Message>| {
                let (width, _, pixels) = drawn(ui);
                let rows = (words.y * 2.0) as usize..((words.y + words.height) * 2.0) as usize;
                let columns = ((words.x - 60.0) * 2.0) as usize..(words.x * 2.0) as usize;
                rows.flat_map(|y| columns.clone().map(move |x| (y * width as usize + x) * 4))
                    .map(|at| pixels[at..at + 4].to_vec())
                    .collect::<Vec<_>>()
            };
            let before = dots(ui);
            std::thread::sleep(std::time::Duration::from_millis(200));
            moved = dots(ui) != before;
        });
        moved
    };
    app.layout.prefs.animate = crate::layout::Animate::Always;
    assert!(moved(&mut app), "they rise in turn");
    app.layout.prefs.animate = crate::layout::Animate::Never;
    assert!(!moved(&mut app));
}

#[test]
fn a_right_click_on_a_link_adds_its_address_to_the_messages_menu() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let says = |app: &mut crate::app::App, body: &str| {
        let body = body.to_owned();
        change(app, |state| {
            let messages = state
                .messages
                .iter()
                .map(|message| {
                    let mut message = (**message).clone();
                    if message.author == "Riley" {
                        message.body = body.clone();
                        message.formatted = true;
                    }
                    Arc::new(message)
                })
                .collect();
            state.messages = Arc::new(messages);
        });
    };
    let link_rows = |app: &crate::app::App| {
        let menu = app.win.menu.as_ref().expect("the message's menu");
        let rows = menu.rows.iter().filter(|row| {
            matches!(
                row.act,
                crate::menus::MenuAct::CopyTextLink | crate::menus::MenuAct::OpenTextLink
            )
        });
        rows.map(|row| (row.label.to_string(), row.key.clone()))
            .collect::<Vec<_>>()
    };
    // Plain text: the message's own rows.
    interact(&mut app, |ui| right_click(ui, text));
    assert_eq!(link_rows(&app), []);
    let _ = app.update(Message::Menu(crate::menus::Event::Close));
    // A link that shows other words: where it goes, not what it says.
    says(&mut app, "[example.com](https://elsewhere.example/page)");
    // The text fills the row: the link is at its start.
    interact(&mut app, |ui| {
        let piece = ui.find("example.com").expect("the message");
        let piece = piece.visible_bounds().expect("in view");
        ui.point_at(iced::Point::new(piece.x + 20.0, piece.center_y()));
        ui.simulate([
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right,
            )),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Right,
            )),
        ]);
    });
    let address = "https://elsewhere.example/page".to_owned();
    assert_eq!(
        link_rows(&app),
        [
            ("Copy Link".to_owned(), address.clone()),
            ("Open Link".to_owned(), address),
        ]
    );
    // The next menu, asked for off the link, has none.
    let _ = app.update(Message::Menu(crate::menus::Event::Close));
    interact(&mut app, |ui| {
        right_click(ui, "Nice, let's get a game going.");
    });
    assert_eq!(link_rows(&app), []);
}

#[test]
fn only_a_web_page_has_an_address_to_copy() {
    use crate::menus::web_address;
    let page = "https://example.com/a";
    assert_eq!(web_address(page), Some(page));
    let masked = format!("{}{page}", yapfast_core::links::LEAVE);
    assert_eq!(web_address(&masked), Some(page));
    assert_eq!(web_address("yapfast:user/1"), None);
    assert_eq!(web_address("https://matrix.to/#/@sam:example.org"), None);
    assert_eq!(web_address("file:///etc/passwd"), None);
}

/// Changes the message reading `body` with `alter`.
fn alter(
    app: &mut crate::app::App,
    body: &str,
    alter: impl Fn(&mut yapfast_core::chat::ChatMessage),
) {
    change(app, |state| {
        let messages = state
            .messages
            .iter()
            .map(|message| {
                let mut message = (**message).clone();
                if message.body == body {
                    alter(&mut message);
                }
                Arc::new(message)
            })
            .collect();
        state.messages = Arc::new(messages);
    });
}

#[test]
fn an_apps_answer_still_to_come_says_it_is_thinking_then_that_it_did_not_respond() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let now = super::now_ms() / 1000;
    alter(&mut app, text, |message| {
        message.flags |= crate::message::LOADING;
        message.timestamp = now;
    });
    assert!(shows(&app, "Riley is thinking…"));
    assert!(!shows(&app, "The application did not respond"));
    alter(&mut app, text, |message| message.timestamp = now - 16 * 60);
    assert!(shows(&app, "The application did not respond"));
    assert!(!shows(&app, "Riley is thinking…"));
}

/// Where the author line of the message reading `body` names `author`:
/// the name above the body, starting where it starts.
fn author_name(
    ui: &mut iced_test::Simulator<'_, Message>,
    body: &str,
    author: &str,
) -> iced::Rectangle {
    use iced_test::selector::Candidate;
    let body = ui.find(body).expect("the message");
    let body = body.visible_bounds().expect("in view");
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content == author
            && bounds.y < body.y
            && (bounds.x - body.x).abs() < 4.0
        {
            found = Some(bounds);
        }
        None::<()>
    });
    found.expect("the author's name")
}

/// A right click where the pointer is.
fn right_click_at(ui: &mut iced_test::Simulator<'_, Message>, at: iced::Point) {
    use iced::mouse;
    ui.point_at(at);
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
}

#[test]
fn a_right_click_on_an_authors_avatar_or_name_opens_their_menu() {
    use crate::menus::MenuAct;
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let persons = |app: &crate::app::App| {
        let menu = app.win.menu.as_ref().expect("a menu");
        let person = menu
            .rows
            .iter()
            .any(|row| matches!(row.act, MenuAct::Person(_)));
        (person, menu.item.is_some())
    };
    interact(&mut app, |ui| {
        let name = author_name(ui, text, "Riley");
        right_click_at(ui, name.center());
    });
    assert_eq!(
        persons(&app),
        (true, false),
        "Riley's menu, not the message's"
    );
    assert!(shows(&app, "Profile"));
    let _ = app.update(Message::Menu(crate::menus::Event::Close));
    interact(&mut app, |ui| {
        let name = author_name(ui, text, "Riley");
        // The avatar, left of the name and the body.
        let avatar = iced::Point::new(name.x - 16.0 - 20.0, name.y + 18.0);
        right_click_at(ui, avatar);
    });
    assert_eq!(
        persons(&app),
        (true, false),
        "Riley's menu, not the message's"
    );
    let _ = app.update(Message::Menu(crate::menus::Event::Close));
    // The body still opens the message's.
    interact(&mut app, |ui| right_click(ui, text));
    assert_eq!(persons(&app), (false, true));
}

#[test]
fn who_used_an_apps_command_opens_their_card_and_the_command_is_written_in_the_composer() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let sam = app
        .client
        .state()
        .messages
        .iter()
        .find(|message| message.author == "Sam")
        .expect("Sam's message")
        .sender
        .clone();
    alter(&mut app, text, |message| {
        message.used = Some(yapfast_core::chat::UsedCommand {
            user: "Sam Carter".into(),
            sender: sam.clone(),
            avatar: None,
            color: None,
            name: "/ping".into(),
            slash: Some("ping".into()),
            target: None,
        });
    });
    assert!(shows(&app, "used"));
    interact(&mut app, |ui| {
        ui.click("Sam Carter").expect("who used it");
    });
    assert_eq!(
        app.profiles.card.as_ref().map(|card| card.user.clone()),
        Some(sam)
    );
    escape(&mut app);
    assert!(app.profiles.card.is_none());
    interact(&mut app, |ui| {
        ui.click("/ping").expect("the command");
    });
    assert_eq!(app.client.state().draft(), "/ping");
    assert_eq!(app.composer.draft.text(), "/ping");
}

/// Escape, as the window's keys hand it to the app.
fn escape(app: &mut crate::app::App) {
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        Default::default(),
    ));
}

/// What each button of the hover bar of the message `item` sends, left to
/// right, the bar drawn alone.
fn hover_bar_acts(app: &crate::app::App, item: &str) -> Vec<Message> {
    let index = app
        .timeline
        .messages
        .iter()
        .position(|message| message.item == item)
        .expect("the row");
    let mut acts = Vec::new();
    // Buttons 32 across in the bar's 3 px padding.
    for at in 0..8 {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            iced::Size::new(400.0, 40.0),
            crate::message::hover_bar::hover_bar(app.ctx(), index).expect("its bar"),
        );
        ui.point_at(iced::Point::new(3.0 + 16.0 + 32.0 * at as f32, 19.0));
        ui.simulate(iced_test::simulator::click());
        acts.extend(ui.into_messages().filter(|message| {
            !matches!(
                message,
                Message::Timeline(crate::timeline::Event::PointRow(..))
            )
        }));
    }
    acts
}

#[test]
fn shift_on_the_hover_bar_offers_create_thread_where_one_may_start() {
    use crate::menus::{Event, MenuAct};
    use yapfast_core::{
        application::{Action, Navigation},
        yapfast_discord::ready::permission,
    };
    let (mut app, _folder) = super::server::discord_server(
        permission::READ_MESSAGE_HISTORY | permission::CREATE_PUBLIC_THREADS,
        |_, _| {},
    );
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.client
        .dispatch(Action::SelectRoom("discord:10:11".into()));
    assert!(app.client.app.can_start_threads());
    let message = |thread| {
        Arc::new(yapfast_core::chat::ChatMessage {
            item: "d:50".into(),
            id: "50".into(),
            author: "Sam".into(),
            body: "Let's talk about it".into(),
            timestamp: 1,
            can_reply: true,
            thread,
            ..Default::default()
        })
    };
    change(&mut app, |state| {
        state.messages = Arc::new(vec![message(None)]);
    });
    let _ = app.update(Message::Modifiers(keyboard::Modifiers::SHIFT));
    let starts = |app: &crate::app::App| {
        hover_bar_acts(app, "d:50").iter().any(|message| {
            matches!(message, Message::Menu(Event::Act(MenuAct::NewThread, item, _)) if item == "d:50")
        })
    };
    assert!(starts(&app), "Create Thread under Shift");
    // Not without Shift.
    let _ = app.update(Message::Modifiers(keyboard::Modifiers::empty()));
    assert!(!starts(&app));
    // Nor on a message that has a thread already.
    let _ = app.update(Message::Modifiers(keyboard::Modifiers::SHIFT));
    change(&mut app, |state| {
        state.messages = Arc::new(vec![message(Some(yapfast_core::chat::ThreadLink {
            room: "discord:10:60".into(),
            name: "About it".into(),
            messages: None,
            archived: false,
        }))]);
    });
    assert!(!starts(&app), "the message has its thread");
}

#[test]
fn apps_are_left_out_of_the_menu_of_an_answer_only_this_account_sees_or_still_to_come() {
    use yapfast_core::{
        application::{Action, Navigation},
        chat::{ChatMessage, CommandIndex, Event},
    };
    let general = "discord:10:11";
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.client.dispatch(Action::SelectRoom(general.into()));
    assert_eq!(app.client.state().selected.as_deref(), Some(general));
    let said = |item: &str, flags| {
        Arc::new(ChatMessage {
            item: item.into(),
            author: "Sam".into(),
            body: format!("Message {item}"),
            timestamp: 1,
            can_reply: true,
            flags,
            ..Default::default()
        })
    };
    change(&mut app, |state| {
        state.messages = Arc::new(vec![
            said("d:1", 0),
            said("d:2", crate::message::EPHEMERAL),
            said("d:3", crate::message::LOADING),
        ]);
    });
    // The room's commands, asked for as a menu opening asks.
    app.client.dispatch(Action::AskAppCommands);
    let notes = CommandIndex::parse(
        &serde_json::json!({
            "applications": [{"id": "8", "name": "Notes", "flags": "0"}],
            "application_commands": [
                {"id": "4", "type": 3, "application_id": "8", "name": "Save note",
                 "description": "", "integration_types": [1], "contexts": [0, 1, 2],
                 "version": "1"}],
        }),
        true,
    );
    app.client.apply(Event::CommandIndex {
        room: general.into(),
        index: Ok(Arc::new(notes)),
    });
    let _ = app.sync();
    let apps = |app: &mut crate::app::App, index| {
        let _ = crate::menus::open_menu(app, index, iced::Point::new(400.0, 300.0));
        let menu = app.win.menu.as_ref().expect("the menu");
        let apps = menu.rows.iter().any(|row| row.label == "Apps");
        let due = menu.apps_due;
        let _ = app.update(Message::Menu(crate::menus::Event::Close));
        (apps, due)
    };
    assert_eq!(apps(&mut app, 0), (true, false));
    assert_eq!(
        apps(&mut app, 1),
        (false, false),
        "only this account sees it"
    );
    assert_eq!(apps(&mut app, 2), (false, false), "still to come");
}

#[test]
fn the_hover_bar_stays_while_the_menu_it_opened_is_open() {
    let (mut app, _folder) = preview();
    let row = |app: &crate::app::App, body: &str| {
        app.timeline
            .messages
            .iter()
            .position(|message| message.body == body)
            .expect("the row")
    };
    let text = "Yep. I should be free after dinner.";
    let ours = row(&app, text);
    let other = row(&app, "Nice, let's get a game going.");
    // The corner of our row its bar floats over, by its author line.
    let (list, name) = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        (timeline_box(&mut ui), author_name(&mut ui, text, "Riley"))
    };
    let corner = iced::Rectangle::new(
        iced::Point::new(list.x + list.width - 16.0 - 120.0, name.y - 12.0),
        iced::Size::new(110.0, 24.0),
    );
    // A menu opened in the same place, of our message or the other's, with
    // the pointer off both.
    let corner_drawn = |app: &mut crate::app::App, index| {
        let _ = crate::menus::open_menu(app, index, iced::Point::new(list.x + 20.0, 60.0));
        assert!(app.pointed.message.is_none());
        let (width, _, pixels) = drawn(&mut iced_test::Simulator::with_size(
            iced::Settings::default(),
            app.shell.frame,
            app.view(),
        ));
        let _ = app.update(Message::Menu(crate::menus::Event::Close));
        let scale = 2.0;
        let rows = (corner.y * scale) as usize..((corner.y + corner.height) * scale) as usize;
        let columns = (corner.x * scale) as usize..((corner.x + corner.width) * scale) as usize;
        rows.flat_map(|y| columns.clone().map(move |x| (y * width as usize + x) * 4))
            .flat_map(|at| pixels[at..at + 4].to_vec())
            .collect::<Vec<u8>>()
    };
    let kept = corner_drawn(&mut app, ours);
    let elsewhere = corner_drawn(&mut app, other);
    assert_ne!(
        kept, elsewhere,
        "our message's bar shows with its menu open"
    );
}

#[test]
fn the_message_the_keys_are_on_has_a_marigold_edge() {
    let (mut app, _folder) = preview();
    let _ = crate::timeline::keys::step_into_messages(&mut app);
    let item = app.keyed.clone().expect("stepping");
    let body = app
        .timeline
        .messages
        .last()
        .expect("a message")
        .body
        .clone();
    // Down the row's left side, beside its text.
    let (x, y) = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        let list = timeline_box(&mut ui);
        let text = ui.find(body.as_str()).expect("the newest message");
        (
            list.x + 1.0,
            text.visible_bounds().expect("in view").center_y(),
        )
    };
    let accent = app.theme.accent;
    let at = |app: &crate::app::App| {
        let (width, _, pixels) = drawn(&mut iced_test::Simulator::with_size(
            iced::Settings::default(),
            app.shell.frame,
            app.view(),
        ));
        let at = ((y * 2.0) as usize * width as usize + (x * 2.0) as usize) * 4;
        pixels[at..at + 3].to_vec()
    };
    let near = |pixel: &[u8], color: iced::Color| {
        let [r, g, b, _] = color.into_rgba8();
        pixel.iter().zip([r, g, b]).all(|(a, b)| a.abs_diff(b) <= 8)
    };
    assert!(
        near(&at(&app), accent),
        "the edge, while the keys are on it"
    );
    // Lit by the pointer alone, it has none.
    app.keyed = None;
    assert_eq!(app.pointed.message.as_deref(), Some(&*item));
    assert!(!near(&at(&app), accent));
}

#[test]
fn the_authors_name_is_underlined_under_the_pointer() {
    let (app, _folder) = preview();
    let mut ui =
        iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    let name = author_name(&mut ui, "Yep. I should be free after dinner.", "Riley");
    let under = iced::Rectangle::new(
        iced::Point::new(name.x, name.y + name.height * 0.7),
        iced::Size::new(name.width, name.height * 0.3 + 2.0),
    );
    assert!(super::hover_draws(&mut ui, name.center(), under));
}

#[test]
fn a_conversation_still_loading_shows_rows_the_shape_of_messages() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.room_ready = false;
        state.messages = Arc::new(Vec::new());
    });
    assert!(!shows(&app, "Loading conversation..."));
    // Down the middle of the faces, the column of the timeline's left
    // edge, a face's bone: 40 px of the ground they're drawn in.
    let x = app.ctx().lane_place(0).0.left + 16.0 + 20.0;
    let (width, height, pixels) = drawn(&mut iced_test::Simulator::with_size(
        iced::Settings::default(),
        app.shell.frame,
        app.view(),
    ));
    let [r, g, b, _] = app.theme.raised.into_rgba8();
    let column = (x * 2.0) as usize;
    let (mut run, mut longest) = (0, 0);
    for y in 0..height as usize {
        let at = (y * width as usize + column) * 4;
        let bone = pixels[at..at + 3]
            .iter()
            .zip([r, g, b])
            .all(|(a, b)| a.abs_diff(b) <= 4);
        run = if bone { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    // Twice its size, as the simulator draws.
    assert!(longest >= 76, "a face's bone: {longest} px");
}

/// Where the timeline's list is in the window.
fn timeline_box(ui: &mut iced_test::Simulator<'_, Message>) -> iced::Rectangle {
    use iced_test::selector::Candidate;
    ui.find(|candidate: Candidate<'_>| match candidate {
        Candidate::Scrollable {
            id: Some(id),
            visible_bounds: Some(bounds),
            ..
        } if *id == Id::new(crate::timeline::TIMELINE) => Some(bounds),
        _ => None,
    })
    .expect("the timeline")
}

#[test]
fn a_messages_menu_opens_at_the_pointer_under_whatever_is_over_the_list() {
    // The preview's banner is over its timeline.
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    let mut pressed = iced::Point::ORIGIN;
    interact(&mut app, |ui| {
        let body = ui.find(text).expect("the message");
        pressed = body.visible_bounds().expect("in view").center();
        right_click(ui, text);
    });
    let menu = app.win.menu.as_ref().expect("the message's menu");
    assert!(
        (menu.at.x - pressed.x).abs() < 1.0 && (menu.at.y - pressed.y).abs() < 1.0,
        "the menu at {:?}, the press at {pressed:?}",
        menu.at
    );
}

#[test]
fn a_press_off_the_messages_takes_the_keys_from_them() {
    let (mut app, _folder) = preview();
    let app_height = app.shell.frame.height;
    let _ = crate::timeline::keys::step_into_messages(&mut app);
    assert!(app.keyed.is_some());
    // A press on a message keeps them there.
    interact(&mut app, |ui| {
        right_click(ui, "Yep. I should be free after dinner.");
    });
    assert!(app.win.menu.is_some(), "the message's menu");
    assert!(app.keyed.is_some(), "a press on the list keeps the keys");
    let _ = app.update(Message::Menu(crate::menus::Event::Close));
    // One on the channel list doesn't.
    interact(&mut app, |ui| {
        ui.click("TEXT CHANNELS").expect("the heading");
    });
    assert!(app.keyed.is_none(), "a press elsewhere takes them");
    // Nor does one on what sends nothing of its own: the window's ground.
    let _ = crate::timeline::keys::step_into_messages(&mut app);
    interact(&mut app, |ui| {
        ui.point_at(iced::Point::new(150.0, app_height - 120.0));
        ui.simulate(iced_test::simulator::click());
    });
    assert!(
        app.keyed.is_none(),
        "a press that sends nothing takes them too"
    );
}

#[test]
fn an_animated_custom_emoji_plays_in_text_and_its_reaction_with_one_player_for_its_places() {
    use yapfast_core::chat::{ChatMessage, CustomEmoji, Picture, Pixels, Reaction};
    let (mut app, _folder) = preview();
    let party = super::animated_emoji();
    let calm = Arc::new(Picture {
        width: 64,
        height: 64,
        rgba: Pixels::new(64, 64, vec![200; 64 * 64 * 4]),
        ..Default::default()
    });
    let emoji = vec![
        CustomEmoji {
            text: ":party:".into(),
            id: "1".into(),
            animated: true,
            usable: true,
            picture: "https://cdn/emojis/1.gif?size=64".into(),
        },
        CustomEmoji {
            text: ":calm:".into(),
            id: "2".into(),
            animated: false,
            usable: true,
            picture: "https://cdn/emojis/2.png?size=64".into(),
        },
    ];
    let message = ChatMessage {
        item: "party".into(),
        author: "Sam".into(),
        body: "Party :party: :party: :party: and :calm:".into(),
        formatted: true,
        emoji: emoji.clone(),
        reactions: vec![
            Reaction {
                key: ":party:".into(),
                count: 2,
                mine: false,
            },
            Reaction {
                key: ":calm:".into(),
                count: 1,
                mine: false,
            },
        ],
        ..Default::default()
    };
    change(&mut app, |state| {
        state
            .pictures
            .insert(emoji[0].picture.clone(), party.clone());
        state
            .pictures
            .insert(emoji[1].picture.clone(), calm.clone());
        state.messages = Arc::new(vec![Arc::new(message)]);
    });
    app.layout.prefs.animate = crate::layout::Animate::Always;
    // The sizes the party plays at, and whether the calm one plays at all.
    let playing = |app: &crate::app::App| {
        super::redraw(app);
        let keys = app.win.pictures.players.keys();
        let at = |picture: &Arc<Picture>| Arc::as_ptr(picture) as usize;
        let mut sizes: Vec<(u32, u32)> = keys
            .iter()
            .filter(|key| key.0 == at(&party))
            .map(|key| (key.1, key.2))
            .collect();
        sizes.sort_unstable();
        (sizes, keys.iter().any(|key| key.0 == at(&calm)))
    };
    let (sizes, calm_plays) = playing(&app);
    // The text's three share one player; the chip, smaller, has its own.
    assert_eq!(sizes.len(), 2, "the text's and the chip's: {sizes:?}");
    assert!(sizes[0].0 < sizes[1].0, "the chip's is the smaller");
    assert!(!calm_plays, "a still emoji has no player");
    // Animate's Never holds them still, and their players go.
    app.layout.prefs.animate = crate::layout::Animate::Never;
    assert_eq!(playing(&app), (Vec::new(), false));
    app.layout.prefs.animate = crate::layout::Animate::Always;
    assert_eq!(playing(&app).0.len(), 2, "playing again");
    // Far above the newest once many more come, its row isn't built, and
    // its players go.
    change(&mut app, |state| {
        let mut messages = (*state.messages).clone();
        messages.extend((0..80).map(|index| {
            Arc::new(ChatMessage {
                item: format!("more-{index}"),
                author: "Sam".into(),
                body: format!("Message number {index}"),
                ..Default::default()
            })
        }));
        state.messages = Arc::new(messages);
    });
    assert_eq!(playing(&app), (Vec::new(), false));
}

#[test]
fn reply_quotes_render_static_and_animated_custom_emoji_and_fall_back_until_loaded() {
    use iced::Settings;
    use iced_test::{Simulator, selector::Candidate};
    use yapfast_core::chat::{ChatMessage, CustomEmoji, Picture, Pixels, Reply};

    let (mut app, _folder) = preview();
    let party = super::animated_emoji();
    // Bright magenta static emoji so rendered pixels differ unmistakably from background.
    let calm_rgba: Vec<u8> = [255u8, 0, 255, 255].repeat(64 * 64);
    let calm = Arc::new(Picture {
        width: 64,
        height: 64,
        rgba: Pixels::new(64, 64, calm_rgba),
        ..Default::default()
    });
    let emoji = vec![
        CustomEmoji {
            text: ":AMkiss:".into(),
            id: "1".into(),
            animated: false,
            usable: true,
            picture: "https://cdn/emojis/1.png?size=64".into(),
        },
        CustomEmoji {
            text: ":party:".into(),
            id: "2".into(),
            animated: true,
            usable: true,
            picture: "https://cdn/emojis/2.gif?size=64".into(),
        },
    ];
    let message = ChatMessage {
        item: "reply-target".into(),
        author: "Kitty".into(),
        body: "Looking at that reply".into(),
        reply: Some(Reply {
            author: "desu".into(),
            body: "hi :AMkiss: and :party:!".into(),
            emoji: emoji.clone(),
            item: Some("orig".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    change(&mut app, |state| {
        state.messages = Arc::new(vec![Arc::new(message)]);
    });

    // Until the emoji pictures arrive, the quote falls back cleanly to `:name:` text.
    assert!(
        shows(&app, "hi :AMkiss: and :party:!"),
        "falls back to :name: text before pictures arrive"
    );

    // Once pictures arrive, the quote replaces `:name:` with inline emoji pictures
    // and requests the animated player at the compact reply size.
    change(&mut app, |state| {
        state
            .pictures
            .insert(emoji[0].picture.clone(), calm.clone());
        state
            .pictures
            .insert(emoji[1].picture.clone(), party.clone());
    });
    assert!(
        !shows(&app, "hi :AMkiss: and :party:!"),
        "literal :name: replaced once emoji pictures load"
    );
    assert!(
        shows(&app, "hi \u{2003} and \u{2003}!"),
        "inline emoji placeholders stay on one line with surrounding text"
    );
    app.layout.prefs.animate = crate::layout::Animate::Always;
    super::redraw(&app);
    let keys = app.win.pictures.players.keys();
    let party_at = Arc::as_ptr(&party) as usize;
    let calm_at = Arc::as_ptr(&calm) as usize;
    let party_sizes: Vec<(u32, u32)> = keys
        .iter()
        .filter(|key| key.0 == party_at)
        .map(|key| (key.1, key.2))
        .collect();
    assert_eq!(party_sizes, vec![(16, 16)]);
    assert!(
        !keys.iter().any(|key| key.0 == calm_at),
        "static reply emoji has no player"
    );

    // Single-line layout stays within HEADER_LINE (22px) even with emojis and long text,
    // and draws the emoji image in both dark and light themes.
    for theme_cmd in [None, Some("theme sakura")] {
        if let Some(cmd) = theme_cmd {
            let _ = app.update(Message::Control(cmd.into()));
        }
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        let reply_bounds = ui
            .find(|candidate: Candidate<'_>| match candidate {
                Candidate::Text {
                    content: "hi \u{2003} and \u{2003}!",
                    visible_bounds: Some(bounds),
                    ..
                } => Some(bounds),
                _ => None,
            })
            .expect("reply text bounds");
        assert!(
            reply_bounds.height <= 22.0,
            "reply quote stays single-line within HEADER_LINE: {}",
            reply_bounds.height
        );
        let (width, height, rgba) = drawn(&mut ui);
        let scale = 2.0;
        let rx0 = (reply_bounds.x * scale) as usize;
        let ry0 = (reply_bounds.y * scale) as usize;
        let rx1 = ((reply_bounds.x + reply_bounds.width) * scale).ceil() as usize;
        let ry1 = ((reply_bounds.y + reply_bounds.height) * scale).ceil() as usize;
        let has_magenta = (ry0..ry1.min(height as usize)).any(|y| {
            (rx0..rx1.min(width as usize)).any(|x| {
                let at = (y * width as usize + x) * 4;
                rgba[at] > 200 && rgba[at + 1] < 40 && rgba[at + 2] > 160
            })
        });
        assert!(
            has_magenta,
            "static reply emoji renders its pixels in theme {}",
            app.theme.id
        );
    }

    // Replying to a message with custom emoji also renders in the composer reply bar.
    let _ = app.update(Message::Control("theme bonfire".into()));
    change(&mut app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "original-with-emoji".into(),
            author: "desu".into(),
            body: "hey :AMkiss:".into(),
            formatted: true,
            emoji: vec![emoji[0].clone()],
            can_reply: true,
            ..Default::default()
        })]);
    });
    interact(&mut app, |ui| right_click(ui, "hey \u{2003}"));
    interact(&mut app, |ui| {
        ui.click("Reply").expect("Reply");
    });
    assert!(
        shows(&app, "Replying to desu: hey \u{2003}"),
        "composer reply bar renders inline custom emoji placeholder"
    );
}
