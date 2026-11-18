//! The video's shots: Bob's clip in #general as it plays, and full screen;
//! and the golden `chan/youtube_gif_embed`'s card with a video file for a
//! poster, whose play disc is the cell's.

use anomi_vm::Vm;
use anomi_vm::fixtures::chat::video as fixture;
use anomi_vm::fixtures::shell;
use anomi_vm::sidebar::Place;

use crate::shots::{GOLDEN, Shot};

/// #general showing the golden's YouTube card, its poster a video that
/// hasn't played.
fn youtube() -> Vm {
    let mut vm = shell::server_home();
    let timeline = fixture::youtube(None);
    let room = timeline.room.as_ref().map_or("", |room| room.id.as_str());
    if let Place::Server(server) = &mut vm.sidebar.place {
        for row in &mut server.channels.rows {
            row.selected = row.id == room;
        }
    }
    vm.chat.timeline = timeline;
    vm
}

pub fn all() -> Vec<Shot> {
    fn shot(name: &'static str, vm: fn() -> Vm) -> Shot {
        Shot {
            area: "video",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("youtube_gif_embed", youtube),
        shot("cell_idle", || fixture::in_channel(fixture::idle())),
        shot("cell_paused", || fixture::in_channel(fixture::paused())),
        shot("cell_muted", || fixture::in_channel(fixture::muted())),
        shot("cell_loading", || fixture::in_channel(fixture::loading())),
        shot("cell_failed", || fixture::in_channel(fixture::failed())),
        shot("cell_ended", || fixture::in_channel(fixture::ended())),
        shot("full_screen", fixture::full_screen),
        shot("full_screen_loading", fixture::full_screen_loading),
        shot("full_screen_failed", fixture::full_screen_failed),
        shot("full_screen_upright", fixture::full_screen_upright),
    ]
}
