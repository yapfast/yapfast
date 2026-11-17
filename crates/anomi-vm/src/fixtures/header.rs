//! Samples of the conversation's header. Task N07's.

use crate::header::{
    AddPeople, Call, Channel, Direct, Friend, Header, Members, Parent, Place, Screen, Title, Tools,
};
use crate::icon::Icon;
use crate::kind::Presence;
use crate::text::{self, Text};

use super::pictures::pictured;
use super::{avatar, kind};

const TOPIC: &str = "Co-op nights, plans and links.";

fn open(title: Title, tools: Tools) -> Header {
    Header::Open(Box::new(Place { title, tools }))
}

/// A topic with a link and some style, long enough to be cut short.
pub fn long_topic() -> Text {
    text::plain(
        "Co-op nights every Friday at 20:00, plans for the weekend, and links to the \
         wiki: https://example.com/wiki. Be kind, keep to the topic and read the pins.",
    )
}

/// Person 5's DM: their avatar and name as a button, Add Friend, a call,
/// pins, adding people and their profile.
pub fn dm() -> Header {
    open(
        Title::Direct(Direct {
            name: "Person 5".into(),
            avatar: pictured(avatar("P", 0x3a9357), "Person 5"),
            person: true,
            ..Direct::default()
        }),
        Tools {
            friend: Friend::Add,
            call: Some(Call {
                going: false,
                can: true,
            }),
            pins: Some(false),
            add_people: Some(AddPeople {
                open: false,
                one_to_one: true,
            }),
            profile: Some(true),
            search: true,
            ..Tools::default()
        },
    )
}

/// Bob's DM: online, playing, a call going on, and a request to accept.
pub fn dm_activity() -> Header {
    open(
        Title::Direct(Direct {
            name: "Bob".into(),
            avatar: pictured(avatar("B", 0x9a5c6a), "Bob"),
            presence: Presence::DoNotDisturb,
            person: true,
            activity: Some(kind::bob_status()),
        }),
        Tools {
            friend: Friend::Accept,
            call: Some(Call {
                going: true,
                can: true,
            }),
            pins: Some(true),
            add_people: Some(AddPeople {
                open: false,
                one_to_one: true,
            }),
            profile: Some(false),
            search: true,
            ..Tools::default()
        },
    )
}

/// A group: its picture and name are no button, the member list is.
pub fn group() -> Header {
    open(
        Title::Direct(Direct {
            name: "Weekend plans".into(),
            avatar: avatar("W", 0xc27a2c),
            ..Direct::default()
        }),
        Tools {
            call: Some(Call {
                going: false,
                can: true,
            }),
            pins: Some(false),
            add_people: Some(AddPeople {
                open: false,
                one_to_one: false,
            }),
            members: Some(Members {
                showing: true,
                label: "3 members".into(),
            }),
            search: true,
            ..Tools::default()
        },
    )
}

/// #general of the first server: threads, notifications, pins, members.
pub fn channel() -> Header {
    open(
        Title::Channel(Channel {
            topic: Some(text::plain(TOPIC)),
            ..general()
        }),
        channel_tools(),
    )
}

/// #general with a topic that does not fit.
pub fn channel_long_topic() -> Header {
    open(
        Title::Channel(Channel {
            topic: Some(long_topic()),
            ..general()
        }),
        channel_tools(),
    )
}

/// The thread "Plans" in #general: its channel before it, "More" after the
/// member list.
pub fn thread() -> Header {
    open(
        Title::Channel(Channel {
            icon: Icon::Thread,
            name: "Plans".into(),
            parent: Some(Parent {
                name: "general".into(),
                forum: false,
            }),
            topic: None,
        }),
        Tools {
            threads: Some(false),
            pins: Some(false),
            members: Some(Members {
                showing: true,
                label: String::new(),
            }),
            more: true,
            search: true,
            ..Tools::default()
        },
    )
}

/// A voice channel's chat: the button back to the stage first.
pub fn voice() -> Header {
    open(
        Title::Channel(Channel {
            icon: Icon::Volume,
            name: "Lounge".into(),
            ..Channel::default()
        }),
        Tools {
            voice_chat: Some(false),
            pins: Some(false),
            members: Some(Members::default()),
            search: true,
            ..Tools::default()
        },
    )
}

/// The Friends screen on its Online tab.
pub fn friends() -> Header {
    open(
        Title::Screen(Screen::Friends { all: false }),
        Tools::default(),
    )
}

/// Message requests.
pub fn requests() -> Header {
    open(Title::Screen(Screen::Requests), Tools::default())
}

/// A server whose channels are browsed.
pub fn browse() -> Header {
    open(Title::Screen(Screen::Browse), Tools::default())
}

fn general() -> Channel {
    Channel {
        name: "general".into(),
        ..Channel::default()
    }
}

fn channel_tools() -> Tools {
    Tools {
        threads: Some(false),
        notify: Some(false),
        pins: Some(false),
        members: Some(Members {
            showing: true,
            label: String::new(),
        }),
        search: true,
        ..Tools::default()
    }
}
