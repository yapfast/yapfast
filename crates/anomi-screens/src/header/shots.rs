//! The header's shots: each kind of place in the whole window. They sit in
//! the `header` folder, beside the golden screenshots they match
//! (`dm/person_chat`, `server/general_unread_divider`, `thread/panel`,
//! `small/server_general`). Task N07's.

use iced::{Point, Rectangle, Size};

use anomi_vm::Vm;
use anomi_vm::fixtures::{conversation, header, shell};
use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;

use crate::shots::{GOLDEN, SMALL, Shot};

fn with(mut vm: Vm, header: anomi_vm::header::Header) -> Vm {
    vm.header = header;
    vm
}

/// `dm/person_chat`: Person 5, with Add Friend, calls and the profile.
fn person_chat() -> Vm {
    shell::dm_home()
}

/// A DM that is playing, with a request to accept and a call to join.
fn bob() -> Vm {
    with(shell::dm_home(), header::dm_activity())
}

fn group() -> Vm {
    with(shell::dm_home(), header::group())
}

/// `server/general_unread_divider`: a channel with its topic.
fn general() -> Vm {
    shell::server_home()
}

/// `thread/panel`: the channel, the thread, and the thread's menu.
fn thread() -> Vm {
    with(
        shell::channel_with(conversation::thread_beside),
        header::thread(),
    )
}

fn voice() -> Vm {
    with(shell::channel_with(conversation::voice), header::voice())
}

fn friends() -> Vm {
    with(shell::dm_with(conversation::friends), header::friends())
}

/// `small/server_general`: the topic cut short and fading out.
fn small() -> Vm {
    shell::server_home()
}

/// The topic in full, under its line in the header.
fn topic_open() -> Vm {
    let line = Rectangle::new(Point::new(508.0, 13.0), Size::new(600.0, 24.0));
    shell::open_at(shell::server_home(), Layer::Topic, Spot::below(line))
}

/// No room selected, none of Friends, Requests or Browse open: the sidebar's
/// title stands for the place.
fn sidebar_title() -> Vm {
    with(
        shell::dm_with(anomi_vm::conversation::Conversation::default),
        header::sidebar_title(),
    )
}

/// `small/dm_conversation`: Bob's activity beside him in a small window,
/// until the search field takes its room (N01).
fn narrow() -> Vm {
    with(shell::dm_home(), header::dm_narrow())
}

pub fn shots() -> Vec<Shot> {
    let shot = |name, size, vm: fn() -> Vm| Shot {
        area: "header",
        name,
        size,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("person_chat", GOLDEN, person_chat),
        shot("dm_activity", GOLDEN, bob),
        shot("group", GOLDEN, group),
        shot("general", GOLDEN, general),
        shot("thread_panel", GOLDEN, thread),
        shot("voice", GOLDEN, voice),
        shot("friends", GOLDEN, friends),
        shot("server_general_small", SMALL, small),
        shot("topic_open", GOLDEN, topic_open),
        shot("sidebar_title", GOLDEN, sidebar_title),
        shot("dm_narrow", SMALL, narrow),
        Shot {
            theme: "catppuccin-latte",
            ..shot("general_latte", GOLDEN, general)
        },
    ]
}
