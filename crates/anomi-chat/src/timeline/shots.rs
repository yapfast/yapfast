//! The timeline's shots for `anomi-shots`, each the whole window from a
//! fixture. They sit in the `timeline` folder, named after the golden
//! screenshots they match (`server/general_unread_divider`,
//! `chan/history_plain_a`, `dm/person_chat`, `dm/conversation_profile`,
//! `voice/text_chat_of_voice_channel`), and the states no golden shot
//! shows. The messages are C02's: until it passes they draw nothing, and
//! the rows show only what the timeline puts around them.

use anomi_vm::Vm;
use anomi_vm::chat::timeline::{Edge, Timeline};
use anomi_vm::conversation::{Body, Conversation};
use anomi_vm::fixtures::chat::timeline as fixtures;
use anomi_vm::fixtures::{GOLDEN, SMALL, Shot, shell};
use anomi_vm::header::Header;

/// The server's #general with `timeline` in it.
fn channel(timeline: fn() -> Timeline) -> Vm {
    let mut vm = shell::server_home();
    vm.chat.timeline = timeline();
    vm
}

/// The DM home's Person 5 with `timeline` in it.
fn dm(timeline: fn() -> Timeline) -> Vm {
    let mut vm = shell::dm_home();
    vm.chat.timeline = timeline();
    vm
}

/// The DM home with no conversation chosen.
fn nothing() -> Vm {
    let mut vm = shell::dm_home();
    vm.header = Header::None;
    vm.conversation = Conversation {
        body: Body::Nothing,
        ..Conversation::default()
    };
    vm.chat.timeline = Timeline::default();
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
        shot("conversation_profile", || dm(fixtures::bob)),
        shot("text_chat_of_voice_channel", || {
            channel(fixtures::empty_channel)
        }),
        shot("no_conversation", nothing),
        shot("loading", || channel(fixtures::loading)),
        shot("empty_dm", || dm(fixtures::empty_dm)),
        shot("empty_thread", || channel(fixtures::empty_thread)),
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
        shot("welcome_channel", || channel(fixtures::welcome_to_channel)),
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
