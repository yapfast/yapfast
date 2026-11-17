//! The timeline's shots for `anomi-shots`, each the whole window from a
//! fixture. They sit in the `timeline` folder, named after the golden
//! screenshots they match (`server/general_unread_divider`,
//! `chan/history_plain_a`, `dm/person_chat`, `dm/conversation_profile`,
//! `voice/text_chat_of_voice_channel`), and the states no golden shot
//! shows. The messages are C02's: until it passes they draw nothing, and
//! the rows show only what the timeline puts around them. The sidebar
//! marks the timeline's room open, and the header names it.

use anomi_vm::Vm;
use anomi_vm::chat::timeline::{Edge, Timeline};
use anomi_vm::conversation::{Body, Conversation};
use anomi_vm::fixtures::chat::timeline as fixtures;
use anomi_vm::fixtures::{GOLDEN, SMALL, Shot, header, shell};
use anomi_vm::header::{Header, Title};
use anomi_vm::sidebar::Place;

/// The first server with `timeline` in it, #general's header over it.
fn channel(timeline: fn() -> Timeline) -> Vm {
    open(shell::server_home(), timeline())
}

/// The DM home with `timeline` in it, Person 5's header over it.
fn dm(timeline: fn() -> Timeline) -> Vm {
    open(shell::dm_home(), timeline())
}

/// `vm` with `timeline` in it, its room the one open in the sidebar.
fn open(mut vm: Vm, timeline: Timeline) -> Vm {
    let room = timeline.room.as_ref().map_or("", |room| room.id.as_str());
    match &mut vm.sidebar.place {
        Place::Dms(dms) => {
            for row in &mut dms.rows {
                row.selected = row.id == room;
            }
        }
        Place::Server(server) => {
            for row in &mut server.channels.rows {
                row.selected = row.id == room;
            }
        }
    }
    vm.chat.timeline = timeline;
    vm
}

/// Bob's DM, his header over it (`dm/conversation_profile`).
fn bob() -> Vm {
    let mut vm = dm(fixtures::bob);
    vm.header = header::dm_activity();
    vm
}

/// The Lounge's chat, the voice channel's header over it and no member
/// list (`voice/text_chat_of_voice_channel`).
fn lounge() -> Vm {
    let mut vm = channel(fixtures::empty_channel);
    vm.header = header::voice();
    vm.conversation.beside.members = false;
    vm
}

/// The thread Plans, its header over it.
fn plans() -> Vm {
    let mut vm = channel(fixtures::empty_thread);
    vm.header = header::thread();
    vm
}

/// The channel #plans from its first message, named over it: no channel
/// of the sidebar's.
fn welcome_channel() -> Vm {
    let mut vm = channel(fixtures::welcome_to_channel);
    if let Header::Open(place) = &mut vm.header
        && let Title::Channel(channel) = &mut place.title
    {
        channel.name = "plans".into();
        channel.topic = None;
    }
    vm
}

/// The DM home with no conversation chosen.
fn nothing() -> Vm {
    let mut vm = open(shell::dm_home(), Timeline::default());
    vm.header = Header::None;
    vm.conversation = Conversation {
        body: Body::Nothing,
        ..Conversation::default()
    };
    vm
}

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "timeline",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("general_unread_divider", || channel(fixtures::general)),
        shot("history_plain_a", || channel(fixtures::history)),
        shot("person_chat", || dm(fixtures::person_chat)),
        shot("conversation_profile", bob),
        shot("text_chat_of_voice_channel", lounge),
        shot("no_conversation", nothing),
        shot("loading", || channel(fixtures::loading)),
        shot("empty_dm", || dm(fixtures::empty_dm)),
        shot("empty_thread", plans),
        shot("earlier_messages", || {
            channel(|| fixtures::start_of_general(Edge::More))
        }),
        shot("earlier_loading", || {
            channel(|| fixtures::start_of_general(Edge::Loading))
        }),
        shot("earlier_failed", || {
            channel(|| fixtures::start_of_general(Edge::Failed))
        }),
        shot("welcome_server", || channel(fixtures::welcome_to_server)),
        shot("welcome_channel", welcome_channel),
        shot("unread_above", || channel(fixtures::unread_above)),
        shot("older_window", || {
            channel(|| fixtures::older_window(Edge::More))
        }),
        shot("newer_failed", || {
            channel(|| fixtures::older_window(Edge::Failed))
        }),
        shot("browsing", || channel(fixtures::browsing)),
        Shot {
            theme: "catppuccin-latte",
            ..shot("general_latte", || channel(fixtures::unread_above))
        },
        Shot {
            size: SMALL,
            ..shot("small", || channel(|| fixtures::older_window(Edge::More)))
        },
    ]
}
