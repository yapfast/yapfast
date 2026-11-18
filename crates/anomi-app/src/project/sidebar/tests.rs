//! The sidebar's rows from a state, ported from the Slint app's tests of
//! `show_sidebar`, `fold_collapsed` and `with_empty_categories`: on the
//! core's preview (server "friends": general, games, weekend and the voice
//! channel lounge; DMs sam, riley and party).

use std::collections::BTreeSet;
use std::sync::Arc;

use yapfast_core::application::{
    Action, Application, BrowseChannels, BrowsedChannel, BrowsedKind, ListedCategory, Navigation,
};
use yapfast_core::chat::ChatRoom;
use yapfast_core::discord;
use yapfast_core::voice::{Participant, Phase};

use anomi_vm::browse::{Browse, Kind};
use anomi_vm::sidebar::channels::{ChannelKind, ChannelRow, Channels};
use anomi_vm::sidebar::dms::Empty;
use anomi_vm::sidebar::{Place, Sidebar};

use super::{BrowseShown, Shown, fold, people, show, show_browse};
use crate::pictures::Requests;
use crate::project::rail::tests::layout;

fn preview() -> Application {
    let mut core = Application::default();
    drop(core.dispatch(Action::Preview));
    core
}

fn edit(core: &mut Application, id: &str, change: impl FnOnce(&mut ChatRoom)) {
    let room = core.state.rooms.iter_mut().find(|room| room.id == id);
    change(Arc::make_mut(room.expect("a preview room")));
    core.state.list_changes += 1;
}

fn sidebar(core: &Application) -> Sidebar {
    let mut sidebar = Sidebar::default();
    let mut shown = Shown::default();
    show(
        core,
        &layout(),
        &mut sidebar,
        &mut shown,
        &mut Requests::default(),
    );
    sidebar
}

fn channels(core: &Application) -> Channels {
    match sidebar(core).place {
        Place::Server(server) => server.channels,
        Place::Dms(_) => panic!("a server's channels"),
    }
}

fn row(id: &str, heading: &str) -> ChannelRow {
    ChannelRow {
        id: id.into(),
        heading: heading.into(),
        category: heading.into(),
        ..ChannelRow::default()
    }
}

#[test]
fn pinned_dms_come_first_and_requests_wait_on_their_screen() {
    let mut core = preview();
    core.state.navigation = Navigation::DirectMessages;
    edit(&mut core, "riley", |room| {
        room.category = discord::REQUESTS.into();
    });
    // Shown before the pin: the rows follow it once the core counts it.
    let (mut sidebar, mut shown, kept) = (Sidebar::default(), Shown::default(), layout());
    let mut again = |core: &Application| {
        show(
            core,
            &kept,
            &mut sidebar,
            &mut shown,
            &mut Requests::default(),
        );
    };
    again(&core);
    core.state.pinned = vec!["party".into()];
    core.state.people_changed();
    again(&core);
    let Place::Dms(dms) = sidebar.place else {
        panic!("the DM list");
    };
    let ids: Vec<&str> = dms.rows.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(ids, ["party", "sam"]);
    assert!(dms.rows[0].pinned);
    assert_eq!(dms.requests_badge, "1", "a message request waits");
    assert_eq!(dms.empty, Empty::None);
}

#[test]
fn a_filter_matching_no_dm_says_so() {
    let mut core = preview();
    core.state.navigation = Navigation::DirectMessages;
    let mut sidebar = Sidebar {
        filter: "nobody".into(),
        ..Sidebar::default()
    };
    show(
        &core,
        &layout(),
        &mut sidebar,
        &mut Shown::default(),
        &mut Requests::default(),
    );
    let Place::Dms(dms) = sidebar.place else {
        panic!("the DM list");
    };
    assert!(dms.rows.is_empty());
    assert_eq!(dms.empty, Empty::NoMatch);
    core.state.rooms.clear();
    core.state.list_changes += 1;
    assert_eq!(
        super::dms::build(&core.state, &key(&core, "")).empty,
        Empty::NoDms
    );
}

fn key(core: &Application, filter: &str) -> super::Key {
    super::Key::of(&core.state, &layout(), filter, None)
}

