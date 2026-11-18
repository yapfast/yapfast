//! The clocks: their waits run in front while something shows the time
//! and go behind another window and in the tray; a tick writes again only
//! what shows the time; nobody using the window for 15 s makes it idle.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use iced::mouse;
use yapfast_core::application::Action;
use yapfast_core::clock::now_ms;

use anomi_vm::sidebar::Place;

use super::away::{self, LAST_USE, QUIET};
use super::{Tick, arm, update};
use crate::app::App;
use crate::message::Message;
use crate::tests::{Window, start};

/// Which waits run once the clocks were set: the minute's, the day's and
/// the second's.
fn running(app: &mut App) -> [bool; 3] {
    drop(arm(app));
    app.clocks.running()
}

/// The preview in front, its voice channel "lounge" in a call for ten
/// minutes.
fn busy(name: &str) -> Window {
    busy_since(name, now_ms() / 1000 - 600)
}

fn busy_since(name: &str, started: u64) -> Window {
    let mut window = start(name, true);
    let app = &mut window.app;
    let state = &mut app.core.state;
    let lounge = state.rooms.iter_mut().find(|room| room.id == "lounge");
    Arc::make_mut(lounge.expect("the preview's voice channel")).voice_started = Some(started);
    state.list_changes += 1;
    app.refresh();
    window
}

/// The call's length on the lounge's row.
fn lasted(app: &App) -> &str {
    let Place::Server(server) = &app.vm.sidebar.place else {
        panic!("a server's channels");
    };
    let row = server.channels.rows.iter().find(|row| row.id == "lounge");
    &row.expect("the lounge's row").lasted
}

#[test]
fn the_clocks_run_only_in_front_while_something_shows_the_time() {
    let mut window = start("clocks-quiet", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    assert!(running(app)[1], "the timeline's day");
    drop(app.update(Message::Focused(false)));
    assert!(!app.clocks.running()[1], "behind another window");
    let mut window = busy("clocks-front");
    let app = &mut window.app;
    assert!(app.shown.sidebar.timed, "the call's length shows");
    assert!(running(app)[0]);
    drop(app.update(Message::Focused(false)));
    assert_eq!(app.clocks.running(), [false; 3], "behind another window");
    drop(app.update(Message::Focused(true)));
    assert!(app.clocks.running()[0], "in front again");
    app.hidden = true;
    assert_eq!(running(app), [false; 3], "in the tray");
}

#[test]
fn a_minute_writes_again_only_the_words_that_changed() {
    let started = now_ms() / 1000 - 600;
    let mut window = busy_since("clocks-minute", started);
    let app = &mut window.app;
    // As the row says it now, the minute turned or not.
    let now = || yapfast_core::clock::lasted(started, now_ms() / 60_000 * 60);
    drop(arm(app));
    assert_eq!(lasted(app), now());
    let due = app.clocks.minute.as_ref().expect("the minute's wait").due;
    assert_eq!(due % 60_000, 0, "a whole minute");
    let Place::Server(server) = &mut app.vm.sidebar.place else {
        panic!("a server's channels");
    };
    let row = server
        .channels
        .rows
        .iter_mut()
        .find(|row| row.id == "lounge");
    row.expect("the lounge's row").lasted = "stale".into();
    let version = app.vm.sidebar.version;
    drop(app.update(Message::Clock(Tick::Minute(due + 1))));
    assert_eq!(lasted(app), "stale", "a wait that was set again since");
    drop(app.update(Message::Clock(Tick::Minute(due))));
    assert_eq!(lasted(app), now());
    assert_eq!(app.vm.sidebar.version, version + 1);
    assert!(app.clocks.running()[0], "set again for the next minute");
    drop(app.update(Message::Clock(Tick::Minute(due))));
    assert_eq!(app.vm.sidebar.version, version + 1, "the same minute");
}

#[test]
fn nobody_using_the_window_for_15_s_makes_it_idle() {
    let mut window = start("clocks-away", true);
    let app = &mut window.app;
    let check = Message::Clock(Tick::Away(away::Event::Check));
    let click = iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let passing = iced::Event::Mouse(mouse::Event::CursorLeft);
    assert!(
        away::input(&click).is_none(),
        "input while in use costs no message"
    );
    drop(app.update(check.clone()));
    assert!(!app.motion.idle, "used just now");
    let quiet = u64::try_from(QUIET.as_millis()).unwrap_or_default();
    LAST_USE.store(now_ms() - quiet, Ordering::Relaxed);
    drop(app.update(check));
    assert!(app.motion.idle && app.clocks.away.inactive);
    assert!(
        away::input(&passing).is_none(),
        "a passing pointer isn't use"
    );
    assert!(matches!(
        away::input(&click),
        Some(Message::Clock(Tick::Away(away::Event::Used)))
    ));
    drop(update(app, Tick::Away(away::Event::Used)));
    assert!(!app.motion.idle && !app.clocks.away.inactive);
    assert!(away::input(&click).is_none(), "in use again");
}
