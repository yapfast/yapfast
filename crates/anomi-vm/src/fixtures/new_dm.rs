//! Samples of the new DM and group DM dialog for tests and shots, after the
//! fake Discord's data: task N11's.

use yapfast_lang::tr;

use crate::fixtures::avatar;
use crate::fixtures::pictures::pictured;
use crate::new_dm::{NewDm, Person};

fn friend(name: &str, tint: u32) -> Person {
    Person {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.into(),
        detail: tr!("Discord friend").into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        discord: false,
    }
}

/// An empty field over the five friends the fake lists, as the golden
/// screenshot has it.
pub fn friends() -> NewDm {
    NewDm {
        people: vec![
            friend("Bob", 0xc2554d),
            friend("Carol", 0xc2554d),
            friend("Person 0", 0xa84bb8),
            friend("Person 2", 0xc2558f),
            friend("Person 4", 0x5b5fbf),
        ],
        ..NewDm::default()
    }
}

/// "Bo" typed: one found.
pub fn found() -> NewDm {
    let mut found = friends();
    found.query = "Bo".into();
    found.people.truncate(1);
    found
}

/// Someone from Matrix typed by id, beside a Discord friend.
pub fn matrix() -> NewDm {
    let mut found = found();
    found.matrix = true;
    found.query = "@bo".into();
    // Both networks list people here, so Discord's friend carries its mark.
    found.people[0].discord = true;
    found.people.push(Person {
        id: "@bo:example.org".into(),
        name: "bo".into(),
        detail: "@bo:example.org".into(),
        avatar: avatar("B", 0x3a9357),
        discord: false,
    });
    found
}

/// Nothing typed and no one listed: what to type.
pub fn hint() -> NewDm {
    NewDm {
        note: tr!("Type a name to find people you know on Discord.").into(),
        ..NewDm::default()
    }
}

/// A query nobody matches.
pub fn nobody() -> NewDm {
    NewDm {
        query: "zzz".into(),
        note: tr!("No one found.").into(),
        ..NewDm::default()
    }
}

/// Bob chosen, the conversation on its way: the rows are still.
pub fn starting() -> NewDm {
    NewDm {
        starting: true,
        note: tr!("Starting the conversation...").into(),
        ..friends()
    }
}

/// The last try failed.
pub fn failed() -> NewDm {
    NewDm {
        note: "Couldn't start the conversation.".into(),
        ..friends()
    }
}
