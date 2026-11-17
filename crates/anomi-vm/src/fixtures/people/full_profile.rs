//! Samples of the full profile for tests and shots, after the fake Discord's
//! data: task P09's. Bob as `profile/full.png` shows him, on his Activity
//! tab with Factorio, with his one mutual friend and server on theirs; and
//! the states a tab can be in: loading, empty, the board and the wishlist,
//! under a profile theme.

use iced::Color;
use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::people::profile;
use crate::fixtures::{avatar, pictures, pictures::pictured, shell};
use crate::kind::{Expected, Presence};
use crate::people::full_profile::{Friend, FullProfile, Game, Page, Recent, Server, Widget, Wish};
use crate::people::profile::Theme;
use crate::shell::Layer;

fn picture(path: &str) -> Expected {
    Expected::Ready(pictures::picture(path))
}

/// Bob's full profile, as the golden screenshot shows it.
pub fn bob() -> FullProfile {
    FullProfile {
        card: profile::side(),
        friends_tab: true,
        servers_tab: true,
        friends_label: trn!("{n} Mutual Friend", "{n} Mutual Friends", 1),
        servers_label: trn!("{n} Mutual Server", "{n} Mutual Servers", 1),
        mutuals_known: true,
        recent_known: true,
        friends: vec![Friend {
            id: "discord:1400000000000000003".into(),
            name: "Carol".into(),
            avatar: pictured(avatar("C", 0xd05b9b), "Carol"),
            presence: Presence::Idle,
        }],
        servers: vec![Server {
            id: "1400000000000000100".into(),
            name: "Server".into(),
            initial: "S".into(),
            icon: pictures::server("Server").map_or(Expected::None, Expected::Ready),
            nick: String::new(),
        }],
        ..FullProfile::default()
    }
}

/// Bob's, on his mutual friends.
pub fn on_friends() -> FullProfile {
    FullProfile {
        chosen: Some(Page::Friends),
        ..bob()
    }
}

/// Bob's, on his mutual servers.
pub fn on_servers() -> FullProfile {
    FullProfile {
        chosen: Some(Page::Servers),
        ..bob()
    }
}

/// A tab Discord hasn't answered for: rows the shape of what's coming.
pub fn loading() -> FullProfile {
    FullProfile {
        chosen: Some(Page::Friends),
        mutuals_known: false,
        friends: Vec::new(),
        friends_label: tr!("Mutual Friends").into(),
        ..bob()
    }
}

/// Bob without activity or anything in common: Discord's words for them.
pub fn empty() -> FullProfile {
    let mut full = FullProfile {
        friends: Vec::new(),
        servers: Vec::new(),
        friends_label: tr!("No Mutual Friends").into(),
        servers_label: tr!("No Mutual Servers").into(),
        ..bob()
    };
    full.card.activities.clear();
    full
}

/// What they did lately, under the activity cards.
pub fn recent() -> FullProfile {
    FullProfile {
        recent: vec![
            Recent {
                title: "Factorio".into(),
                subtitle: "Spaghetti, ready to ship".into(),
                initial: "F".into(),
                picture: picture("/app-assets/factorio.png"),
                music: false,
                when: "Yesterday".into(),
            },
            Recent {
                title: "Lofi beats".into(),
                subtitle: "Someone".into(),
                initial: "L".into(),
                picture: Expected::None,
                music: true,
                when: "3 days ago".into(),
            },
        ],
        ..bob()
    }
}

/// The Board: a widget with two games and one only Discord shows.
pub fn board() -> FullProfile {
    FullProfile {
        board_tab: true,
        chosen: Some(Page::Board),
        widgets: vec![
            Widget {
                title: "Favourite games".into(),
                games: vec![
                    Game {
                        name: "Factorio".into(),
                        initial: "F".into(),
                        icon: picture("/app-assets/factorio.png"),
                        comment: "The factory must grow".into(),
                    },
                    Game {
                        name: "Hades".into(),
                        initial: "H".into(),
                        icon: Expected::None,
                        comment: String::new(),
                    },
                ],
                elsewhere: false,
            },
            Widget {
                title: "Now streaming".into(),
                games: Vec::new(),
                elsewhere: true,
            },
        ],
        ..bob()
    }
}

/// The Wishlist: five items.
pub fn wishlist() -> FullProfile {
    let wish = |name: &str, picture: Expected| Wish {
        name: name.into(),
        initial: name[..1].into(),
        picture,
    };
    FullProfile {
        wishlist_tab: true,
        chosen: Some(Page::Wishlist),
        wishlist: vec![
            wish("Satisfactory", picture("/wishlist/one.png")),
            wish("Dyson Sphere Program", picture("/wishlist/two.png")),
            wish("Hades II", Expected::Waiting),
            wish("Nitro", Expected::None),
            wish(
                "A very long name for a very small game",
                picture("/wishlist/five.png"),
            ),
        ],
        wishlist_note: "Not a gift list.".into(),
        ..bob()
    }
}

/// Bob's under a profile theme, which tints the whole modal.
pub fn themed() -> FullProfile {
    let mut full = bob();
    full.card.theme = Some(Theme::new([
        Color::from_rgb8(0x8b, 0x4b, 0xd8),
        Color::from_rgb8(0x2f, 0x1a, 0x5e),
    ]));
    full
}

/// The window with the full profile open over it.
pub fn open(full: FullProfile) -> Vm {
    let mut vm = shell::server_home();
    vm.people.full_profile = full;
    shell::open(vm, Layer::FullProfile)
}
