//! Samples of a video in a message, and full screen for tests and shots,
//! after the fake Discord's data: task C09's. Bob's clip in #general, a
//! 400 by 225 cell whose poster is a still and whose frame is the sunset,
//! a minute and a half long; and the same clip full screen.

use yapfast_lang::tr;

use crate::Vm;
use crate::chat::message::Message;
use crate::chat::timeline::group::key;
use crate::chat::timeline::{Row, Timeline};
use crate::chat::video::{FullScreen, Player, Video};
use crate::fixtures::chat::{attachments, embeds, message::bobs, timeline};
use crate::fixtures::{pictures, shell};
use crate::shell::Layer;
use crate::text::Source;

/// The key the clip's message carries, which its intents name, and the
/// attachment it is.
pub const ITEM: u64 = attachments::ITEM;
pub const INDEX: usize = 0;
/// The cell, as `attachments::video` sizes it.
pub const CELL: (f32, f32) = (400.0, 225.0);

/// Playing, twelve seconds in, with sound at 80%.
pub fn player() -> Player {
    Player {
        frame: Some(pictures::sunset(640, 360)),
        playing: true,
        sound: true,
        volume: 0.8,
        progress: 12.0 / 90.0,
        time: "0:12 / 1:30".into(),
        duration: 90_000,
        size: (1280, 720),
        ..Player::default()
    }
}

fn with(player: Player) -> Video {
    Video {
        player: Some(player),
        ..idle()
    }
}

/// Not playing here: its poster's play button.
pub fn idle() -> Video {
    Video {
        item: ITEM,
        index: INDEX,
        player: None,
    }
}

pub fn playing() -> Video {
    with(player())
}

/// Paused: its controls show without the pointer.
pub fn paused() -> Video {
    with(Player {
        playing: false,
        ..player()
    })
}

/// Asked to play, before its first frame: the poster and "Loading...".
pub fn loading() -> Video {
    with(Player {
        frame: None,
        loading: true,
        playing: false,
        progress: 0.0,
        time: String::new(),
        duration: 0,
        size: (0, 0),
        ..player()
    })
}

/// It couldn't play: why, and the play button to try again.
pub fn failed() -> Video {
    with(Player {
        playing: false,
        error: Some("Couldn't play: the file is damaged".into()),
        ..player()
    })
}

/// Played to its end: its last frame and the play button.
pub fn ended() -> Video {
    with(Player {
        playing: false,
        ended: true,
        progress: 1.0,
        time: "1:30 / 1:30".into(),
        ..player()
    })
}

/// Paused and muted.
pub fn muted() -> Video {
    with(Player {
        playing: false,
        muted: true,
        ..player()
    })
}

/// A file without sound: no mute button.
pub fn silent() -> Video {
    with(Player {
        playing: false,
        sound: false,
        ..player()
    })
}

/// #general with Bob's clip under the rest, playing as `video` has it.
pub fn in_channel(video: Video) -> Vm {
    let mut clip = attachments::video();
    if let Some(cell) = clip.gallery.cells.first_mut() {
        cell.video = Some(video);
    }
    let message = Message {
        key: key("clip"),
        body: crate::text::markdown(Source::new("The bridge from the ride")),
        attachments: clip,
        time: "01:10".into(),
        short_time: "01:10".into(),
        ..bobs(30, true)
    };
    let mut general = timeline::general();
    general.rows.push(Row {
        key: message.key,
        first: true,
        message,
        ..Row::default()
    });
    let mut vm = shell::server_home();
    vm.chat.timeline = Timeline {
        shown: None,
        unread: None,
        ..general
    };
    vm
}

/// #general as the golden `chan/youtube_gif_embed` has it, the YouTube
/// card's poster a video file that plays in its cell as `player` has it.
pub fn youtube(player: Option<Player>) -> Timeline {
    let mut timeline = embeds::showcase(true);
    if let Some(row) = timeline.rows.first_mut() {
        let item = row.message.embeds.item;
        let cards = &mut row.message.embeds.cards;
        let cells = cards.iter_mut().filter_map(|card| card.gallery.as_mut());
        for cell in cells.flat_map(|gallery| gallery.cells.iter_mut()) {
            cell.playing = Some(Video {
                item,
                index: cell.index,
                player: player.clone(),
            });
        }
    }
    timeline
}

/// The clip full screen, playing.
pub fn full_screen_of(player: Player) -> FullScreen {
    FullScreen {
        item: ITEM,
        index: INDEX,
        name: "clip.mp4".into(),
        save_tip: tr!("Save {}", "clip.mp4"),
        preview: false,
        link: Some("https://cdn.discordapp.com/attachments/1/2/clip.mp4".into()),
        player,
    }
}

pub fn full_screen_video() -> FullScreen {
    full_screen_of(player())
}

/// A link's preview full screen: no Save.
pub fn preview_video() -> FullScreen {
    FullScreen {
        preview: true,
        ..full_screen_video()
    }
}

/// Filmed upright, narrower than the controls' least width.
pub fn upright_video() -> FullScreen {
    full_screen_of(Player {
        frame: Some(pictures::sunset(360, 640)),
        size: (720, 1280),
        ..player()
    })
}

/// The window with `full` over #general.
pub fn open(full: FullScreen) -> Vm {
    let mut vm = in_channel(with(full.player.clone()));
    vm.chat.video = full;
    shell::open(vm, Layer::Video)
}

pub fn full_screen() -> Vm {
    open(full_screen_video())
}

pub fn full_screen_loading() -> Vm {
    let Some(player) = loading().player else {
        return full_screen();
    };
    open(full_screen_of(player))
}

pub fn full_screen_failed() -> Vm {
    let Some(player) = failed().player else {
        return full_screen();
    };
    open(full_screen_of(player))
}

pub fn full_screen_upright() -> Vm {
    open(upright_video())
}
