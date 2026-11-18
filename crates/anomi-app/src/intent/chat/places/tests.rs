//! Search, pins and the thread beside on the core's preview (server
//! "Friends" with general and games; DMs Sam and Riley), Sam given
//! Discord's abilities: what the screens ask reaches the core, and what
//! the core holds shows, each side building only what changed.

use std::sync::Arc;

use yapfast_core::application::{Action, Application, Pins};
use yapfast_core::chat::{ChatMessage, SearchHit};
use yapfast_core::network::Network;
use yapfast_lang::{tr, trn};

use anomi_vm::chat::composer::Intent as ComposerIntent;
use anomi_vm::chat::search::Intent as SearchIntent;
use anomi_vm::chat::thread_panel::Intent as PanelIntent;
use anomi_vm::chat::{self as chat};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::project::Reach;

fn app(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-places-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    drop(app.core.dispatch(Action::Preview));
    for room in app.core.state.rooms.iter_mut() {
        if room.id == "sam" {
            Arc::make_mut(room).capabilities = Network::Discord.capabilities();
        }
    }
    app.core.state.list_changes += 1;
    (app, folder)
}

fn done(app: App, folder: std::path::PathBuf) {
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

fn ask(app: &mut App, intent: impl Into<chat::Intent>) {
    drop(super::handle(app, intent.into()));
}

fn message(item: &str, body: &str) -> ChatMessage {
    ChatMessage {
        id: item.into(),
        item: item.into(),
        author: "Sam".into(),
        body: body.into(),
        formatted: true,
        timestamp: 1_790_000_000,
        ..ChatMessage::default()
    }
}

fn hit(item: &str) -> SearchHit {
    SearchHit {
        room: "sam".into(),
        message: message(item, &format!("found {item}")),
    }
}

/// The room games as a thread beside the channel, its history come.
fn beside(core: &mut Application, messages: Vec<Arc<ChatMessage>>) {
    let aside = core.state.aside.insert(Default::default());
    aside.room = "games".into();
    (aside.ready, aside.history_start) = (true, true);
    aside.messages = Arc::new(messages);
}

#[test]
fn typing_a_search_reaches_nothing_until_enter_and_the_results_build_once() {
    let (mut app, folder) = app("search");
    drop(app.dispatch(Action::Open("sam".into())));
    assert_eq!(app.vm.chat.search.placeholder, tr!("Search {}", "Sam"));
    ask(&mut app, SearchIntent::Edit("found".into()));
    assert_eq!(app.vm.chat.search.text, "found");
    assert!(app.core.state.search.is_none());
    ask(&mut app, SearchIntent::Submit);
    assert_eq!(
        app.core
            .state
            .search
            .as_ref()
            .map(|search| search.query.as_str()),
        Some("found")
    );
    assert_eq!(app.vm.chat.search.heading, tr!("Searching..."));
    // A page comes: its rows build.
    let search = app.core.state.search.as_mut().expect("a search");
    (search.loading, search.total, search.next) = (false, 3, 2);
    search.results = vec![hit("a"), hit("b")];
    app.refresh();
    let shown = &mut app.vm.chat.search;
    assert_eq!(shown.heading, trn!("{n} Result", "{n} Results", 3));
    assert_eq!(shown.results.len(), 2);
    assert_eq!(shown.results[1].room, "sam");
    // The next page builds its rows alone; those shown stay as they were.
    shown.results[0].when = "kept".into();
    let search = app.core.state.search.as_mut().expect("a search");
    search.results.push(hit("c"));
    app.refresh();
    let results = &app.vm.chat.search.results;
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].when, "kept");
    assert_eq!(results[2].item, "c");
    // The cross ends it: the field empties and the results go.
    ask(&mut app, SearchIntent::Clear);
    assert!(app.core.state.search.is_none());
    assert!(app.vm.chat.search.text.is_empty() && app.vm.chat.search.results.is_empty());
    done(app, folder);
}

