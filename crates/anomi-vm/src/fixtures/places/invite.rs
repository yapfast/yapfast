//! Samples of the invite dialog for tests and shots, after the fake Discord's
//! data: task N12's.

use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, shell};
use crate::icon::Icon;
use crate::places::invite::{Invite, Invitee, Page, Sent};
use crate::shell::Layer;

fn person(name: &str, username: &str, tint: u32) -> Invitee {
    Invitee {
        id: username.into(),
        name: name.into(),
        username: username.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        sent: Sent::Not,
    }
}

/// Friends and DMs not in the server, as the fake's Person rows.
fn friends() -> Vec<Invitee> {
    vec![
        person("Erin", "erin", 0x3a9357),
        person("Person 0", "person0", 0xa84bb8),
        person("Person 2", "person2", 0xc2558f),
        person("Person 4", "person4", 0x5b5fbf),
    ]
}

/// A link made: seven days, no limit, to #general, with friends to invite.
pub fn link() -> Invite {
    Invite {
        title: tr!("Invite friends to {}", "Server"),
        channel: "general".into(),
        icon: Some(Icon::Hash),
        friends: friends(),
        link: "https://discord.gg/qsGhsSbQ".into(),
        note: trn!(
            "Your invite link expires in {n} day.",
            "Your invite link expires in {n} days.",
            7
        ),
        age: 5,
        uses: 0,
        ..Invite::default()
    }
}

/// Discord is making one: no link yet, and Generate waits.
pub fn making() -> Invite {
    Invite {
        link: String::new(),
        note: String::new(),
        making: true,
        ..link()
    }
}

/// Making one failed.
pub fn failed() -> Invite {
    Invite {
        link: String::new(),
        note: String::new(),
        error: "Discord refused to make the link.".into(),
        ..link()
    }
}

/// The link's settings, the second page.
pub fn settings() -> Invite {
    Invite {
        page: Page::Link,
        ..link()
    }
}

/// To a private voice channel: the server's members first, one invited,
/// one on the way, one refused.
pub fn voice() -> Invite {
    let mut members = vec![
        person("Bob", "bob", 0xc2554d),
        person("Carol", "carol", 0xc2554d),
        person("Dave", "dave", 0x3a7bd5),
        person("Mallory", "mallory", 0x8e6b3a),
    ];
    members[0].sent = Sent::Sent;
    members[1].sent = Sent::Sending;
    members[2].sent =
        Sent::Failed("Couldn't send the invite: Cannot send messages to this user".into());
    Invite {
        channel: "Lounge".into(),
        icon: Some(Icon::VolumeLock),
        private: true,
        members,
        ..link()
    }
}

/// A search that found no one.
pub fn nobody() -> Invite {
    Invite {
        query: "zz".into(),
        friends: Vec::new(),
        ..link()
    }
}

/// The window on the first server with the dialog open on `invite`.
pub fn over(invite: Invite) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Invite);
    vm.places.invite = invite;
    vm
}

/// The dialog with the link made (`dialog/invite_to_server.png`).
pub fn made() -> Vm {
    over(link())
}
