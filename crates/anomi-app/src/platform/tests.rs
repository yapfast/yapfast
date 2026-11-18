//! The window's title and bar, and closing to the tray.

use std::sync::Arc;

use iced::window;
use yapfast_core::application::{Action, Navigation};

use anomi_vm::shell::Layer;

use super::window::{framed, own_bar, title};
use crate::app::App;

fn app(name: &str) -> App {
    let folder = std::env::temp_dir().join(format!("anomi-platform-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    drop(app.dispatch(Action::Preview));
    app
}

#[test]
fn the_title_counts_the_mentions_waiting() {
    let mut shown = String::new();
    title(&mut shown, 0);
    assert_eq!(shown, yapfast_lang::PRODUCT);
    title(&mut shown, 3);
    assert_eq!(shown, format!("(3) {}", yapfast_lang::PRODUCT));
    title(&mut shown, 150);
    assert_eq!(shown, format!("(99+) {}", yapfast_lang::PRODUCT));

    let mut app = app("title");
    let room = Arc::make_mut(&mut app.core.state.rooms[0]);
    room.mentions = 2;
    app.refresh();
    assert_eq!(app.desktop.title, format!("(2) {}", yapfast_lang::PRODUCT));
}

#[test]
fn the_bar_is_anomis_where_the_desktop_gives_windows_one() {
    assert!(framed("ubuntu:GNOME") && framed("kde"));
    assert!(!framed("Hyprland") && !framed("sway") && !framed(""));
    let mut app = app("bar");
    app.layout.edit(|saved| saved.system_title_bar = Some(true));
    assert!(!own_bar(&app.layout));
    app.layout
        .edit(|saved| saved.system_title_bar = Some(false));
    assert_eq!(
        own_bar(&app.layout),
        cfg!(any(target_os = "linux", windows))
    );
}

#[test]
fn the_bar_says_where_the_person_is() {
    let mut app = app("chrome");
    app.refresh();
    let chrome = &app.vm.chrome;
    assert!(chrome.workspace);
    let server = app.core.state.servers[0].clone();
    drop(app.dispatch(Action::Navigate(Navigation::Server(server.id.clone()))));
    assert_eq!(app.vm.chrome.place, server.name);
    let rooms: Vec<String> = app
        .core
        .state
        .rooms
        .iter()
        .take(2)
        .map(|room| room.id.clone())
        .collect();
    for room in rooms {
        drop(app.dispatch(Action::Open(room)));
    }
    assert!(app.vm.chrome.can_go_back, "a room came before");
    assert!(!app.vm.chrome.can_go_forward);
    app.vm.shell.open(Layer::Settings);
    app.refresh();
    assert!(!app.vm.chrome.can_go_back, "Settings cover the arrows");
    drop(app.dispatch(Action::Navigate(Navigation::DirectMessages)));
    assert_eq!(app.vm.chrome.place, yapfast_lang::tr!("Direct Messages"));
}

#[test]
fn closed_to_the_tray_nothing_shows_until_the_window_does() {
    let mut app = app("tray");
    app.window = Some(window::Id::unique());
    let first = app.core.state.rooms[0].id.clone();
    drop(app.dispatch(Action::Open(first)));
    assert!(app.vm.chat.timeline.room.is_some());
    app.vm.shell.open(Layer::Pins);
    app.vm.shell.open(Layer::Status);
    drop(super::forget::window(&mut app));
    assert!(app.window.is_none() && app.hidden);
    assert!(
        !app.vm.shell.is_open(Layer::Pins),
        "a popover goes with the window"
    );
    assert!(
        app.vm.shell.is_open(Layer::Status),
        "a dialog stays for the next"
    );
    assert!(app.vm.chat.timeline.rows.is_empty());
    assert!(!app.motion.front);
    // The core goes on; the window's rows wait for the window.
    let room = app.core.state.rooms[1].id.clone();
    drop(app.dispatch(Action::Open(room.clone())));
    assert!(app.vm.chat.timeline.room.is_none());
    drop(super::window::open(&mut app, None));
    assert!(app.window.is_some() && !app.hidden);
    assert_eq!(
        app.vm.chat.timeline.room.as_ref().map(|room| &room.id),
        Some(&room)
    );
}
