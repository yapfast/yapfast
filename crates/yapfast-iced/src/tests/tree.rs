//! A theme's server tree (`layout.rail: "tree"`): places open and close,
//! categories fold, the keys walk it as a file tree's, what's open is kept,
//! and a closed line still shows what's unread under it.

use iced::keyboard::{Key, key::Named};
use yapfast_core::application::{Action, Navigation};

use super::{change, unread};
use crate::{
    app::{App, Message},
    sidebar::{
        Event,
        tree::{Node, TreeKind, TreeRow},
    },
};

/// The preview (Friends open, Co-op crew beside it) under an installed
/// theme that lays the sidebar out as a tree.
fn tree_theme_in(folder: &tempfile::TempDir) -> App {
    let source = folder.path().join("tree-theme");
    if !source.exists() {
        std::fs::create_dir_all(&source).expect("theme source");
        let manifest = serde_json::json!({
            "theme_api": 1,
            "id": "dev.example.tree",
            "name": "Tree",
            "version": "1.0.0",
            "variants": {
                "dark": {
                    "base": "yap",
                    "colours": { "accent": "#5cff85" },
                    "layout": { "rail": "tree", "tree": { "expand": "one" } }
                }
            }
        });
        std::fs::write(
            source.join(yapfast_core::themes::MANIFEST),
            serde_json::to_vec(&manifest).expect("manifest JSON"),
        )
        .expect("theme manifest");
        let fetched = yapfast_core::themes::install::Fetched::local(&source).expect("package");
        yapfast_core::themes::install::Themes::open(folder.path())
            .install(&fetched)
            .expect("installed package");
        std::fs::write(
            folder.path().join("layout.json"),
            br#"{"theme":"installed:dev.example.tree"}"#,
        )
        .expect("chosen theme");
    }
    let mut app = super::start(folder, true);
    let _ = app.sync();
    app
}

fn tree_theme() -> (App, tempfile::TempDir) {
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    (tree_theme_in(&folder), folder)
}

fn row<'a>(app: &'a App, key: &str) -> &'a TreeRow {
    app.sidebar
        .tree
        .iter()
        .find(|row| row.key == key)
        .unwrap_or_else(|| panic!("{key} isn't in the tree"))
}

fn listed(app: &App, key: &str) -> bool {
    app.sidebar.tree.iter().any(|row| row.key == key)
}

fn open(app: &App, key: &str) -> bool {
    match &row(app, key).kind {
        TreeKind::Node { open, .. } | TreeKind::Heading { open, .. } => *open,
        _ => panic!("{key} doesn't open"),
    }
}

fn toggle(app: &mut App, key: &str) {
    let _ = app.update(Message::Side(Event::TreeToggle(key.into())));
}

fn press(app: &mut App, named: Named) {
    let _ = app.update(Message::Key(Key::Named(named), Default::default()));
}

fn cursor(app: &App) -> &str {
    app.sidebar.tree_cursor.as_deref().unwrap_or("")
}

const FRIENDS: &str = "server:friends";
const COOP: &str = "server:coop";
const TEXT: &str = "server:friends>heading:TEXT CHANNELS";
const GENERAL: &str = "server:friends>general";

#[test]
fn the_place_the_window_is_in_is_open_and_the_others_closed() {
    let (app, _folder) = tree_theme();
    let rows = &app.sidebar.tree;
    assert!(matches!(rows[0].kind, TreeKind::Root(_)));
    assert!(matches!(
        &row(&app, "dm").kind,
        TreeKind::Node {
            node: Node::Dms,
            open: false,
            ..
        }
    ));
    assert!(open(&app, FRIENDS));
    assert!(!open(&app, COOP));
    assert!(listed(&app, TEXT));
    assert!(listed(&app, GENERAL));
    // Nothing of a closed place is made.
    assert!(!rows.iter().any(|row| row.key.starts_with("server:coop>")));
}

#[test]
fn places_and_categories_open_and_close_and_their_lines_follow() {
    let (mut app, _folder) = tree_theme();
    // The last place ends the tree; Friends' rows run its line on.
    assert_eq!(row(&app, COOP).branch, Some(true));
    assert_eq!(row(&app, FRIENDS).branch, Some(false));
    assert_eq!(row(&app, TEXT).guides, vec![true]);
    assert_eq!(row(&app, GENERAL).guides, vec![true, true]);

    toggle(&mut app, FRIENDS);
    assert!(!open(&app, FRIENDS));
    assert!(!listed(&app, GENERAL));
    assert_eq!(app.layout.tree_open.get("friends"), Some(&false));

    toggle(&mut app, COOP);
    assert!(open(&app, COOP));
    assert_eq!(app.layout.tree_open.get("coop"), Some(&true));
    // Another place's channels, with nothing to drag: hung after its last
    // line, so no line runs on beside them.
    let announcements = row(&app, "server:coop>announcements");
    assert!(matches!(announcements.kind, TreeKind::Other(_)));
    assert_eq!(announcements.guides, vec![false, true]);

    // Opening the window's own place again keeps no note of it.
    toggle(&mut app, FRIENDS);
    assert!(open(&app, FRIENDS));
    assert!(!app.layout.tree_open.contains_key("friends"));

    // A category folds as the stock list folds it.
    toggle(&mut app, TEXT);
    assert!(!open(&app, TEXT));
    assert!(!listed(&app, GENERAL));
    assert!(
        app.layout.collapsed["friends"].contains("TEXT CHANNELS"),
        "the stock list's fold"
    );
}

