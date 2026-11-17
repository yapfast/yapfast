//! Samples of a stream in picture in picture for tests and shots, after the
//! fake Discord's data: task V06's. The card follows what the Lounge's call
//! has: Alice watching Bob's recorded stream (`voice/stream_pip_over_text_channel.png`),
//! Alice sharing her own screen, a stream still connecting, a camera.
//!
//! The pictures are the tiles' (`fixtures::voice::tiles`): the colour bars
//! of Bob's recording, and the people's avatars.

use iced::Vector;
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::sidebar::channels::joined_window;
use crate::fixtures::voice::{dock, tiles};
use crate::fixtures::{chat, conversation, people};
use crate::icon::Icon;
use crate::shell::Layer;
use crate::voice::pip::{Pip, Stop};
use crate::voice::tiles::Viewers;

/// Alice watches Bob's stream, the one viewer (`watch-viewers`).
pub fn watching() -> Pip {
    Pip {
        title: tr!("{}'s screen", "Bob"),
        frame: Some(tiles::frame(640, 360)),
        viewers: Viewers {
            faces: vec![tiles::alice_avatar()],
            count: 1,
            names: tr!("Watching: {}", "Alice"),
        },
        stop: Some(Stop {
            icon: Icon::X,
            label: tr!("Stop watching").into(),
        }),
        ..Pip::default()
    }
}

/// Alice's own screen, live, Bob watching it: it can stop sharing.
pub fn sharing() -> Pip {
    Pip {
        title: tr!("Your screen").into(),
        live: true,
        viewers: Viewers {
            faces: vec![tiles::bob_avatar()],
            count: 1,
            names: tr!("Watching: {}", "Bob"),
        },
        stop: Some(Stop {
            icon: Icon::ScreenShareOff,
            label: tr!("Stop sharing").into(),
        }),
        ..watching()
    }
}

/// Bob's stream before its first picture, five watching it.
pub fn waiting() -> Pip {
    Pip {
        frame: None,
        status: tr!("Connecting to {}'s stream...", "Bob"),
        viewers: Viewers {
            faces: vec![
                tiles::alice_avatar(),
                tiles::carol_avatar(),
                tiles::dave_avatar(),
            ],
            count: 5,
            names: tr!("Watching: {}", "Alice, Carol, Dave, Erin, Person 1"),
        },
        ..watching()
    }
}

/// Bob's camera: nobody counts its watchers and nothing stops it.
pub fn camera() -> Pip {
    Pip {
        title: "Bob".into(),
        frame: Some(tiles::frame(320, 180)),
        viewers: Viewers::default(),
        stop: None,
        ..watching()
    }
}

/// `watching`, taken this far from its corner by a drag.
pub fn dragged() -> Pip {
    Pip {
        drag: Vector::new(-240.0, 120.0),
        ..watching()
    }
}

/// The Lounge's call on the account card, a text channel open, `pip` over it.
fn over(pip: Pip) -> Vm {
    let mut vm = joined_window();
    vm.voice.dock = dock::joined();
    vm.conversation = conversation::channel();
    vm.chat.timeline = chat::timeline::general();
    vm.chat.composer = chat::composer::channel();
    vm.people.members = people::members::server();
    vm.people.activity = people::activity::panel();
    vm.voice.pip = pip;
    vm.shell.open(Layer::Pip);
    vm
}

/// `voice/stream_pip_over_text_channel.png`.
pub fn watching_window() -> Vm {
    over(watching())
}

pub fn waiting_window() -> Vm {
    over(waiting())
}

pub fn sharing_window() -> Vm {
    over(sharing())
}

pub fn camera_window() -> Vm {
    over(camera())
}

/// In the corner it was dragged to, the bottom left.
pub fn bottom_left_window() -> Vm {
    let mut vm = over(watching());
    vm.shell.pip.left = true;
    vm.shell.pip.bottom = true;
    vm
}
