//! Samples of joining a voice channel or a DM's call for tests and shots,
//! after the fake Discord's data: task V07's.

use yapfast_lang::tr;

use crate::Vm;
use crate::conversation::{Body, Call as Presence, Conversation};
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, conversation, header, shell};
use crate::kind::Avatar;
use crate::voice::join::{Call, Channel, Join};

/// The voice channel Lounge, not joined, encrypted.
pub fn channel() -> Channel {
    Channel {
        name: "Lounge".into(),
        description: tr!("{} here. {}", "Bob", tr!("Join and hang out.")),
        encrypted: true,
        can_join: true,
        ..Channel::default()
    }
}

/// Another voice channel is joined: the button switches.
pub fn switching() -> Channel {
    Channel {
        switching: true,
        ..channel()
    }
}

/// Without encryption: the notice says Anomi won't join, and the button is
/// off.
pub fn unencrypted() -> Channel {
    Channel {
        encrypted: false,
        can_join: false,
        ..channel()
    }
}

/// Locked by the person's roles.
pub fn locked() -> Channel {
    Channel {
        description: tr!("Your roles on this Discord server don't let you join this channel.")
            .into(),
        can_join: false,
        ..channel()
    }
}

/// The local preview: no encryption notice.
pub fn preview() -> Channel {
    Channel {
        preview: true,
        encrypted: false,
        ..channel()
    }
}

fn bob() -> Avatar {
    pictured(avatar("B", 0xd9_7b_8c), "Bob")
}

/// Bob is in the DM's call (`light/dm_bob.png`).
pub fn call() -> Call {
    Call {
        people: vec![bob()],
        note: tr!("{} in the call.", "Bob"),
        room: "bob".into(),
        can_join: true,
        ..Call::default()
    }
}

/// Two people in it.
pub fn crowd() -> Call {
    Call {
        people: vec![bob(), pictured(avatar("C", 0x6f_9a_a8), "Carol")],
        note: tr!("{} in the call.", "Bob, Carol"),
        ..call()
    }
}

/// Bob is calling: Decline and Accept (`dmcall/incoming.png`'s DM).
pub fn ringing() -> Call {
    Call {
        note: tr!("{} calling you.", "Bob"),
        incoming: true,
        ..call()
    }
}

/// A ring nobody has joined yet.
pub fn ringing_empty() -> Call {
    Call {
        people: Vec::new(),
        note: tr!("Incoming call.").into(),
        ..ringing()
    }
}

/// The voice channel's body with `channel` as its join screen.
pub fn channel_window(channel: Channel) -> Vm {
    let mut vm = shell::channel_with(|| Conversation {
        body: Body::Voice,
        call: Presence::Elsewhere,
        ..Conversation::default()
    });
    vm.header = header::voice();
    vm.voice.join = Join::Channel(channel);
    vm
}

/// Bob's DM with `call` above its messages.
pub fn call_window(call: Call) -> Vm {
    let mut vm = shell::dm_with(conversation::dm_call_elsewhere);
    vm.header = header::dm_activity();
    vm.voice.join = Join::Call(call);
    vm
}