#[test]
fn a_closed_line_shows_the_mentions_and_unread_under_it() {
    let (mut app, _folder) = tree_theme();
    unread(&mut app, "games", 2);
    unread(&mut app, "weekend", 0);
    unread(&mut app, "announcements", 0);
    toggle(&mut app, TEXT);
    match &row(&app, TEXT).kind {
        TreeKind::Heading { unread, badge, .. } => {
            assert!(*unread);
            assert_eq!(badge, "2");
        }
        _ => unreachable!(),
    }
    // The unread channels the stock list keeps under a fold aren't rows.
    assert!(!listed(&app, "server:friends>games"));
    match &row(&app, COOP).kind {
        TreeKind::Node { open, unread, .. } => assert!(!open && *unread),
        _ => unreachable!(),
    }
}

#[test]
fn the_keys_walk_the_tree() {
    let (mut app, _folder) = tree_theme();
    // A click gives the tree the keys, on the row clicked.
    toggle(&mut app, TEXT);
    toggle(&mut app, TEXT);
    assert_eq!(cursor(&app), TEXT);
    assert!(!app.sidebar.tree_ring, "outlined once a key moves it");

    press(&mut app, Named::ArrowDown);
    assert_eq!(cursor(&app), GENERAL);
    assert!(app.sidebar.tree_ring);
    // Left from a channel goes to its category; on an open one closes it.
    press(&mut app, Named::ArrowLeft);
    assert_eq!(cursor(&app), TEXT);
    press(&mut app, Named::ArrowLeft);
    assert!(!open(&app, TEXT));
    // Right opens it, then steps into its first channel.
    press(&mut app, Named::ArrowRight);
    assert!(open(&app, TEXT));
    press(&mut app, Named::ArrowRight);
    assert_eq!(cursor(&app), GENERAL);
    // Up past the category to its place, Left out of it, Home and End.
    press(&mut app, Named::ArrowUp);
    press(&mut app, Named::ArrowUp);
    assert_eq!(cursor(&app), FRIENDS);
    press(&mut app, Named::Home);
    assert_eq!(cursor(&app), "dm");
    press(&mut app, Named::End);
    assert_eq!(cursor(&app), COOP);
    // Enter on another place goes there, which opens it.
    press(&mut app, Named::Enter);
    assert_eq!(
        app.client.state().navigation,
        Navigation::Server("coop".into())
    );
    assert!(open(&app, COOP));
    assert!(!open(&app, FRIENDS), "the place left closes");
    // Enter on a channel opens it.
    press(&mut app, Named::ArrowDown);
    press(&mut app, Named::ArrowDown);
    let room = cursor(&app).trim_start_matches("server:coop>").to_owned();
    press(&mut app, Named::Enter);
    assert_eq!(app.client.state().selected.as_deref(), Some(room.as_str()));
    // Escape gives the keys back.
    press(&mut app, Named::Escape);
    assert!(app.sidebar.tree_cursor.is_none());
}

#[test]
fn a_channel_of_a_place_the_window_isnt_in_opens_there() {
    let (mut app, _folder) = tree_theme();
    toggle(&mut app, COOP);
    let _ = app.update(Message::Side(Event::Select("announcements".into())));
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::Server("coop".into()));
    assert_eq!(state.selected.as_deref(), Some("announcements"));
}

#[test]
fn going_to_a_place_or_a_channel_opens_the_way_to_it() {
    let (mut app, _folder) = tree_theme();
    toggle(&mut app, FRIENDS);
    app.client
        .dispatch(Action::Navigate(Navigation::Server("coop".into())));
    let _ = app.sync();
    // Friends, left closed, is closed as a place left is.
    app.client
        .dispatch(Action::Navigate(Navigation::Server("friends".into())));
    let _ = app.sync();
    assert!(open(&app, FRIENDS), "going there opens it");
    // A channel in a folded category opened (as a mention's jump does)
    // unfolds it.
    toggle(&mut app, TEXT);
    assert!(!open(&app, TEXT));
    app.client.dispatch(Action::Open("games".into()));
    let _ = app.sync();
    assert!(open(&app, TEXT));
}

#[test]
fn what_is_open_is_kept_for_the_next_start() {
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    let mut app = tree_theme_in(&folder);
    toggle(&mut app, FRIENDS);
    toggle(&mut app, COOP);
    crate::layout::flush(folder.path());
    drop(app);
    let app = tree_theme_in(&folder);
    // The window starts in Friends, which stays closed as it was left.
    assert!(!open(&app, FRIENDS));
    assert!(open(&app, COOP));
}

#[test]
fn the_tree_draws_only_open_places_and_rows_near_its_view() {
    let (mut app, _folder) = tree_theme();
    let open_rows = app.sidebar.tree.len();
    toggle(&mut app, FRIENDS);
    assert!(app.sidebar.tree.len() < open_rows);
    let _ = app.view();
    // A stock theme makes no tree at all.
    let (mut stock, _stock_folder) = super::preview();
    change(&mut stock, |_| {});
    assert!(stock.sidebar.tree.is_empty());
}

#[test]
fn the_rows_of_another_open_place_ask_for_their_pictures() {
    let (mut app, _folder) = tree_theme();
    if !open(&app, COOP) {
        toggle(&mut app, COOP);
    }
    let others: Vec<String> = app
        .sidebar
        .tree
        .iter()
        .filter_map(|row| match &row.kind {
            TreeKind::Other(side) => match side.as_ref() {
                crate::sidebar::SideRow::Room(room) => Some(room.id.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert!(!others.is_empty(), "Co-op crew's rows are in the tree");
    let shown = crate::sidebar::tree::rows_in_view(&app, 0.0, f32::MAX);
    for id in others {
        assert!(
            shown
                .iter()
                .any(|row| matches!(row, crate::sidebar::SideRow::Room(room) if room.id == id)),
            "{id}'s pictures are asked for"
        );
    }
}
