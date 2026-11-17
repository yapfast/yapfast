//! Samples of an incoming call and of the question before switching calls
//! for tests and shots, after the fake Discord's data: task V08's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::{avatar, pictures, shell};
use crate::shell::Layer;
use crate::voice::incoming::{Incoming, Switch};

/// Bob's call ringing while another DM is open (`dmcall/incoming.png`).
pub fn ringing() -> Incoming {
    Incoming {
        room: "1500000000000000002".into(),
        name: "Bob".into(),
        detail: tr!("Incoming call").into(),
        avatar: pictures::pictured(avatar("B", 0xc2554d), "Bob"),
        switch: Switch::default(),
    }
}

/// A group's call: the detail names who is in it, and no picture yet.
pub fn group() -> Incoming {
    Incoming {
        room: "1500000000000000010".into(),
        name: "Weekend plans".into(),
        detail: tr!("{} calling", tr!("{} and {} are", "Bob", "Carol")),
        avatar: avatar("W", 0xb86e22),
        switch: Switch::default(),
    }
}

/// A long name that must elide, with the detail wrapping to two lines.
pub fn long() -> Incoming {
    Incoming {
        name: "A group with a name far too long for a card three hundred pixels wide".into(),
        detail: tr!(
            "{} calling",
            tr!(
                "{}, {} and {} are",
                "Bartholomew Montgomery",
                "Carolina Esperanza",
                "Maximilian"
            )
        ),
        ..group()
    }
}

/// In the Lounge, asked to leave it for another channel.
pub fn switching() -> Incoming {
    Incoming {
        switch: Switch {
            text: tr!(
                "You're already in {}. Leave it and join {}?{}",
                "Lounge",
                "Games",
                ""
            ),
            joins: false,
        },
        ..ringing()
    }
}

/// In no call, asked to join one to watch Bob's stream.
pub fn watching() -> Incoming {
    Incoming {
        switch: Switch {
            text: tr!("Join {} to watch {}'s stream?", "Lounge", "Bob"),
            joins: true,
        },
        ..ringing()
    }
}

/// A DM's window with the call ringing over it.
pub fn ringing_window() -> Vm {
    let mut vm = shell::open(shell::dm_home(), Layer::IncomingCall);
    vm.voice.incoming = ringing();
    vm
}

/// A server's window asking whether to switch channels.
pub fn switching_window() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::SwitchCall);
    vm.voice.incoming = switching();
    vm
}
