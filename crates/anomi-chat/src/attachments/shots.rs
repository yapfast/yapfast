//! The shots of a message's attachments: the pictures, files and stickers
//! in #general, the same with moving GIFs and stickers, and the golden
//! `chan/showcase_emoji_stickers_gif`'s stickers, GIF and spoiler.

use anomi_vm::Vm;
use anomi_vm::chat::timeline::Timeline;
use anomi_vm::fixtures::chat::attachments as fixtures;
use anomi_vm::fixtures::shell;
use anomi_widgets::picture::sample::bouncing;

use crate::shots::{GOLDEN, Shot};
use anomi_vm::chat::attachments::Cell;
use anomi_vm::kind::Expected;

fn showcase() -> Vm {
    let mut vm = shell::server_home();
    vm.chat.timeline = fixtures::showcase();
    vm
}

/// Every picture of the timeline that moves, moving: its GIFs, and its
/// stickers but the first.
fn moving() -> Vm {
    playing(fixtures::moving())
}

/// The golden showcase's stickers, GIF and spoiler.
fn stickers() -> Vm {
    playing(fixtures::showcase_stickers())
}

fn playing(mut timeline: Timeline) -> Vm {
    let mut vm = shell::server_home();
    for row in &mut timeline.rows {
        let attachments = &mut row.message.attachments;
        let cells: &mut [Cell] = &mut attachments.gallery.cells;
        for cell in cells.iter_mut().filter(|cell| cell.gif.is_some()) {
            let side = cell.width.min(240);
            cell.picture = Expected::Ready(bouncing(side, 10, [250, 190, 70]));
        }
        let stickers = attachments.stickers.iter_mut().skip(1);
        for (sticker, colour) in stickers.zip([[120, 190, 250], [140, 20, 20]]) {
            if matches!(sticker.picture, Expected::Ready(_)) {
                sticker.picture = Expected::Ready(bouncing(160, 10, colour));
            }
        }
    }
    vm.chat.timeline = timeline;
    vm
}

pub fn all() -> Vec<Shot> {
    let shot = |area, name, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("attachments", "showcase", showcase),
        shot("attachments", "moving", moving),
        shot("chan", "showcase_emoji_stickers_gif", stickers),
    ]
}