#[test]
fn a_pin_edited_shows_again_and_the_pins_go_with_their_popover() {
    let (mut app, folder) = app("pins");
    drop(app.dispatch(Action::Open("sam".into())));
    app.core.state.pins = Some(Pins {
        room: "sam".into(),
        messages: None,
    });
    app.refresh();
    assert_eq!(app.vm.chat.pins.note, tr!("Loading pinned messages..."));
    let pinned = |body: &str| Some(Ok(vec![message("p", body)]));
    app.core.state.pins.as_mut().expect("pins").messages = pinned("first");
    app.refresh();
    assert!(app.vm.chat.pins.note.is_empty());
    let said = |app: &App| format!("{:?}", app.vm.chat.pins.pins[0].body);
    assert!(said(&app).contains("first"));
    app.core.state.pins.as_mut().expect("pins").messages = pinned("edited");
    app.refresh();
    assert!(said(&app).contains("edited"));
    // Closed, they stay while the popover plays its way out, then go.
    app.core.state.pins = None;
    app.refresh();
    assert_eq!(app.vm.chat.pins.pins.len(), 1);
    drop(crate::intent::handle(
        &mut app,
        anomi_vm::shell::Intent::Gone(Layer::Pins).into(),
    ));
    assert!(app.vm.chat.pins.pins.is_empty());
    done(app, folder);
}

#[test]
fn the_thread_beside_is_a_timeline_of_its_own() {
    let (mut app, folder) = app("aside");
    drop(app.dispatch(Action::Open("general".into())));
    beside(
        &mut app.core,
        vec![Arc::new(message("t1", "in the thread"))],
    );
    app.refresh();
    let panel = &mut app.vm.chat.thread_panel;
    assert_eq!(panel.name, "games");
    assert!(panel.timeline.aside && panel.timeline.ready);
    assert_eq!(panel.timeline.rows.len(), 1);
    assert_eq!(panel.composer.placeholder, tr!("Message {}", "games"));
    panel.timeline.rows[0].message.time = "kept".into();
    let channel = app.vm.chat.timeline.rows.len();
    // A message coming to the channel builds nothing beside it.
    let mut messages = app.core.state.messages.as_ref().clone();
    messages.push(Arc::new(message("c1", "in the channel")));
    app.core.state.messages = Arc::new(messages);
    app.refresh();
    assert_eq!(app.vm.chat.timeline.rows.len(), channel + 1);
    assert_eq!(
        app.vm.chat.thread_panel.timeline.rows[0].message.time,
        "kept"
    );
    // One coming to the thread builds its own row alone.
    app.vm.chat.timeline.rows[0].message.time = "kept".into();
    let aside = app.core.state.aside.as_mut().expect("the thread");
    let mut messages = aside.messages.as_ref().clone();
    messages.push(Arc::new(message("t2", "more")));
    aside.messages = Arc::new(messages);
    app.refresh();
    let rows = &app.vm.chat.thread_panel.timeline.rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].message.time, "kept");
    assert_eq!(app.vm.chat.timeline.rows[0].message.time, "kept");
    // Its rows are found by key, as the channel's are.
    let key = rows[1].key;
    assert_eq!(
        app.shown
            .holding(key)
            .message(key)
            .map(|found| found.item.as_str()),
        Some("t2")
    );
    done(app, folder);
}

#[test]
fn the_thread_beside_writes_its_own_draft_and_a_keystroke_reaches_the_composer_alone() {
    let (mut app, folder) = app("aside-draft");
    drop(app.dispatch(Action::Open("general".into())));
    beside(&mut app.core, Vec::new());
    app.refresh();
    let room = app.vm.chat.thread_panel.composer.room;
    assert_ne!(room, app.vm.chat.composer.room);
    let typed = ComposerIntent::Edited {
        room,
        text: "hey".into(),
        caret: 3,
    };
    ask(&mut app, PanelIntent::within(typed));
    assert_eq!(app.core.state.aside_draft(), "hey");
    assert_eq!(app.vm.chat.thread_panel.composer.draft, "hey");
    assert!(app.vm.chat.composer.draft.is_empty());
    ask(&mut app, PanelIntent::within(ComposerIntent::Focus(true)));
    assert!(app.aside_last);
    assert_eq!(
        Reach::after(&Action::SetAsideDraft(String::new())),
        Reach::COMPOSER
    );
    let named = Action::WriteNewThread {
        name: Some(String::new()),
        body: None,
    };
    assert_eq!(Reach::after(&named), Reach::FORM);
    done(app, folder);
}
