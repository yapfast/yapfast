//! Samples of the conversation's header. Task N07's.

use crate::header::{
    AddPeople, Call, Channel, Direct, Friend, Header, Members, Parent, Place, Screen, Title, Tools,
};
use crate::icon::Icon;
use crate::kind::{Presence, Status};
use crate::text::{self, Link, Mention, Source, Text};
use yapfast_lang::{tr, trn};

use super::avatar;
use super::pictures::pictured;

fn open(title: Title, tools: Tools) -> Header {
    Header::Open(Box::new(Place { title, tools }))
}

/// #general's topic as the fake sets it (`testing/fake_discord.ts`): a
/// mention of Bob, a channel link, bold text and an address. Long enough to
/// be cut short in a narrow header.
pub fn topic() -> Text {
    let markdown = "Co-op nights, plans and links. Ask @Bob to join, keep #general on topic, \
                    and read the **house rules** at https://example.com/rules first.";
    let at = |tag: &str| vec![markdown.find(tag).unwrap_or_default()];
    let mentions = [
        Mention {
            text: "@Bob".into(),
            target: Link::User("bob".into()),
            color: None,
            at: at("@Bob"),
        },
        Mention {
            text: "#general".into(),
            target: Link::Channel("general".into()),
            color: None,
            at: at("#general"),
        },
    ];
    text::markdown(Source {
        mentions: &mentions,
        cards: true,
        ..Source::new(markdown)
    })
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
            activity: Some(Status {
                text: tr!("Playing {}", "Factorio"),
                ..Status::default()
            }),
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
                label: trn!("{n} member", "{n} members", 3),
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
            topic: Some(topic()),
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

/// A voice channel's stage: the button to its chat, and how many are here,
/// with no member list to open (`voice/stage`).
pub fn voice() -> Header {
    open(
        Title::Channel(Channel {
            icon: Icon::Volume,
            name: "Lounge".into(),
            ..Channel::default()
        }),
        Tools {
            voice_chat: Some(false),
            count: trn!("{n} member", "{n} members", 4),
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

/// Direct messages with nothing open: the sidebar's title stands for the
/// place.
pub fn sidebar_title() -> Header {
    open(
        Title::Sidebar(tr!("Direct messages").into()),
        Tools::default(),
    )
}

/// Bob's DM in a small window, as the app draws it (`small/dm_conversation`):
/// his activity beside him, the call and video buttons, pins, adding people
/// and the profile, no friend button, and room enough for the 10 px gaps.
pub fn dm_narrow() -> Header {
    let Header::Open(mut place) = dm_activity() else {
        unreachable!("an open place");
    };
    place.tools.friend = Friend::None;
    place.tools.pins = Some(false);
    place.tools.call = Some(Call {
        going: false,
        can: true,
    });
    Header::Open(place)
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
