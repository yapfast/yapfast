//! The embeds' shots: the golden screenshots' cards on #general.

use anomi_vm::Vm;
use anomi_vm::fixtures::chat::embeds;
use anomi_vm::fixtures::shell;
use anomi_vm::sidebar::Place;

use crate::shots::{GOLDEN, Shot};

/// #general showing `embeds::showcase(video)`.
fn channel(video: bool) -> Vm {
    let mut vm = shell::server_home();
    let timeline = embeds::showcase(video);
    let room = timeline.room.as_ref().map_or("", |room| room.id.as_str());
    if let Place::Server(server) = &mut vm.sidebar.place {
        for row in &mut server.channels.rows {
            row.selected = row.id == room;
        }
    }
    vm.chat.timeline = timeline;
    vm
}

/// The status bot's card (`chan/bot_embed_card_poll`, whose poll is C07's)
/// and a YouTube link's card over a card of pictures
/// (`chan/youtube_gif_embed`, whose animated picture is an attachment).
pub fn shots() -> Vec<Shot> {
    vec![
        Shot {
            area: "embeds",
            name: "bot_embed_card",
            size: GOLDEN,
            theme: "bonfire",
            vm: || channel(false),
        },
        Shot {
            area: "embeds",
            name: "youtube_embed",
            size: GOLDEN,
            theme: "bonfire",
            vm: || channel(true),
        },
    ]
}
