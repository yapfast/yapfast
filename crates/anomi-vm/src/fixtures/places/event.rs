//! Samples of a scheduled event's dialog for tests and shots, after the fake
//! Discord's data: task N12's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::places::event::{Event, Found};
use crate::shell::Layer;

/// An event coming up, which the person may mark interesting.
pub fn upcoming() -> Event {
    Event {
        title: tr!("Event").into(),
        found: Some(Found {
            name: "Movie night".into(),
            when: "Fri, 20:00".into(),
            detail: format!("#Lounge · {}", tr!("by {}", "Bob")),
        }),
        description: "Bring snacks.".into(),
        confirm: tr!("Interested").into(),
        ..Event::default()
    }
}

/// Marked interesting: the button takes it back.
pub fn interested() -> Event {
    Event {
        confirm: tr!("Remove interest").into(),
        status: tr!("You're interested in this event.").into(),
        ..upcoming()
    }
}

/// Under way: the status says so.
pub fn going_on() -> Event {
    Event {
        status: tr!("This event is going on now.").into(),
        ..upcoming()
    }
}

/// Over: nothing to mark, and Close alone.
pub fn ended() -> Event {
    Event {
        confirm: String::new(),
        status: tr!("This event has ended.").into(),
        ..upcoming()
    }
}

/// Looking it up: nothing found yet.
pub fn looking() -> Event {
    Event {
        title: tr!("Event").into(),
        status: tr!("Looking up the event...").into(),
        busy: true,
        ..Event::default()
    }
}

/// Saving the change.
pub fn saving() -> Event {
    Event {
        busy: true,
        status: tr!("Saving...").into(),
        ..upcoming()
    }
}

/// The window on the first server with the dialog open on `event`.
pub fn over(event: Event) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Event);
    vm.places.event = event;
    vm
}
