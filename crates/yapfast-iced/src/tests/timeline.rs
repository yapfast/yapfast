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
    at_top, change, interact, long_history, numbered, present, preview, right_click, scroll_up,
    shows, starting,
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
    interact(&mut app, |ui| {
        let bar = starting(ui, "70 new messages since ").expect("the bar counts what's new");
        ui.point_at(bar);
        ui.simulate(click());
    });
    let _ = app.sync();
    // The rows round it are built; the scroll there is a task, which
    // doesn't run here, so the list itself stays where it was.
    assert!(present(&app, "Message number 10"), "the view went to it");
    assert!(present(&app, "New"));
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
