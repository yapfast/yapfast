//! The window's focus as the core hears it.

use std::sync::Arc;

use yapfast_core::application::Effect;
use yapfast_core::chat::{Command, DiscordCommand, Event};

use crate::app::App;
use crate::message::Message;

/// Whether `effects` ask Discord for the Activity panel's feed.
fn asks_for_feed(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::ActivityFeed))
        )
    })
}

/// The feed is due again, as Discord's timer says: whether the core asks.
fn asks_when_due(app: &mut App) -> bool {
    asks_for_feed(&app.core.apply(Event::ActivityFeedDue))
}

/// The feed asked for came: whether the core asks again, as it does when
/// the feed fell due meanwhile. One that wasn't asked for is dropped.
fn feed_came(app: &mut App) -> bool {
    asks_for_feed(&app.core.apply(Event::ActivityFeed(Arc::default())))
}

#[test]
fn the_core_hears_whether_the_window_is_in_front() {
    let folder = std::env::temp_dir().join(format!("anomi-focus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    // A Discord server's channel, online: the Activity panel's feed is due.
    drop(app.dispatch(yapfast_core::application::Action::Preview));
    let state = &mut app.core.state;
    let mut room = (*state.rooms[0]).clone();
    room.id = yapfast_core::discord::room_id("1", "2");
    room.direct = false;
    room.forum = false;
    state.selected = Some(room.id.clone());
    state.rooms = vec![Arc::new(room)];
    state.preview = false;
    state.discord_online = true;

    assert!(!asks_when_due(&mut app), "behind until the window says so");
    drop(app.update(Message::Focused(true)));
    assert!(app.motion.front);
    // Coming to the front asked for the feed (of no backend here): it's on
    // its way, and as it comes the core asks again for the turn that fell
    // due meanwhile.
    assert!(!asks_when_due(&mut app), "one is on its way");
    assert!(feed_came(&mut app), "in front, it's asked when due");
    assert!(!feed_came(&mut app));

    drop(app.update(Message::Focused(false)));
    assert!(!app.motion.front);
    assert!(!asks_when_due(&mut app), "behind another window");
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
