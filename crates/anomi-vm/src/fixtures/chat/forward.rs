//! Samples of forwarding a message for tests and shots, after the fake
//! Discord's data: task C28's. There is no golden shot of the dialog; these
//! are the Slint one's states.

use yapfast_lang::tr;

use crate::chat::forward::{Forward, Mark, Target};
use crate::fixtures::avatar;
use crate::fixtures::pictures::{pictured, server};
use crate::icon::Icon;
use crate::kind::Avatar;

/// What the fake's message says, quoted.
const QUOTE: &str = "Anyone up for the weekend trip? I found a cabin by the lake.";
/// `yapfast_core::application::FORWARD_LIMIT`.
const LIMIT: usize = 5;

fn person(id: &str, name: &str, tint: u32) -> Target {
    Target {
        id: id.into(),
        name: name.into(),
        place: tr!("Direct message").into(),
        mark: Mark::Avatar(pictured(avatar(&name[..1], tint), name)),
        chosen: false,
    }
}

fn channel(id: &str, name: &str, icon: Icon) -> Target {
    Target {
        id: id.into(),
        name: name.into(),
        place: tr!("Channel").into(),
        mark: Mark::Icon(icon),
        chosen: false,
    }
}

fn server_icon() -> Mark {
    Mark::Avatar(Avatar {
        picture: server("Server"),
        ..avatar("S", 0x5b5fbf)
    })
}

fn targets() -> Vec<Target> {
    let mut general = channel("discord:c1", "general", Icon::Hash);
    general.mark = server_icon();
    vec![
        person("discord:d1", "Bob", 0xc2554d),
        person("discord:d2", "Carol", 0xc2554d),
        person("discord:d3", "Weekend plans", 0xb86e22),
        general,
        channel("discord:c2", "voice", Icon::Volume),
        channel("discord:c3", "ideas", Icon::MessageSquareText),
        channel("discord:c4", "Plans", Icon::Thread),
        person("discord:d4", "Person 0", 0xa84bb8),
    ]
}

fn chosen(mut forward: Forward, ids: &[&str]) -> Forward {
    for target in &mut forward.targets {
        target.chosen = ids.contains(&target.id.as_str());
    }
    forward.chosen = ids.len();
    forward.send = match ids.len() {
        0 | 1 => tr!("Send").into(),
        n => tr!("Send ({})", n),
    };
    forward
}

/// Just opened: the places, none chosen, nothing written.
pub fn open() -> Forward {
    Forward {
        quote: QUOTE.into(),
        targets: targets(),
        limit: LIMIT,
        send: tr!("Send").into(),
        ..Forward::default()
    }
}

/// Two places chosen and a note written.
pub fn chosen_two() -> Forward {
    Forward {
        note: "Thought of you both".into(),
        ..chosen(open(), &["discord:d1", "discord:c1"])
    }
}

/// One place chosen: the button reads "Send".
pub fn chosen_one() -> Forward {
    chosen(open(), &["discord:d2"])
}

/// The most chosen: the places left out are dimmed and take no press.
pub fn at_limit() -> Forward {
    chosen(
        open(),
        &[
            "discord:d1",
            "discord:d2",
            "discord:d3",
            "discord:c1",
            "discord:c4",
        ],
    )
}

/// "Bo" searched: one place found.
pub fn found() -> Forward {
    let mut forward = open();
    forward.query = "Bo".into();
    forward.targets.truncate(1);
    forward
}

/// A search that found nothing.
pub fn nothing_found() -> Forward {
    Forward {
        query: "zzz".into(),
        targets: Vec::new(),
        empty: tr!("We searched far and wide. Unfortunately, no results were found.").into(),
        ..open()
    }
}

/// Matrix, with no encrypted conversation to forward into.
pub fn nowhere() -> Forward {
    Forward {
        targets: Vec::new(),
        empty: tr!("There's nowhere to forward to yet. Yapfast forwards Matrix messages only into encrypted conversations.").into(),
        ..open()
    }
}

/// A note written for a place that has slowmode.
pub fn slowmode() -> Forward {
    Forward {
        note: "See you there".into(),
        slowmode: tr!(
            "The message you added will not be sent to slowmode channel: {}.",
            "Server #general"
        ),
        ..chosen(open(), &["discord:c1"])
    }
}

/// The last try failed.
pub fn failed() -> Forward {
    Forward {
        error: "Couldn't forward to Bob.".into(),
        ..chosen(open(), &["discord:d1"])
    }
}
