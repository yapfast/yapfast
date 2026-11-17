//! Samples of a thread beside its channel for tests and shots, after the fake
//! Discord's data: task C24's. The messages are `message::Message`'s (C02's)
//! as they come; until it passes they draw nothing, and the rows show only
//! what the timeline puts around them. The forum post's three messages (Carol,
//! Bob, a blocked one) and Plans' two are keyed as the fake's.

use iced::Rectangle;
use yapfast_lang::tr;

use crate::Vm;
use crate::chat::composer::{Composer, More, Reply, Staged};
use crate::chat::message::Message;
use crate::chat::thread_panel::ThreadPanel;
use crate::chat::timeline::group::key;
use crate::chat::timeline::{Edge, Kind, Room, Row, Timeline};
use crate::fixtures::people::members;
use crate::fixtures::sidebar::channels::PLANS;
use crate::fixtures::{conversation, header, shell};
use crate::icon::Icon;
use crate::menu::{Menu, Spot};

use super::{composer as composers, forum, timeline};

/// The forum post's thread, as the fake's "Dark theme for the sidebar".
pub const POST: &str = "discord:c7";

/// The "..." button of the golden window's panel (`thread/panel_menu`): 28 px
/// at its header's right, 10 px in from the cross's side.
pub const MORE: Rectangle = Rectangle {
    x: 1289.0,
    y: 60.0,
    width: 28.0,
    height: 28.0,
};

/// A thread's composer: only the plus, which takes files; no GIFs or
/// stickers.
pub fn composer(name: &str) -> Composer {
    Composer {
        room: key(name),
        enabled: true,
        placeholder: tr!("Message {}", name),
        more: More {
            attach: true,
            ..More::default()
        },
        ..Composer::default()
    }
}

fn thread(id: &str, name: &str) -> Room {
    Room {
        id: id.into(),
        name: name.into(),
        kind: Kind::Thread(Icon::Thread),
        intro: None,
    }
}

/// A day's first message, and then each of the others on its own line.
fn rows(ids: &[&str]) -> Vec<Row> {
    ids.iter()
        .enumerate()
        .map(|(index, id)| Row {
            key: key(id),
            first: true,
            day: (index == 0).then(|| timeline::day(2, tr!("October"), 2026)),
            message: Message::default(),
            ..Row::default()
        })
        .collect()
}

/// The post "Dark theme for the sidebar", its history at its start
/// (`forum/post_open`).
pub fn post() -> ThreadPanel {
    let name = "Dark theme for the sidebar";
    ThreadPanel {
        name: name.into(),
        timeline: Timeline {
            room: Some(thread(POST, name)),
            ready: true,
            earlier: Edge::Start,
            rows: rows(&["post-1", "post-2", "post-3"]),
            aside: true,
            ..Timeline::default()
        },
        composer: composer(name),
    }
}

/// Its history not yet come: skeletons stand in for the messages.
pub fn loading() -> ThreadPanel {
    let mut panel = post();
    panel.timeline.ready = false;
    panel.timeline.rows.clear();
    panel
}

/// A thread nobody has written in yet: its start, centred.
pub fn empty() -> ThreadPanel {
    let mut panel = post();
    panel.timeline.rows.clear();
    panel
}

/// Earlier messages to load above the first, which a click asks for.
pub fn earlier() -> ThreadPanel {
    let mut panel = post();
    panel.timeline.earlier = Edge::More;
    panel
}

/// Earlier messages on their way.
pub fn loading_earlier() -> ThreadPanel {
    let mut panel = earlier();
    panel.timeline.earlier = Edge::Loading;
    panel
}

/// Earlier messages that didn't come: Retry.
pub fn failed() -> ThreadPanel {
    let mut panel = earlier();
    panel.timeline.earlier = Edge::Failed;
    panel
}

/// A reply to Bob being written, with a file waiting: the bar on the frame
/// and the files over the field.
pub fn replying() -> ThreadPanel {
    let mut panel = post();
    panel.composer = Composer {
        draft: "Agreed".into(),
        placeholder: tr!("Write a reply").into(),
        focused: true,
        reply: Some(Reply {
            label: tr!("Replying to {}: {}", "Bob", "Yes please"),
            ping: true,
        }),
        staged: vec![composers::staged("icon.png", &tr!("{} KB", 7), true, false)],
        can_spoil: true,
        ..panel.composer
    };
    panel
}

/// A thread the person may not write in: the field takes nothing.
pub fn locked() -> ThreadPanel {
    let mut panel = post();
    panel.composer.enabled = false;
    panel.composer.more = More::default();
    panel
}

/// A file waiting to go, for tests of its buttons.
pub fn staging(file: Staged) -> ThreadPanel {
    let mut panel = post();
    panel.composer.staged = vec![file];
    panel
}

/// The window with the forum "ideas" open and `panel` beside it
/// (`forum/post_open`).
pub fn in_forum(panel: ThreadPanel) -> Vm {
    let mut vm = forum::open();
    vm.conversation.beside = conversation::thread_beside().beside;
    vm.conversation.beside.members = false;
    vm.chat.thread_panel = panel;
    vm
}

/// `forum/post_open.png`.
pub fn forum_post_open() -> Vm {
    in_forum(post())
}

/// `thread/panel_menu.png`: the "..." menu under the button.
pub fn menu_open() -> Vm {
    let mut vm = forum_post_open();
    let rows = vm.chat.thread_panel.menu_rows();
    vm.menu = Menu::new(1, Spot::below(MORE), rows);
    vm
}

/// `thread/panel.png`: the thread "Plans" opened as the channel itself, its
/// breadcrumb in the header, the member list beside it.
pub fn plans_as_channel() -> Vm {
    let mut vm = shell::channel_with(conversation::channel);
    vm.header = header::thread();
    vm.chat.timeline = Timeline {
        room: Some(thread(PLANS, "Plans")),
        ready: true,
        earlier: Edge::Start,
        rows: rows(&["plans-1", "plans-2"]),
        ..Timeline::default()
    };
    // The channel's composer, in a thread: GIFs and stickers beside the
    // field, polls but no threads under the plus.
    vm.chat.composer = Composer {
        more: More {
            attach: true,
            polls: true,
            ..More::default()
        },
        gifs: true,
        stickers: true,
        ..composer("Plans")
    };
    vm.people.members = members::server();
    vm
}
