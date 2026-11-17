//! Samples of a server's Emoji settings for tests and shots, after the fake
//! Discord's data: task X10's.

use crate::Vm;
use crate::fixtures::pictures::{badge, pictured, sunset};
use crate::fixtures::{avatar, settings::server, shell};
use crate::kind::Expected;
use crate::settings::emoji::{Emoji, Row, Table, Upload};
use crate::settings::server::Section;
use crate::shell::Layer;

fn row(id: &str, name: &str, uploader: &str, may_change: bool) -> Row {
    Row {
        id: id.into(),
        name: name.into(),
        picture: Expected::Ready(badge(id, 32)),
        uploader: uploader.into(),
        uploader_avatar: pictured(avatar(&uploader[..1], 0x5865f2), uploader),
        may_change,
    }
}

/// An emoji and an animated one, both Bob's, for someone who may change
/// them: the golden screenshot's.
pub fn emoji() -> Emoji {
    Emoji {
        slots: 50,
        tables: vec![
            Table {
                heading: "Emoji".into(),
                slots: "49 slots available".into(),
                rows: vec![row("1", "talk", "Bob", true)],
            },
            Table {
                heading: "Animated Emoji".into(),
                slots: "49 slots available".into(),
                rows: vec![row("2", "party", "Bob", true)],
            },
        ],
        may_upload: true,
        ..Emoji::default()
    }
}

/// The upload dialog on a picture, named.
pub fn upload() -> Upload {
    Upload {
        preview: Expected::Ready(sunset(128, 128)),
        name: "sunset".into(),
        may_finish: true,
        ..Upload::default()
    }
}

/// The window with the server's settings open on the Emoji.
pub fn page() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = server::server();
    vm.settings.server.section = Section::Emoji;
    // The tables scroll on their own, in the frame's height.
    vm.settings.server.fill = true;
    vm.settings.emoji = emoji();
    vm
}
