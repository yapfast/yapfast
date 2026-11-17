//! Samples of adding people to a DM for tests and shots, after the fake
//! Discord's data: task P13's. A one-to-one DM with Bob, whose friends
//! Alice, Carol and Erin and a few more are not in it yet.

use crate::fixtures::{avatar, pictures::pictured};
use crate::kind::Presence;
use crate::people::add_people::{AddPeople, Person};

fn person(id: &str, name: &str, tint: u32, presence: Presence) -> Person {
    Person {
        id: id.into(),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        presence,
        chosen: false,
    }
}

fn friends() -> Vec<Person> {
    vec![
        person("alice", "Alice", 0x5865f2, Presence::Online),
        person("carol", "Carol", 0x3a9357, Presence::Mobile),
        person("erin", "Erin", 0xc27a2c, Presence::Idle),
        person("dave", "Dave", 0x9a5c6a, Presence::DoNotDisturb),
        person("frank", "Frank", 0x3f8fd8, Presence::Offline),
    ]
}

/// A one-to-one DM, as it opens: nobody chosen, the group's room used up
/// by two people so far (eight more).
pub fn empty() -> AddPeople {
    AddPeople {
        people: friends(),
        room: 8,
        one_to_one: true,
        ..AddPeople::default()
    }
}

/// Alice and Erin chosen.
pub fn chosen() -> AddPeople {
    let mut people = friends();
    people[0].chosen = true;
    people[2].chosen = true;
    AddPeople {
        people,
        chosen: 2,
        ..empty()
    }
}

/// A group DM whose room one pick used up: the chosen is the only row that
/// answers, the rest are dim.
pub fn at_limit() -> AddPeople {
    let mut people = friends();
    people[1].chosen = true;
    AddPeople {
        people,
        room: 1,
        chosen: 1,
        one_to_one: false,
        ..empty()
    }
}

/// A group DM with no room at all.
pub fn no_room() -> AddPeople {
    AddPeople {
        room: 0,
        one_to_one: false,
        ..empty()
    }
}

/// Typed "al": one friend left.
pub fn searched() -> AddPeople {
    let mut people = friends();
    people.truncate(1);
    AddPeople {
        query: "al".into(),
        people,
        ..empty()
    }
}

/// The query matches no one.
pub fn no_match() -> AddPeople {
    AddPeople {
        query: "zed".into(),
        people: Vec::new(),
        ..empty()
    }
}

/// Every friend is in the DM already.
pub fn all_here() -> AddPeople {
    AddPeople {
        people: Vec::new(),
        ..empty()
    }
}

/// Group made: the request is on its way.
pub fn adding() -> AddPeople {
    AddPeople {
        adding: true,
        ..chosen()
    }
}

/// Discord refused.
pub fn failed() -> AddPeople {
    AddPeople {
        note: "Missing Permissions".into(),
        ..chosen()
    }
}
