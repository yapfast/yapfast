//! Samples of adding people to a DM for tests and shots, after the fake
//! Discord's data: task P13's. Alice's DM with Person 5 (`shell::dm_home()`),
//! and the friends of hers who are not in it: Bob, Carol and Person 0, 2 and
//! 4, as `friends.rs` has them.

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
    let mut people = vec![
        person(
            "1400000000000000002",
            "Bob",
            0xc2554d,
            Presence::DoNotDisturb,
        ),
        person("1400000000000000003", "Carol", 0x3a9357, Presence::Idle),
    ];
    for n in [0, 2, 4] {
        people.push(person(
            &format!("160000000000000000{n}"),
            &format!("Person {n}"),
            0x3a9357,
            Presence::Offline,
        ));
    }
    people
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

/// Bob and Person 0 chosen.
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

/// Typed "bo": one friend left.
pub fn searched() -> AddPeople {
    let mut people = friends();
    people.truncate(1);
    AddPeople {
        query: "bo".into(),
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
