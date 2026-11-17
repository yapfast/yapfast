//! Samples of the profile card for tests and shots, after the fake Discord's
//! data (`testing/fake_discord.ts:1059`): task P07's. Bob's card from the
//! member list and in his DM, Carol's with her banner colour and status,
//! one's own, a themed one, and the full profile's side. Pictures are the
//! fake's; the decoration, effect and frame that move or are drawn by the
//! widgets are the shots' to add.

use iced::{Color, Point, Rectangle, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::kind::bob_tag;
use crate::fixtures::people::{activity, badges, members};
use crate::fixtures::{avatar, pictures, pictures::pictured, shell};
use crate::kind::{Doing, Expected, NameStyle, Presence, Status, Tag};
use crate::menu::Spot;
use crate::people::activity::card::Card as Activity;
use crate::people::profile::{
    Card, Connection, Friendship, Joined, Mutual, Place, Profile, Role, Theme, connection_icon,
};
use crate::shell::Layer;
use crate::text::{self, Source, Spoilers, Text};

/// Bob's and Carol's user ids in the fake.
const BOB: &str = "1400000000000000002";
const CAROL: &str = "1400000000000000003";

/// The Mods role's blue (the fake's role colour `3447003`).
const MODS: Color = Color::from_rgb(
    0x34 as f32 / 255.0,
    0x98 as f32 / 255.0,
    0xdb as f32 / 255.0,
);

/// Where the member list's rows start, and the popout's top against a row:
/// the window's `profile-y - 16px` (`ui/yapfast.slint:5035`).
const MEMBERS_X: f32 = 1135.0;
const ROW: Size = Size::new(265.0, 42.0);

fn bio(markdown: &str) -> Text {
    let covered = |_| false;
    text::markdown(Source {
        markdown,
        mentions: &[],
        emoji: &[],
        spoilers: Spoilers::Covered(&covered),
        sections: None,
        cards: true,
    })
}

/// Bob as the member list's popout shows him: his server bio, his game,
/// one friend and one server in common, the Mods role, the message box.
pub fn bob() -> Card {
    Card {
        id: format!("discord:{BOB}"),
        name: "Bob".into(),
        avatar: pictured(avatar("B", 0xc2554d), "Bob"),
        name_style: NameStyle::new(14, 3, &[0x00ff99]),
        presence: Presence::DoNotDisturb,
        handle: "bob".into(),
        pronouns: "he/him".into(),
        tag: Some(Tag {
            badge: Expected::Ready(pictures::picture(
                "/clan-badges/1400000000000000100/bob.png",
            )),
            ..bob_tag()
        }),
        banner: Expected::Ready(pictures::picture(&format!("/banners/{BOB}/bob-banner.png"))),
        // The fake's game has no details, state or small picture.
        activities: vec![Activity {
            details: String::new(),
            state: String::new(),
            small: Expected::None,
            small_text: String::new(),
            ..activity::game()
        }],
        now: activity::NOW,
        badges: badges::few(),
        bio: Some(bio("Keeps #general tidy.")),
        roles: vec![Role {
            name: "Mods".into(),
            color: Some(MODS),
        }],
        created: "Jul 30, 2025".into(),
        joined: Some(Joined {
            date: "Aug 2, 2025".into(),
            server: "Server".into(),
            initial: "S".into(),
            icon: pictures::server("Server").map_or(Expected::None, Expected::Ready),
        }),
        mutual: Mutual {
            faces: vec![pictured(avatar("C", 0xd05b9b), "Carol")],
            friends: 1,
            servers: 1,
        },
        note: Some(String::new()),
        friend: Friendship::Friends,
        can_message: true,
        message_hint: tr!("Message @{}", "Bob"),
        ..Card::default()
    }
}

/// Bob beside his DM: his own bio with its link, no server, the dates.
pub fn bob_docked() -> Card {
    Card {
        name_style: None,
        bio: Some(bio(
            "Plays bass in a **ska** band. Ask me about https://example.com/gigs",
        )),
        roles: Vec::new(),
        joined: None,
        message_hint: String::new(),
        ..bob()
    }
}

/// Carol with her banner colour, her status in the bubble, her song, one
/// server in common and the Leads role, which has no colour.
pub fn carol() -> Card {
    let mut avatar = pictured(avatar("C", 0xd05b9b), "Carol");
    // Her avatar for the server (`testing/fake_discord.ts:959`).
    avatar.picture = Some(pictures::picture(&format!(
        "/guilds/1400000000000000100/users/{CAROL}/avatars/carol-server.png"
    )));
    Card {
        id: format!("discord:{CAROL}"),
        name: "Carol".into(),
        avatar,
        name_style: NameStyle::new(3, 2, &[0xff7eb3, 0x7a5cff]),
        presence: Presence::Idle,
        handle: "carol".into(),
        accent: Some(Color::from_rgb8(0x58, 0x65, 0xf2)),
        // The fake's moon is a Unicode emoji in the words.
        thought: Some(Status {
            text: "\u{1f319} back later\nafter dinner, probably".into(),
            emoji: Expected::None,
            doing: Doing::Nothing,
        }),
        activities: vec![activity::song()],
        now: activity::NOW,
        roles: vec![Role {
            name: "Leads".into(),
            color: None,
        }],
        mutual: Mutual {
            servers: 1,
            ..Mutual::default()
        },
        note: Some(String::new()),
        friend: Friendship::Friends,
        can_message: true,
        message_hint: tr!("Message @{}", "Carol"),
        ..Card::default()
    }
}

/// One's own card from the account: Edit Profile, no mutual line, no
/// friendship and no box.
pub fn own() -> Card {
    Card {
        id: "discord:1400000000000000001".into(),
        name: "Alice".into(),
        avatar: pictured(avatar("A", 0x9c8a6a), "Alice"),
        presence: Presence::Online,
        handle: "alice".into(),
        bio: Some(bio("Hi, I'm **Alice**.")),
        own: true,
        ..Card::default()
    }
}

/// A Nitro theme, light: pink to sky blue, as the editor's sample.
pub fn themed() -> Card {
    Card {
        theme: Some(Theme::new([
            Color::from_rgb8(0xf5, 0xa9, 0xc8),
            Color::from_rgb8(0x9a, 0xd0, 0xf5),
        ])),
        banner: Expected::None,
        friend: Friendship::Offer,
        ..bob()
    }
}

/// A card still coming, and someone with a pending friend request.
pub fn loading() -> Card {
    Card {
        id: "discord:m5".into(),
        name: "Member 5".into(),
        avatar: pictured(avatar("M", 0x3a9357), "Member 5"),
        presence: Presence::Online,
        handle: "member5".into(),
        remark: tr!("Loading the profile...").into(),
        friend: Friendship::Sent,
        message_hint: tr!("Message @{}", "Member 5"),
        ..Card::default()
    }
}

/// Bob as the full profile's side shows him: Message, the friendship and
/// the menu under his name, both dates, the friendship's, roles, his
/// accounts and a note.
pub fn side() -> Card {
    Card {
        bio: bob_docked().bio,
        badges: badges::many(9),
        friends_since: "Mar 3, 2026".into(),
        connections: vec![
            Connection {
                kind: "steam".into(),
                icon: connection_icon("steam"),
                name: "bobthebuilder".into(),
                url: "https://steamcommunity.com/id/bob".into(),
            },
            Connection {
                kind: "github".into(),
                icon: connection_icon("github"),
                name: "bob".into(),
                url: String::new(),
            },
        ],
        note: Some("Owes me a pizza.".into()),
        ..bob()
    }
}

fn popout(card: Card, top: f32) -> Vm {
    let mut vm = shell::server_home();
    vm.people.activity = activity::panel();
    vm.people.members = members::server();
    vm.people.profile = Profile { card, panel: None };
    let row = Rectangle::new(Point::new(MEMBERS_X, top), ROW);
    shell::open_at(vm, Layer::Profile, Spot::beside(row))
}

/// `profile/popout_member.png`: Bob's card beside his row.
pub fn popout_member() -> Vm {
    popout(bob(), 337.0)
}

/// `profile/popout_carol.png`: Carol's card beside hers.
pub fn popout_carol() -> Vm {
    popout(carol(), 408.0)
}

/// `profile/popout_menu.png`: Bob's card with his menu open under its
/// "...", the rows the member list's menu has.
pub fn popout_menu() -> Vm {
    let menu = members::context().menu;
    let mut vm = popout_member();
    vm.menu = menu;
    // The card's right edge is the row's left less 8 px; the button sits
    // 10 px in and 10 px down, 32 px a side.
    let x = MEMBERS_X - 8.0 - 10.0 - 32.0;
    vm.menu.at = Spot::below(Rectangle::new(Point::new(x, 347.0), Size::new(32.0, 32.0)));
    vm
}

/// `dm/conversation_profile.png`: Bob's DM with his card docked beside it.
/// The panel's caller draws `Profile::panel`; until it calls the panel's
/// view, the popout's card is Bob's as well, in the panel's place.
pub fn dm_panel() -> Vm {
    let mut vm = shell::dm_home();
    vm.conversation.beside.profile = true;
    vm.people.profile = Profile {
        card: bob_docked(),
        panel: Some(bob_docked()),
    };
    vm
}

/// The profile cards for a test: `card` in its `place`.
pub fn at(place: Place, card: Card) -> Profile {
    match place {
        Place::Panel => Profile {
            card: Card::default(),
            panel: Some(card),
        },
        _ => Profile { card, panel: None },
    }
}