#[test]
fn a_server_without_categories_heads_its_text_and_voice_channels() {
    let core = preview();
    let channels = channels(&core);
    let headed: Vec<(&str, &str)> = channels
        .rows
        .iter()
        .filter(|row| !row.heading.is_empty())
        .map(|row| (row.id.as_str(), row.heading.as_str()))
        .collect();
    assert_eq!(
        headed,
        [("general", "TEXT CHANNELS"), ("lounge", "VOICE CHANNELS")]
    );
    let lounge = channels.rows.iter().find(|row| row.id == "lounge").unwrap();
    assert_eq!(lounge.kind, ChannelKind::Voice);
    assert_eq!(channels.server, "friends");
}

#[test]
fn pinned_channels_lead_with_their_threads() {
    let mut core = preview();
    let mut server = (*core.state.servers[0]).clone();
    server.pinned = vec!["weekend".into()];
    server.channels.push("plans".into());
    server.channels.push("tickets".into());
    core.state.servers[0] = Arc::new(server);
    let thread = |id: &str| {
        let mut room = (*core.state.rooms[0]).clone();
        room.id = id.into();
        room.name = id.into();
        room.parent = Some("weekend".into());
        Arc::new(room)
    };
    let (plans, tickets) = (thread("plans"), thread("tickets"));
    core.state.rooms.extend([plans, tickets]);
    core.state.list_changes += 1;
    let channels = channels(&core);
    let first: Vec<(&str, bool, bool)> = channels.rows[..3]
        .iter()
        .map(|row| (row.id.as_str(), row.pinned, row.thread_continues))
        .collect();
    assert_eq!(
        first,
        [
            ("weekend", true, false),
            ("plans", true, true),
            ("tickets", true, false)
        ]
    );
    assert_eq!(channels.rows[0].heading, "PINNED CHANNELS");
    assert!(channels.rows[1].thread());
    assert!(!channels.rows[3].pinned, "then the rest");
}

#[test]
fn a_folded_category_keeps_what_needs_seeing() {
    let rows = vec![
        row("general", "Text"),
        ChannelRow {
            unread: true,
            ..row("games", "")
        },
        row("quiet", ""),
        ChannelRow {
            selected: true,
            ..row("open", "")
        },
        row("call", ""),
        row("lone", "Empty"),
        row("other", "Open"),
    ];
    let collapsed: BTreeSet<String> = ["Text".into(), "Empty".into()].into();
    let folded = fold::fold_collapsed(rows, &collapsed, Some("call"));
    let shown: Vec<(&str, bool, bool)> = folded
        .iter()
        .map(|row| (row.id.as_str(), row.hidden, row.collapsed))
        .collect();
    assert_eq!(
        shown,
        [
            ("general", true, true),
            ("games", false, true),
            ("open", false, true),
            ("call", false, true),
            ("lone", true, true),
            ("other", false, false),
        ]
    );
}

#[test]
fn empty_categories_show_in_their_place_and_places_number_the_rest() {
    let listed = |id: &str, empty| ListedCategory {
        id: id.into(),
        name: id.to_uppercase(),
        empty,
    };
    let rows = vec![row("a1", "a"), row("a2", ""), row("c1", "c")];
    let categories = [
        listed("a", false),
        listed("b", true),
        listed("c", false),
        listed("d", true),
    ];
    let mut rows = fold::with_empty_categories(rows, &categories);
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(ids, ["a1", "a2", "b", "c1", "d"]);
    assert!(rows[2].hidden && rows[2].heading == "B");
    let mut places = Vec::new();
    fold::number(&mut rows, 0, &mut places);
    assert_eq!(places, ["a", "a1", "a2", "b", "c", "c1", "d"]);
    let numbered: Vec<(u32, u32)> = rows
        .iter()
        .map(|row| (row.heading_place, row.place))
        .collect();
    assert_eq!(numbered, [(1, 2), (0, 3), (4, 0), (5, 6), (7, 0)]);
}

fn participant(id: &str, speaking: bool) -> Participant {
    Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local: false,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking,
        connected: true,
        problem: None,
        streaming: false,
        video: None,
        avatar: None,
    }
}

