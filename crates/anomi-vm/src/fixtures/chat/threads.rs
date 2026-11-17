//! Samples of the threads popover for tests and shots, after the fake
//! Discord's data: task C23's.

use iced::{Color, Point, Rectangle, Size};
use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::chat::threads::{Person, Row, Thread, Threads};
use crate::fixtures::{avatar, pictures, shell};
use crate::kind::Avatar;
use crate::menu::{Align, Spot};
use crate::shell::Layer;

/// The people's avatars: the fake's pictures by name, the others' tints.
fn face(name: &str) -> Avatar {
    let tint = match name {
        "Alice" => 0xb8a88a,
        "Bob" => 0xc06c84,
        "Carol" => 0x6c9aa8,
        _ => 0x5865f2,
    };
    let mut face = pictures::pictured(avatar(&name[..1], tint), name);
    // In #general Carol shows her server avatar and Dave Discord's default
    // for his id, as in the sidebar's fixtures.
    match name {
        "Carol" => {
            face.picture = Some(pictures::picture(
                "/guilds/1400000000000000100/users/1400000000000000003/avatars/carol-server.png",
            ));
        }
        "Dave" => face.picture = Some(pictures::picture("/embed/avatars/2.png")),
        _ => {}
    }
    face
}

fn person(name: &str) -> Person {
    Person {
        name: name.into(),
        avatar: face(name),
        color: (name == "Bob").then_some(Color::from_rgb8(0x34, 0x98, 0xdb)),
    }
}

/// A thread of the fake's #general that it was last active over 30 days
/// ago in.
fn thread(name: &str, members: &[&str]) -> Thread {
    Thread {
        room: format!("discord:t-{name}"),
        name: name.into(),
        time: tr!(">30d ago").into(),
        members: members.len() as u32,
        member_avatars: members.iter().map(|member| face(member)).collect(),
        ..Thread::default()
    }
}

/// An active thread whose newest message is by `by`.
fn active(name: &str, by: &str, said: &str, members: &[&str]) -> Row {
    Row::Thread(Box::new(Thread {
        person: Some(person(by)),
        text: said.into(),
        ..thread(name, members)
    }))
}

/// An archived thread, started by `owner`.
fn older(name: &str, owner: &str, members: &[&str]) -> Row {
    Row::Thread(Box::new(Thread {
        archived: true,
        person: Some(person(owner)),
        lead: tr!("Started by ").into(),
        time: tr!("Last active {}", tr!(">30d ago")),
        ..thread(name, members)
    }))
}

/// The channel's threads as the fake has them: Plans joined, Book club
/// another active one, and its older ones.
pub fn sections() -> Threads {
    let mut rows = vec![
        Row::Heading(trn!("{n} joined thread", "{n} joined threads", 1).to_uppercase()),
        active("Plans", "Carol", "Count me in", &["Bob", "Carol", "Alice"]),
        Row::Heading(trn!("{n} other active thread", "{n} other active threads", 1).to_uppercase()),
        active(
            "Book club",
            "Bob",
            "Chapter 3 by Friday?",
            &["Carol", "Bob"],
        ),
        Row::Heading(tr!("OLDER THREADS").into()),
    ];
    let owners = ["Bob", "Carol", "Dave", "Bob", "Carol", "Dave"];
    for (at, owner) in owners.iter().enumerate() {
        let name = [
            "Game night",
            "Movie picks",
            "Lunch spots",
            "Bug hunt",
            "Road trip",
            "Retro",
        ][at];
        let members: &[&str] = if at == 4 {
            &["Carol", "Alice"]
        } else if at == 0 {
            &["Bob", "Alice"]
        } else {
            &[owner]
        };
        rows.push(older(&format!("{name} {}", at + 1), owner, members));
    }
    Threads {
        rows,
        searchable: true,
        can_create: true,
        more: true,
        ..Threads::default()
    }
}

/// Nothing found, or nothing yet: a list of no threads, which can start
/// one.
pub fn empty() -> Threads {
    Threads {
        empty: true,
        searchable: true,
        can_create: true,
        ..Threads::default()
    }
}

/// The first page still on its way.
pub fn loading() -> Threads {
    Threads {
        note: tr!("Loading threads...").into(),
        searchable: true,
        can_create: true,
        loading: true,
        ..Threads::default()
    }
}

/// A search that is under way or failed, in the list's place.
pub fn searching() -> Threads {
    Threads {
        note: tr!("Searching...").into(),
        query: "plans".into(),
        searchable: true,
        can_create: true,
        ..Threads::default()
    }
}

/// A search's results: threads with no headings, one of them private.
pub fn found() -> Threads {
    let mut threads = sections();
    threads.rows.retain(|row| matches!(row, Row::Thread(_)));
    threads.rows.truncate(2);
    if let Some(Row::Thread(first)) = threads.rows.first_mut() {
        first.private = true;
    }
    Threads {
        query: "pl".into(),
        more: false,
        ..threads
    }
}

/// A channel the account may not search the threads of, with a page on its
/// way under the list.
pub fn unsearchable() -> Threads {
    Threads {
        searchable: false,
        can_create: false,
        loading: true,
        ..sections()
    }
}

/// #general with the list open under the header's Threads button at
/// (991, 25), as the golden shot has it.
pub fn list_popover() -> Vm {
    with(sections())
}

/// #general with `threads` open under the Threads button.
pub fn with(threads: Threads) -> Vm {
    let button = Rectangle::new(Point::new(976.0, 9.0), Size::new(32.0, 32.0));
    let spot = Spot::below(button).align(Align::End);
    let mut vm = shell::open_at(shell::server_home(), Layer::Threads, spot);
    vm.chat.threads = threads;
    vm
}
