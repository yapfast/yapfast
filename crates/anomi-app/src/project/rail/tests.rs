//! The rail's rows from a state: folders summing up their servers, unread
//! DMs above, the call's speaker, and the pictures asked for as rows show.

use std::sync::Arc;
use std::time::Instant;

use yapfast_core::application::{Action, AppState, Application, Navigation};
use yapfast_core::chat::{ChatRoom, ChatServer, ServerFolder};
use yapfast_core::voice::Phase;

use anomi_vm::rail::Rail;

use super::{Shown, seen_now, show};
use crate::layout::Layout;
use crate::pictures::Requests;

/// A layout read from nowhere, which nothing writes.
pub(crate) fn layout() -> Layout {
    Layout::read(std::path::Path::new("/nonexistent/anomi-places-tests"))
}

/// The preview's state: its rooms are the patterns rooms are made from.
fn state() -> AppState {
    let mut core = Application::default();
    drop(core.dispatch(Action::Preview));
    core.state
}

fn room(state: &AppState, id: &str, direct: bool) -> ChatRoom {
    ChatRoom {
        id: id.into(),
        name: id.into(),
        direct,
        unread: 0,
        mentions: 0,
        ..(*state.rooms[0]).clone()
    }
}

fn server(id: &str, channels: &[&str], folder: Option<&ServerFolder>) -> ChatServer {
    ChatServer {
        id: id.into(),
        name: id.into(),
        channels: channels.iter().map(|&id| id.to_owned()).collect(),
        icon: Some(format!("{id}.png")),
        folder: folder.cloned(),
        ..ChatServer::default()
    }
}

/// Server `a` on its own, then `b` and `c` in a red folder without a name;
/// `b`'s channel unread, `c`'s mentioning twice; a DM unread three times.
fn rail_state() -> AppState {
    let mut state = state();
    let folder = ServerFolder {
        id: "f".into(),
        name: None,
        color: Some(0x00ff_0000),
    };
    state.servers = vec![
        server("a", &["a1"], None),
        server("b", &["b1"], Some(&folder)),
        server("c", &["c1"], Some(&folder)),
    ]
    .into_iter()
    .map(Arc::new)
    .collect();
    let unread = ChatRoom {
        unread: 1,
        ..room(&state, "b1", false)
    };
    let mentions = ChatRoom {
        mentions: 2,
        unread: 2,
        ..room(&state, "c1", false)
    };
    let dm = ChatRoom {
        unread: 3,
        ..room(&state, "dm", true)
    };
    state.rooms = [room(&state, "a1", false), unread, mentions, dm]
        .into_iter()
        .map(Arc::new)
        .collect();
    state.navigation = Navigation::Server("b".into());
    state.list_changes += 1;
    state
}

#[test]
fn folders_sum_up_their_servers_and_open_as_the_layout_keeps() {
    let state = rail_state();
    let mut layout = layout();
    layout.edit(|saved| {
        saved.open_folders.insert("f".into());
    });
    let (mut rail, mut shown) = (Rail::default(), Shown::default());
    show(
        &state,
        &layout,
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert_eq!(rail.servers.len(), 3, "a folder's servers stay listed");
    assert_eq!(rail.servers[1].folder.as_deref(), Some("f"));
    let folder = &rail.folders[0];
    assert_eq!(folder.name, "b, c", "named after its servers");
    assert_eq!(folder.tint, iced::Color::from_rgb8(0xff, 0, 0));
    assert!(folder.open && folder.tile.selected && folder.tile.unread);
    assert_eq!(folder.tile.badge, "2");
    assert!(rail.servers[1].tile.selected && rail.servers[1].tile.unread);
    assert!(!rail.servers[0].tile.unread);
    assert_eq!(rail.servers[0].avatar.initial, "A");
}

#[test]
fn unread_dms_come_above_the_servers() {
    let state = rail_state();
    let (mut rail, mut shown) = (Rail::default(), Shown::default());
    show(
        &state,
        &layout(),
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert_eq!(rail.dms.len(), 1);
    assert_eq!(
        (rail.dms[0].id.as_str(), rail.dms[0].badge.as_str()),
        ("dm", "3")
    );
    assert!(rail.home.unread && !rail.home.selected);
}

#[test]
fn the_call_shows_on_its_server_and_rebuilds_the_rail_once() {
    let mut state = rail_state();
    let (mut rail, mut shown) = (Rail::default(), Shown::default());
    let layout = layout();
    show(
        &state,
        &layout,
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert!(!rail.servers[0].call);
    state.voice.room = Some("a1".into());
    state.voice.phase = Phase::Connected;
    show(
        &state,
        &layout,
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert!(rail.servers[0].call && !rail.folders[0].call);
    // Who speaks changes nothing the rail shows.
    rail.servers[0].name = "kept".into();
    show(
        &state,
        &layout,
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert_eq!(rail.servers[0].name, "kept", "not built again");
}

#[test]
fn the_rows_in_view_ask_for_their_pictures() {
    let state = rail_state();
    let mut shown = Shown::default();
    let mut requests = Requests::default();
    seen_now(
        &state,
        &mut shown,
        vec!["a".into(), "f".into()],
        &mut requests,
    );
    let asked: Vec<String> = requests
        .take(Instant::now())
        .into_iter()
        .flat_map(|action| match action {
            Action::LoadPictures(urls) => urls,
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(asked, ["a.png", "b.png", "c.png"], "a folder's servers too");
    // Built again, the rail asks again for what's still missing.
    let mut rail = Rail::default();
    show(&state, &layout(), &mut rail, &mut shown, &mut requests);
    assert!(requests.due(Instant::now()).is_some());
}

#[test]
fn a_discord_servers_place_may_be_moved_outside_the_preview() {
    let mut state = rail_state();
    for server in &mut state.servers {
        Arc::make_mut(server).discord = true;
    }
    let (mut rail, mut shown) = (Rail::default(), Shown::default());
    let layout = layout();
    show(
        &state,
        &layout,
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert!(
        rail.servers.iter().all(|server| !server.movable),
        "the preview's"
    );
    state.preview = false;
    state.list_changes += 1;
    show(
        &state,
        &layout,
        &mut rail,
        &mut shown,
        &mut Requests::default(),
    );
    assert!(rail.servers.iter().all(|server| server.movable));
}
