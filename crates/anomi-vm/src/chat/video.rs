//! A video in a message, and full screen: what it shows and what it asks for
//! (`VideoCell`, `VideoControls`, `VideoViewer`, `ui/messages.slint:524`,
//! `:412`, `:2731`, over the `VideoPlayer` global, `:344`).
//!
//! One video plays at a time. The app projects the player's state
//! (`show_video`, `src/ui/refresh.rs:5176`) into the cell of the message
//! that plays it ([`Video::player`]) and, while it is large, into
//! [`FullScreen`], held as `vm.chat.video` and drawn over the window as
//! `Layer::Video`. Both hold the same [`Player`], so the video goes full
//! screen and back without starting again. Its picture is
//! [`Player::frame`]: each frame a new handle on the pixels the core wrote
//! (`anomi-app/src/pictures/frames.rs`), never a copy.

use iced::widget::image;

use crate::menu::Spot;

/// How far the arrow keys move a video full screen, in milliseconds
/// (`:2759`).
pub const SKIP_MS: f32 = 5000.0;
/// How far a turn of the wheel over the sound button moves the volume
/// (`:452`).
pub const VOLUME_STEP: f32 = 0.05;
/// The size a video full screen takes until its file says, 16:9 (`:2767`).
pub const NATURAL: (u32, u32) = (1600, 900);

/// A video of a message's gallery or of a card (`VideoCell`): its poster
/// with a play button until it plays here, then its picture and controls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {
    /// The message's key and the attachment's place among its attachments,
    /// which `Intent::Play` names.
    pub item: u64,
    pub index: usize,
    /// The player, while this cell's video is the one playing (or paused,
    /// loading, ended or failed); none shows the poster's play button.
    pub player: Option<Player>,
}

/// The video playing, as `VideoPlayer` has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    /// The last frame; none until the first comes, which leaves the poster
    /// showing.
    pub frame: Option<image::Handle>,
    pub loading: bool,
    pub playing: bool,
    pub ended: bool,
    /// The file has sound: only then is there a mute button.
    pub sound: bool,
    pub muted: bool,
    /// How loud it plays unmuted, from 0 to 1 as the slider stands.
    pub volume: f32,
    /// How far in, from 0 to 1, and as text, such as "0:12 / 1:30".
    pub progress: f32,
    pub time: String,
    /// Why it can't play, in the user's language.
    pub error: Option<String>,
    /// How long it plays, in milliseconds; 0 until known.
    pub duration: u32,
    /// Its pictures' full size in pixels; `(0, 0)` until known.
    pub size: (u32, u32),
}

impl Default for Player {
    fn default() -> Self {
        Self {
            frame: None,
            loading: false,
            playing: false,
            ended: false,
            sound: false,
            muted: false,
            volume: 1.0,
            progress: 0.0,
            time: String::new(),
            error: None,
            duration: 0,
            size: (0, 0),
        }
    }
}

impl Player {
    pub fn failed(&self) -> bool {
        self.error.is_some()
    }

    /// The volume the slider shows: none while muted.
    pub fn shown_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
    }

    /// The volume a turn of the wheel asks for, a step up or down from the
    /// one shown (`:450`).
    pub fn turned(&self, up: bool) -> f32 {
        let step = if up { VOLUME_STEP } else { -VOLUME_STEP };
        (self.shown_volume() + step).clamp(0.0, 1.0)
    }

    /// Where an arrow key moves it, five seconds back or on; none while its
    /// length is unknown (`:2758`).
    pub fn skipped(&self, on: bool) -> Option<f32> {
        (self.duration > 0).then(|| {
            let step = SKIP_MS / self.duration as f32;
            let step = if on { step } else { -step };
            (self.progress + step).clamp(0.0, 1.0)
        })
    }

    /// The size it is drawn at full screen, before it is fitted: the
    /// file's, or 16:9 until known.
    pub fn natural(&self) -> (f32, f32) {
        let (width, height) = match self.size {
            (0, _) | (_, 0) => NATURAL,
            size => size,
        };
        (width as f32, height as f32)
    }
}

impl Video {
    /// The disc with its play triangle shows: the video isn't playing here,
    /// it failed, or it ended and the pointer is off it (`:579`).
    pub fn disc(&self, hovered: bool) -> bool {
        match &self.player {
            None => true,
            Some(player) => player.failed() || (player.ended && !hovered),
        }
    }

    /// Its controls show while the pointer is on it or it's paused, once
    /// it has loaded and unless it failed (`:569`).
    pub fn controls(&self, hovered: bool) -> bool {
        self.player.as_ref().is_some_and(|player| {
            !player.failed() && !player.loading && (hovered || !player.playing)
        })
    }

    /// "Loading..." shows in the middle (`:587`).
    pub fn loading(&self) -> bool {
        self.player
            .as_ref()
            .is_some_and(|player| player.loading && !player.failed())
    }

    /// A click on it plays or pauses the video playing here, and starts it
    /// otherwise, or again after it failed, in a cell `width` by `height`
    /// (`:561`).
    pub fn click(&self, width: f32, height: f32) -> Intent {
        match &self.player {
            Some(player) if !player.failed() => Intent::Toggle,
            _ => Intent::Play {
                item: self.item,
                index: self.index,
                width,
                height,
            },
        }
    }
}

