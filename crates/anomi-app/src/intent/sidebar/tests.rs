//! The places' intents on an app without a window, on the core's preview
//! (server "friends": general, games, weekend, lounge).

use yapfast_core::application::{Action, Navigation};

use anomi_vm::rail;
use anomi_vm::sidebar::channels::{self, Arrange, Move};
use anomi_vm::sidebar::{Intent, Place};

use crate::app::App;

fn app(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-places-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    drop(app.dispatch(Action::Preview));
    (app, folder)
}

fn shown_ids(app: &App) -> Vec<(String, bool)> {
    let Place::Server(server) = &app.vm.sidebar.place else {
        panic!("a server's channels");
    };
    server
        .channels
        .rows
        .iter()
        .map(|row| (row.id.clone(), row.hidden))
        .collect()
}

#[test]
fn a_category_folds_and_opens_as_the_layout_keeps_it() {
    let (mut app, folder) = app("fold");
    assert_eq!(
        app.core.state.navigation,
        Navigation::Server("friends".into())
    );
    let fold = || Intent::Channels(channels::Intent::Fold("TEXT CHANNELS".into()));
    drop(super::handle(&mut app, fold()));
    assert!(app.layout.collapsed["friends"].contains("TEXT CHANNELS"));
    // The preview's open room stays under its folded heading; the rest go.
    assert_eq!(app.core.state.selected.as_deref(), Some("general"));
    let ids = shown_ids(&app);
    assert_eq!(ids[0], ("general".to_owned(), false));
    assert!(!ids.iter().any(|(id, _)| id == "games" || id == "weekend"));
    drop(super::handle(&mut app, fold()));
    assert!(!app.layout.collapsed.contains_key("friends"), "none folded");
    assert!(shown_ids(&app).iter().any(|(id, _)| id == "games"));
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folder_opens_and_closes() {
    let (mut app, folder) = app("folder");
    drop(super::super::rail::handle(
        &mut app,
        rail::Intent::Folder("f".into()),
    ));
    assert!(app.layout.open_folders.contains("f"));
    drop(super::super::rail::handle(
        &mut app,
        rail::Intent::Folder("f".into()),
    ));
    assert!(app.layout.open_folders.is_empty());
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_search_filters_the_rows_without_the_core() {
    let (mut app, folder) = app("filter");
    drop(super::handle(&mut app, Intent::Filter("wee".into())));
    let ids: Vec<String> = shown_ids(&app).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["weekend"]);
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn nothing_drags_where_the_list_numbers_no_places() {
    let (mut app, folder) = app("drag");
    // A server without Discord's categories numbers no places.
    let arrange = Intent::Channels(channels::Intent::Arrange(Arrange::Start(1)));
    drop(super::handle(&mut app, arrange));
    assert_eq!(app.shown.sidebar.drags.arranging, None);
    let start = Move::Start {
        channel: "lounge".into(),
        user: "sam".into(),
    };
    drop(super::handle(
        &mut app,
        Intent::Channels(channels::Intent::Move(start)),
    ));
    // No voice channel takes them: the drag stops at once.
    assert_eq!(app.shown.sidebar.drags.moving, None);
    let Place::Server(server) = &app.vm.sidebar.place else {
        panic!("a server's channels");
    };
    assert!(server.channels.moving.is_none() && server.channels.arranging.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
