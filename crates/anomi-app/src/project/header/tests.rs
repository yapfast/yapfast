//! The header from a state, on the core's preview (server "Friends" with
//! general, games and the voice channel lounge; DMs Sam, Riley and the
//! group "Friday crew"), its rooms given Discord's abilities where a button
//! needs them.

use std::sync::Arc;

use yapfast_core::application::{Action, Application, DmScreen, Navigation, Pins};
use yapfast_core::chat::{ChatRoom, Topic};
use yapfast_core::network::Network;

use anomi_vm::header::{Channel, Direct, Header, Members, Place, Screen, Title};
use anomi_vm::icon::Icon;
use anomi_vm::shell::{Layer, Shell};

use super::{Shown, show};
use crate::layout::Layout;
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

/// Discord's abilities: pins, search, calls in DMs, groups.
fn discord(core: &mut Application, id: &str) {
    edit(core, id, |room| {
        room.capabilities = Network::Discord.capabilities();
    });
}

/// The header of `core`, shown afresh.
fn header(core: &Application, layout: &Layout) -> Place {
    let mut header = Header::None;
    show(
        &core.state,
        layout,
        false,
        &mut Shell::default(),
        &mut header,
        &mut Shown::default(),
        &mut Requests::default(),
    );
    match header {
        Header::Open(place) => *place,
        Header::None => panic!("a place"),
    }
}

fn open(core: &mut Application, id: &str) {
    drop(core.dispatch(Action::Open(id.into())));
}

#[test]
fn with_nothing_open_the_title_is_the_screens_or_the_sidebars() {
    let mut core = preview();
    drop(core.dispatch(Action::Navigate(Navigation::DirectMessages)));
    drop(core.dispatch(Action::OpenDmScreen(DmScreen::Friends)));
    let place = header(&core, &layout());
    assert_eq!(place.title, Title::Screen(Screen::Friends { all: false }));
    assert_eq!(place.tools, Default::default());
    drop(core.dispatch(Action::OpenDmScreen(DmScreen::Requests)));
    assert_eq!(
        header(&core, &layout()).title,
        Title::Screen(Screen::Requests)
    );
    core.state.dm_screen = None;
    core.state.selected = None;
    let words = match header(&core, &layout()).title {
        Title::Sidebar(words) => words,
        title => panic!("{title:?}"),
    };
    assert_eq!(words, "Direct messages");
}

#[test]
fn a_one_to_one_dm_names_its_person_and_what_they_do() {
    let mut core = preview();
    discord(&mut core, "sam");
    edit(&mut core, "sam", |room| {
        room.activity = Some("Playing   Factorio".into());
    });
    open(&mut core, "sam");
    let place = header(&core, &layout());
    let Title::Direct(Direct {
        name,
        person,
        activity,
        ..
    }) = &place.title
    else {
        panic!("{:?}", place.title)
    };
    assert_eq!((name.as_str(), *person), ("Sam", true));
    assert_eq!(
        activity.as_ref().map(|status| status.text.as_str()),
        Some("Playing Factorio")
    );
    let tools = &place.tools;
    // A one-to-one DM lists no members: its profile shows instead.
    assert_eq!((tools.members.as_ref(), tools.profile), (None, Some(true)));
    assert!(tools.pins == Some(false) && tools.search);
    let mut hidden = layout();
    hidden.edit(|saved| saved.profile_panel_hidden = true);
    assert_eq!(header(&core, &hidden).tools.profile, Some(false));
}

#[test]
fn a_group_lists_its_members_and_keeps_the_switch() {
    let mut core = preview();
    open(&mut core, "party");
    let place = header(&core, &layout());
    let Title::Direct(direct) = &place.title else {
        panic!("{:?}", place.title)
    };
    assert!(!direct.person && direct.activity.is_none());
    let members = Members {
        showing: true,
        label: "3 members".into(),
    };
    assert_eq!(place.tools.members.as_ref(), Some(&members));
    let mut hidden = layout();
    hidden.edit(|saved| saved.members_hidden = true);
    assert!(!header(&core, &hidden).tools.members.unwrap().showing);
}

#[test]
fn a_channel_shows_its_topic_on_one_line_and_in_full() {
    let mut core = preview();
    open(&mut core, "general");
    core.state.topic = Some(Arc::new(Topic {
        body: "Plans\nand links".into(),
        ..Topic::default()
    }));
    let place = header(&core, &layout());
    let Title::Channel(Channel {
        icon,
        name,
        topic,
        line,
        parent,
    }) = &place.title
    else {
        panic!("{:?}", place.title)
    };
    assert_eq!(
        (*icon, name.as_str(), parent),
        (Icon::Hash, "general", &None)
    );
    assert!(topic.is_some() && line.is_some());
    assert!(place.tools.members.is_some() && !place.tools.more);
}

#[test]
fn the_popovers_close_as_the_core_lets_go_of_what_they_list() {
    let mut core = preview();
    discord(&mut core, "sam");
    open(&mut core, "sam");
    let (mut shell, mut header, mut shown) = (Shell::default(), Header::None, Shown::default());
    let mut again = |core: &Application, shell: &mut Shell| {
        show(
            &core.state,
            &layout(),
            false,
            shell,
            &mut header,
            &mut shown,
            &mut Requests::default(),
        );
    };
    core.state.pins = Some(Pins {
        room: "sam".into(),
        messages: None,
    });
    again(&core, &mut shell);
    shell.open(Layer::Pins);
    shell.open(Layer::Topic);
    again(&core, &mut shell);
    assert!(shell.is_open(Layer::Pins));
    core.state.pins = None;
    again(&core, &mut shell);
    assert!(!shell.is_open(Layer::Pins));
    // The topic of another place closes with it.
    assert!(shell.is_open(Layer::Topic));
    open(&mut core, "general");
    again(&core, &mut shell);
    assert!(!shell.is_open(Layer::Topic));
}