#[test]
fn the_call_lists_its_participants_and_follows_who_speaks() {
    let mut core = preview();
    let mut channels = channels(&core);
    assert!(!people::follow(&core, &mut channels), "no call, no change");
    core.state.voice.room = Some("lounge".into());
    core.state.voice.phase = Phase::Connected;
    core.state.voice.participants = Arc::new([participant("sam", false)]);
    assert!(people::follow(&core, &mut channels));
    let lounge = channels.rows.iter().find(|row| row.id == "lounge").unwrap();
    assert!(lounge.in_call && lounge.people.len() == 1 && !lounge.people[0].speaking);
    assert!(!people::follow(&core, &mut channels), "nothing new");
    core.state.voice.participants = Arc::new([participant("sam", true)]);
    assert!(people::follow(&core, &mut channels));
    let lounge = channels.rows.iter().find(|row| row.id == "lounge").unwrap();
    assert!(lounge.people[0].speaking, "ringed");
    core.state.voice.phase = Phase::Disconnected;
    assert!(
        people::follow(&core, &mut channels),
        "back to the room's own"
    );
    assert!(channels.rows.iter().all(|row| !row.in_call));
}

#[test]
fn browse_lists_the_matching_channels_under_their_categories() {
    let mut core = preview();
    let channel = |id: &str, kind| BrowsedChannel {
        id: id.into(),
        name: id.into(),
        kind,
        ..BrowsedChannel::default()
    };
    core.state.browse = Some(BrowseChannels {
        server: "friends".into(),
        rows: vec![
            channel("rules", BrowsedKind::Text),
            channel("Games", BrowsedKind::Category),
            channel("chess", BrowsedKind::Text),
            channel("voice", BrowsedKind::Voice),
            channel("Talk", BrowsedKind::Category),
            channel("chatter", BrowsedKind::Forum),
        ],
    });
    let (mut browse, mut shown) = (Browse::default(), BrowseShown::default());
    show_browse(&core.state, &mut browse, &mut shown);
    assert_eq!(browse.rows.len(), 6);
    browse.filter = "CH".into();
    show_browse(&core.state, &mut browse, &mut shown);
    let rows: Vec<(&str, Kind)> = browse
        .rows
        .iter()
        .map(|row| (row.id.as_str(), row.kind))
        .collect();
    assert_eq!(
        rows,
        [
            ("Games", Kind::Category),
            ("chess", Kind::Text),
            ("Talk", Kind::Category),
            ("chatter", Kind::Forum)
        ]
    );
}

#[test]
fn rows_are_built_again_only_when_their_inputs_change() {
    let core = preview();
    let mut sidebar = Sidebar::default();
    let mut shown = Shown::default();
    let layout = layout();
    show(
        &core,
        &layout,
        &mut sidebar,
        &mut shown,
        &mut Requests::default(),
    );
    let version = sidebar.version;
    show(
        &core,
        &layout,
        &mut sidebar,
        &mut shown,
        &mut Requests::default(),
    );
    assert_eq!(sidebar.version, version, "the same state shows the same");
    sidebar.filter = "gen".into();
    show(
        &core,
        &layout,
        &mut sidebar,
        &mut shown,
        &mut Requests::default(),
    );
    assert!(sidebar.version > version);
    let Place::Server(server) = &sidebar.place else {
        panic!("a server");
    };
    assert_eq!(server.channels.rows.len(), 1);
}

#[test]
fn a_busy_voice_channel_counts_its_call_by_the_minute() {
    let mut core = preview();
    let started = yapfast_core::clock::now_ms() / 1000 - 600;
    edit(&mut core, "lounge", |room| {
        room.voice_started = Some(started)
    });
    let mut sidebar = sidebar(&core);
    assert!(super::timed(&sidebar));
    let now = started + 600;
    super::tick(&core.state, &mut sidebar, now);
    let version = sidebar.version;
    assert!(
        !super::tick(&core.state, &mut sidebar, now),
        "the same minute"
    );
    assert!(super::tick(&core.state, &mut sidebar, now + 3600));
    assert!(sidebar.version > version);
}
