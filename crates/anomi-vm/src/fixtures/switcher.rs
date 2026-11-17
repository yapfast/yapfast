//! Samples of the quick switcher for tests and shots, after the fake
//! Discord's data: task N10's. Its two golden shots, `switcher/empty` and
//! `switcher/query`, are Ctrl+K and then "ob" on the first server's
//! #general.

use yapfast_lang::tr;

use crate::Vm;
use crate::icon::Icon;
use crate::kind::count_badge;
use crate::shell::Layer;
use crate::switcher::{Mark, Place, Switcher};

use super::pictures::{self, pictured};
use super::{avatar, shell};

fn person(id: &str, name: &str, tint: u32) -> Place {
    Place {
        id: id.into(),
        name: name.into(),
        place: tr!("Direct message").into(),
        mark: Mark::Avatar(pictured(avatar(&name[..1], tint), name)),
        ..Place::default()
    }
}

fn channel(id: &str, name: &str, icon: Icon) -> Place {
    Place {
        id: id.into(),
        name: name.into(),
        // The name of the server the channel is in.
        place: "Server".into(),
        mark: Mark::Icon(icon),
        ..Place::default()
    }
}

/// Ctrl+K with nothing typed: Discord's DM with a mention above the places
/// opened lately.
pub fn empty() -> Switcher {
    let mut discord = person("discord:d9", "Discord", 0x7983f5);
    discord.unread = true;
    discord.badge = count_badge(1);
    // The fake's Discord user has no avatar, so the client shows its default
    // one, number 5.
    if let Mark::Avatar(avatar) = &mut discord.mark {
        avatar.picture = Some(pictures::picture("/embed/avatars/5.png"));
    }
    Switcher {
        places: vec![
            discord,
            channel("discord:c3", "ideas", Icon::MessageSquareText),
            channel("discord:c2", "Plans", Icon::Thread),
            person("discord:p5", "Person 5", 0x3a9357),
            person("discord:d2", "Weekend plans", 0xb86e22),
            person("discord:d1", "Bob", 0xc2554d),
        ],
        mentions: 1,
        ..Switcher::default()
    }
}

/// "ob" typed: only Bob, with no headings.
pub fn query() -> Switcher {
    Switcher {
        query: "ob".into(),
        places: vec![person("discord:d1", "Bob", 0xc2554d)],
        ..Switcher::default()
    }
}

/// "zzz" typed: nothing goes by that name.
pub fn nothing() -> Switcher {
    Switcher {
        query: "zzz".into(),
        ..Switcher::default()
    }
}

/// The switcher open over #general with `switcher`.
pub fn open(switcher: Switcher) -> Vm {
    shell::open(
        Vm {
            switcher,
            ..shell::server_home()
        },
        Layer::Switcher,
    )
}

/// Ctrl+K on #general.
pub fn opened() -> Vm {
    open(empty())
}

/// Ctrl+K, then "ob".
pub fn searched() -> Vm {
    open(query())
}
