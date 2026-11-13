use super::*;
use crate::{application::preview::preview_room, chat::*};

/// An application showing a server's channel, connected to Discord.
fn in_a_channel() -> Application {
    let mut app = Application::default();
    app.state.discord_online = true;
    let room = ChatRoom {
        id: crate::discord::room_id("1", "2"),
        ..preview_room("x", "general", false, 3)
    };
    app.state.selected = Some(room.id.clone());
    app.state.rooms = [Arc::new(room)].into();
    app
}

fn asks_for_feed(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::ActivityFeed))
        )
    })
}

fn entry(author: &str) -> FeedEntry {
    FeedEntry {
        author: author.into(),
        kind: yapfast_discord::activity_feed::Kind::Game,
        title: "Factorio".into(),
        picture: None,
        live: true,
        expires_ms: None,
        at_ms: 0,
        hours: None,
        tracks: None,
    }
}

#[test]
fn asks_once_until_the_feed_is_due_again() {
    let mut app = in_a_channel();
    assert!(asks_for_feed(&app.ask_activity()));
    assert!(!asks_for_feed(&app.ask_activity()), "one is on its way");
    assert!(!asks_for_feed(&app.show_activity_feed(Arc::default())));
    assert!(!asks_for_feed(&app.ask_activity()), "not due yet");
    assert!(asks_for_feed(&app.activity_feed_due()));
}

#[test]
fn waits_while_it_could_not_be_seen() {
    let mut app = in_a_channel();
    app.window_focused(false);
    assert!(!asks_for_feed(&app.ask_activity()), "out of focus");
    assert!(asks_for_feed(&app.window_focused(true)));
    app.show_activity_feed(Arc::default());

    let long_ago = crate::clock::now_ms() - FEED_IDLE_MS - 1;
    app.feed_quiet_since(Some(long_ago));
    assert!(!asks_for_feed(&app.activity_feed_due()), "long idle");
    assert!(asks_for_feed(&app.feed_quiet_since(None)));
    app.show_activity_feed(Arc::default());

    app.state.discord_online = false;
    assert!(!asks_for_feed(&app.activity_feed_due()), "disconnected");
    app.state.discord_online = true;
    assert!(asks_for_feed(&app.ask_activity()));
    app.show_activity_feed(Arc::default());

    app.set_activity_cards_hidden(true);
    assert!(
        !asks_for_feed(&app.activity_feed_due()),
        "hidden, feed held"
    );
    assert!(asks_for_feed(&app.set_activity_cards_hidden(false)));
}

#[test]
fn hidden_cards_still_fetch_a_first_feed_for_their_count() {
    let mut app = in_a_channel();
    app.set_activity_cards_hidden(true);
    assert!(asks_for_feed(&app.ask_activity()));
}

#[test]
fn only_a_server_channel_asks() {
    let mut app = in_a_channel();
    let dm = ChatRoom {
        id: crate::discord::room_id("@me", "3"),
        direct: true,
        ..preview_room("x", "Bob", true, 3)
    };
    app.state.selected = Some(dm.id.clone());
    app.state.rooms = [Arc::new(dm)].into();
    assert!(app.ask_activity().is_empty());
}

#[test]
fn asks_who_may_show_once_per_room_and_feed() {
    let mut app = in_a_channel();
    app.ask_activity();
    let entries: Arc<[FeedEntry]> = vec![entry("7"), entry("5"), entry("7")].into();
    let effects = app.show_activity_feed(entries.clone());
    let [Effect::Backend(Command::Discord(DiscordCommand::ActivityPeople { room, users }))] =
        &effects[..]
    else {
        panic!("should ask who may show");
    };
    assert_eq!(*room, crate::discord::room_id("1", "2"));
    let users: Vec<&str> = users.iter().map(|user| &**user).collect();
    assert_eq!(users, ["5", "7"]);
    assert!(app.ask_activity().is_empty(), "asked already");

    app.activity_feed_due();
    assert_eq!(
        app.show_activity_feed(entries).len(),
        1,
        "a new feed asks again"
    );
}

#[test]
fn an_answer_for_another_room_or_account_is_dropped() {
    let mut app = in_a_channel();
    app.show_activity_people(crate::discord::room_id("1", "9"), Arc::default());
    assert!(app.state.activity_people.is_none());
    app.show_activity_people(crate::discord::room_id("1", "2"), Arc::default());
    assert!(app.state.activity_people.is_some());

    assert!(asks_for_feed(&app.ask_activity()));
    app.forget_activity_feed();
    assert!(app.state.activity_people.is_none());
    app.show_activity_feed(vec![entry("7")].into());
    assert!(app.state.activity_feed.is_empty(), "the old account's");
}
