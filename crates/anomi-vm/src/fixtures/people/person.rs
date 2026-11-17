//! Samples of about a person: nickname and note for tests and shots, after
//! the fake Discord's data and the words `src/ui/refresh.rs:3600` gives each
//! ask: task P11's.

use yapfast_lang::{tr, trn};

use crate::people::person::{Person, choices, tick};

/// A note on Bob, empty and waiting for it (`PersonAsk::Note`).
pub fn note() -> Person {
    Person {
        title: tr!("Note on {}", "Bob"),
        text: tr!("Only you see your note, in Discord too.").into(),
        field: tr!("Note").into(),
        confirm: tr!("Save").into(),
        ..Person::default()
    }
}

/// The note with something written.
pub fn note_typed() -> Person {
    Person {
        text_now: "Met at the meetup".into(),
        ..note()
    }
}

/// The note on its way from Discord: the field is disabled.
pub fn loading() -> Person {
    Person {
        loading: true,
        ..note()
    }
}

/// Saving it.
pub fn saving() -> Person {
    Person {
        saving: true,
        ..note_typed()
    }
}

/// Discord refused.
pub fn failed() -> Person {
    Person {
        error: "Missing Access".into(),
        ..note_typed()
    }
}

/// A friend's nickname (`PersonAsk::Nickname`).
pub fn nickname() -> Person {
    Person {
        title: tr!("Nickname for {}", "Bob"),
        text: tr!("Only you see the nickname you give a friend. Leave it empty to remove it.")
            .into(),
        field: tr!("Nickname").into(),
        text_now: "Bobby".into(),
        confirm: tr!("Save").into(),
        ..Person::default()
    }
}

/// Before a call, with its box (`PersonAsk::Call`): no field.
pub fn call() -> Person {
    Person {
        title: tr!("Ready to start a call?").into(),
        text: tr!("Enjoy this moment. Then jump in when you're ready.").into(),
        tick: Some(tick(tr!("Don't ask again"), false)),
        confirm: tr!("Start Call").into(),
        ..Person::default()
    }
}

/// Before unfriending (`PersonAsk::Unfriend`): a red button.
pub fn unfriend() -> Person {
    Person {
        title: tr!("Remove {} as a friend?", "Bob"),
        text: tr!(
            "You'll need to send {} a friend request to be friends again.",
            "Bob"
        ),
        confirm: tr!("Remove friend").into(),
        danger: true,
        ..Person::default()
    }
}

/// A timeout in a server (`PersonAsk::Timeout`): lengths to pick from, and a
/// reason.
pub fn timeout() -> Person {
    Person {
        title: tr!("Time out {}", "Bob"),
        text: tr!(
            "Until it ends, {} can read the server but can't send messages, react, or join voice. The reason goes in the server's audit log.",
            "Bob"
        ),
        options_heading: tr!("DURATION").into(),
        options: choices(
            [
                trn!("{n} second", "{n} seconds", 60),
                trn!("{n} minute", "{n} minutes", 5),
                trn!("{n} minute", "{n} minutes", 10),
                trn!("{n} hour", "{n} hours", 1),
                trn!("{n} day", "{n} days", 1),
                trn!("{n} week", "{n} weeks", 1),
            ],
            2,
        ),
        field: tr!("Reason (optional)").into(),
        text_now: String::new(),
        confirm: tr!("Time out").into(),
        danger: true,
        ..Person::default()
    }
}

/// A ban (`PersonAsk::Ban`): how much of their history goes.
pub fn ban() -> Person {
    Person {
        title: tr!("Ban {}?", "Bob"),
        text: tr!(
            "{} can't join again until unbanned. The reason goes in the server's audit log.",
            "Bob"
        ),
        options_heading: tr!("DELETE THEIR MESSAGES FROM").into(),
        options: choices(
            [
                tr!("Don't delete any").to_owned(),
                tr!("The last hour").to_owned(),
                trn!("The last {n} hour", "The last {n} hours", 6),
                trn!("The last {n} hour", "The last {n} hours", 12),
                trn!("The last {n} hour", "The last {n} hours", 24),
                trn!("The last {n} day", "The last {n} days", 3),
                trn!("The last {n} day", "The last {n} days", 7),
            ],
            0,
        ),
        field: tr!("Reason (optional)").into(),
        confirm: tr!("Ban").into(),
        danger: true,
        ..Person::default()
    }
}