/// A video played full screen (`VideoViewer`). The window holds it as
/// `vm.chat.video`; it shows as `Layer::Video`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FullScreen {
    /// The message's key and the attachment, which Save names.
    pub item: u64,
    pub index: usize,
    /// Its attachment's name, in the bar.
    pub name: String,
    /// What the Save button's tooltip says, "Save clip.mp4" in the user's
    /// language (`@tr("Save {}", VideoPlayer.name)`): the app makes it.
    pub save_tip: String,
    /// It's a link's preview, not a file of the message: no Save.
    pub preview: bool,
    /// Its address on the web, which its menu copies and opens.
    pub link: Option<String>,
    pub player: Player,
}

impl FullScreen {
    /// A right click opens its menu: it has a file to save or an address
    /// (`:2789`).
    pub fn has_menu(&self) -> bool {
        !self.preview || self.link.is_some()
    }
}

/// A video in a message, and full screen: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Play a message's attachment in its cell, `width` by `height` logical
    /// pixels (`VideoPlayer.play`).
    Play {
        item: u64,
        index: usize,
        width: f32,
        height: f32,
    },
    /// Play or pause the video playing (`toggle`): a click on it, its play
    /// button, Space full screen.
    Toggle,
    /// Move this far through it, from 0 to 1 (`seek`).
    Seek(f32),
    /// Mute it, or not (`mute`).
    Mute(bool),
    /// Play it this loud, from 0 to 1, unmuting it; 0 mutes it
    /// (`change-volume`).
    Volume(f32),
    /// Show it full screen, or back in its cell (`expand`). Escape and a
    /// press on the backdrop close `Layer::Video`, which the app takes as
    /// `Expand(false)`.
    Expand(bool),
    /// Full screen shows it this size, in logical pixels (`resize`).
    Resize { width: f32, height: f32 },
    /// Save the file, from the bar full screen (`save-attachment`).
    Save { item: u64, index: usize },
    /// A right click on it full screen, with the pointer in the window: the
    /// app opens the window's context menu there (`vm.menu`) with the rows
    /// of `anomi_chat::viewer::menu::rows(.., true)`.
    Menu(Spot),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Player {
        Player {
            playing: true,
            progress: 0.5,
            duration: 10_000,
            volume: 0.5,
            ..Player::default()
        }
    }

    #[test]
    fn the_arrows_move_five_seconds_within_the_video() {
        let player = player();
        assert_eq!(player.skipped(true), Some(1.0));
        assert_eq!(player.skipped(false), Some(0.0));
        let near = Player {
            progress: 0.9,
            ..player.clone()
        };
        assert_eq!(near.skipped(true), Some(1.0));
        let unknown = Player {
            duration: 0,
            ..player
        };
        assert_eq!(unknown.skipped(true), None);
    }

    #[test]
    fn the_wheel_moves_the_volume_from_where_it_shows() {
        let player = player();
        assert!((player.turned(true) - 0.55).abs() < 1e-6);
        let muted = Player {
            muted: true,
            ..player
        };
        assert_eq!(muted.turned(false), 0.0);
        assert!((muted.turned(true) - VOLUME_STEP).abs() < 1e-6);
    }

    #[test]
    fn full_screen_is_sixteen_by_nine_until_the_file_says() {
        assert_eq!(Player::default().natural(), (1600.0, 900.0));
        let known = Player {
            size: (720, 1280),
            ..Player::default()
        };
        assert_eq!(known.natural(), (720.0, 1280.0));
    }

    #[test]
    fn a_click_starts_a_cell_toggles_the_one_playing_and_retries_a_failure() {
        let idle = Video {
            item: 7,
            index: 1,
            player: None,
        };
        let play = Intent::Play {
            item: 7,
            index: 1,
            width: 400.0,
            height: 225.0,
        };
        assert_eq!(idle.click(400.0, 225.0), play);
        let playing = Video {
            player: Some(player()),
            ..idle.clone()
        };
        assert_eq!(playing.click(400.0, 225.0), Intent::Toggle);
        let failed = Video {
            player: Some(Player {
                error: Some("No".into()),
                ..player()
            }),
            ..idle
        };
        assert_eq!(failed.click(400.0, 225.0), play);
    }

    #[test]
    fn what_a_cell_shows_follows_the_pointer_and_the_player() {
        let playing = Video {
            player: Some(player()),
            ..Video::default()
        };
        assert!(!playing.disc(false) && !playing.controls(false));
        assert!(playing.controls(true));
        let ended = Video {
            player: Some(Player {
                playing: false,
                ended: true,
                ..player()
            }),
            ..Video::default()
        };
        assert!(ended.disc(false) && !ended.disc(true));
        assert!(ended.controls(false));
        let loading = Video {
            player: Some(Player {
                loading: true,
                ..player()
            }),
            ..Video::default()
        };
        assert!(loading.loading() && !loading.controls(true));
        assert!(Video::default().disc(true));
    }
}
