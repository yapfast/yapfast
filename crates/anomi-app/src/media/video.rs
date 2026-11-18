//! The video playing in a message: in its cell and full screen
//! (`show_video`, `src/ui/refresh.rs:4966`; `VideoPlayer`'s callbacks,
//! `src/ui/callbacks.rs:3426`-`:3527`).
//!
//! Rows are built only as their message changes; the player is put on the
//! one cell playing, in place, after the timeline showed, and taken off any
//! other. Its frames come through [`super::frames`]: the `Live` it holds is
//! the same while it plays, so a frame changes no view-model; only the
//! first and a step of its bar build the view again.

use std::sync::Arc;

use iced::Task;
use yapfast_core::application::{Action, AppState};
use yapfast_core::chat::{self, VideoCommand};
use yapfast_core::frames::Frames;
use yapfast_lang::tr;

use anomi_vm::Vm;
use anomi_vm::chat::timeline::{Timeline, group};
use anomi_vm::chat::video::{FullScreen, Intent, Player};
use anomi_vm::kind::Live;
use anomi_vm::shell::Layer;

use super::frames::{Clock, Source};
use super::time;
use crate::app::App;
use crate::message::Message;

/// The video the window follows.
struct Playing {
    item: String,
    index: usize,
    /// Its row's key, while the core has its message.
    key: Option<u64>,
    frames: Option<Arc<Frames>>,
    live: Live,
    clock: Arc<Clock>,
}

/// What the window keeps of the video playing.
#[derive(Default)]
pub struct Shown {
    playing: Option<Playing>,
    /// Its cell's size in logical pixels, which its frames fit again when
    /// it leaves full screen (`video_cell`).
    cell: (f32, f32),
    /// It shows full screen (`VideoPlayer.expanded`), at this size.
    expanded: bool,
    full: Option<(f32, f32)>,
    /// Its pictures wait to come at another size until it plays again: a
    /// size asked of an ended video shows its last picture again, which
    /// keeps the worker from starting over on Play.
    size_due: bool,
}

impl Shown {
    /// A video plays, or did, in the open room.
    pub fn plays(&self) -> bool {
        self.playing.is_some()
    }
}

/// Shows the core's video in its cell and full screen; the source of its
/// frames while it plays where it can be seen.
pub fn show(state: &AppState, vm: &mut Vm, shown: &mut Shown) -> Option<Source> {
    let video = state.video.as_ref();
    let same = matches!((&shown.playing, video),
        (Some(playing), Some(video)) if playing.item == video.item && playing.index == video.index);
    if !same {
        shown.playing = video.map(|video| Playing {
            item: video.item.clone(),
            index: video.index,
            key: None,
            frames: None,
            live: Live::default(),
            clock: Arc::default(),
        });
        shown.size_due = false;
        // Another video, or none: full screen goes.
        if std::mem::take(&mut shown.expanded) {
            vm.shell.close(Layer::Video);
            shown.full = None;
        }
    }
    let (Some(video), Some(playing)) = (video, shown.playing.as_mut()) else {
        place(&mut vm.chat.timeline, None, None);
        place(&mut vm.chat.thread_panel.timeline, None, None);
        return None;
    };
    playing.key = state
        .message(&video.item)
        .map(|message| group::key(&message.id));
    let started = match (&playing.frames, &video.frames) {
        (Some(was), Some(now)) => !Arc::ptr_eq(was, now),
        (None, now) => now.is_some(),
        (Some(_), None) => false,
    };
    if started {
        // Played again: its bar starts over; the last frame shows until
        // the first of this playing.
        playing.clock = Arc::default();
    }
    playing.frames.clone_from(&video.frames);
    let player = player(state, video, playing);
    let at = playing.key.map(|key| (key, video.index));
    place(&mut vm.chat.timeline, at, Some(&player));
    place(&mut vm.chat.thread_panel.timeline, at, Some(&player));
    let seen = shown.expanded || at.is_some_and(|(key, _)| in_view(vm, key));
    if shown.expanded {
        full(state, video, playing, player, &mut vm.chat.video);
    }
    let frames = playing.frames.clone().filter(|_| seen)?;
    Some(Source::Mapped(
        frames,
        playing.live.clone(),
        playing.clock.clone(),
    ))
}

/// The player as `VideoPlayer` has it.
fn player(state: &AppState, video: &chat::Video, playing: &Playing) -> Player {
    let attachment =
        (state.message(&video.item)).and_then(|message| message.attachments.get(video.index));
    let size = video
        .size
        .or_else(|| attachment.and_then(|attachment| attachment.dimensions))
        .unwrap_or_default();
    let duration = video.duration_ms.filter(|&duration| duration > 0);
    // The last picture shows a little before the end.
    let at = match duration {
        Some(duration) if video.ended => duration,
        _ => playing.clock.time(),
    };
    let progress = duration.map_or(0.0, |duration| {
        (at as f64 / duration as f64).min(1.0) as f32
    });
    Player {
        frame: playing.live.has_frame().then(|| playing.live.clone()),
        loading: video.loading,
        playing: video.playing,
        ended: video.ended,
        sound: video.sound,
        muted: video.muted,
        volume: video.volume,
        progress,
        time: time::video_time(at, video.duration_ms),
        error: video.error.clone(),
        duration: duration.map_or(0, |duration| u32::try_from(duration).unwrap_or(u32::MAX)),
        size,
    }
}

/// Puts `player` on the cell `at` (a row's key and an attachment) and takes
/// it off any other, writing only what differs.
fn place(timeline: &mut Timeline, at: Option<(u64, usize)>, player: Option<&Player>) {
    for row in &mut timeline.rows {
        let key = row.key;
        let message = &mut row.message;
        let cells = message.attachments.gallery.cells.iter_mut();
        let videos = cells.filter_map(|cell| cell.video.as_mut());
        let cards = message.embeds.cards.iter_mut();
        let playing = cards
            .filter_map(|card| card.gallery.as_mut())
            .flat_map(|gallery| gallery.cells.iter_mut())
            .filter_map(|cell| cell.playing.as_mut());
        for video in videos.chain(playing) {
            let wanted = player.filter(|_| at == Some((key, video.index)));
            if video.player.as_ref() != wanted {
                video.player = wanted.cloned();
            }
        }
    }
}

/// Whether the row `key` is in view in the timeline or the thread beside;
/// a list that hasn't told what's in view yet shows it.
fn in_view(vm: &Vm, key: u64) -> bool {
    let shows = |timeline: &Timeline| {
        let Some(at) = timeline.index_of(key) else {
            return false;
        };
        let Some(span) = timeline.shown else {
            return true;
        };
        match (timeline.index_of(span.top), timeline.index_of(span.bottom)) {
            (Some(top), Some(bottom)) => (top..=bottom).contains(&at),
            _ => false,
        }
    };
    shows(&vm.chat.timeline) || shows(&vm.chat.thread_panel.timeline)
}

/// The video full screen: its name, Save, its address and the player.
fn full(
    state: &AppState,
    video: &chat::Video,
    playing: &Playing,
    player: Player,
    shown: &mut FullScreen,
) {
    let attachment =
        (state.message(&video.item)).and_then(|message| message.attachments.get(video.index));
    let name = attachment.map_or("", |attachment| attachment.name.as_str());
    if shown.name != name {
        shown.name = name.to_owned();
        shown.save_tip = tr!("Save {}", name);
    }
    shown.item = playing.key.unwrap_or_default();
    shown.index = video.index;
    shown.preview = state.preview;
    let link = attachment.and_then(|attachment| attachment.url.as_deref());
    if shown.link.as_deref() != link {
        shown.link = link.map(str::to_owned);
    }
    if shown.player != player {
        shown.player = player;
    }
}

/// `side` logical pixels as the window's, at the display's scale and the
/// zoom.
fn pixels((width, height): (f32, f32)) -> (u32, u32) {
    let scale = anomi_widgets::picture::scale();
    let side = |side: f32| (side * scale).round().max(1.0) as u32;
    (side(width), side(height))
}

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let video = app.core.state.video.as_ref();
    let (plays, playing, duration) = (
        video.is_some(),
        video.is_some_and(|video| video.playing),
        video.and_then(|video| video.duration_ms),
    );
    match intent {
        Intent::Play {
            item: key,
            index,
            width,
            height,
        } => {
            let Some(item) = (app.shown.timeline.message(key)).map(|message| message.item.clone())
            else {
                return Task::none();
            };
            app.shown.media.video.cell = (width, height);
            let (width, height) = pixels((width, height));
            app.dispatch(Action::PlayVideo {
                item,
                index,
                width,
                height,
            })
        }
        Intent::Toggle if playing => app.dispatch(Action::Video(VideoCommand::Pause)),
        Intent::Toggle => {
            let play = app.dispatch(Action::Video(VideoCommand::Play));
            if !std::mem::take(&mut app.shown.media.video.size_due) {
                return play;
            }
            // After Play, so the worker starts over first.
            Task::batch([play, send_size(app, side(&app.shown.media.video))])
        }
        Intent::Seek(fraction) => match duration {
            Some(duration) => {
                let at = (f64::from(fraction.clamp(0.0, 1.0)) * duration as f64) as u64;
                app.dispatch(Action::Video(VideoCommand::Seek(at)))
            }
            None => Task::none(),
        },
        Intent::Mute(muted) => app.dispatch(Action::Video(VideoCommand::Mute(muted))),
        Intent::Volume(volume) => app.dispatch(Action::Video(VideoCommand::Volume(volume))),
        Intent::Expand(true) => {
            if app.shown.media.video.playing.is_some() {
                app.shown.media.video.expanded = true;
                app.vm.shell.open(Layer::Video);
                app.show(crate::project::Reach::VIDEO);
            }
            Task::none()
        }
        Intent::Expand(false) => collapse(app),
        // Full screen, its pictures come at its size (`resize`).
        Intent::Resize { width, height } => {
            let shown = &mut app.shown.media.video;
            if !shown.expanded || shown.full == Some((width, height)) || !plays {
                return Task::none();
            }
            shown.full = Some((width, height));
            size(app, (width, height))
        }
        Intent::Save { item: _, index } => match &app.shown.media.video.playing {
            Some(playing) => {
                let item = playing.item.clone();
                crate::intent::chat::save::choose_item(app, &item, index)
            }
            None => Task::none(),
        },
        Intent::Menu(spot) => super::viewer::video_menu(app, spot),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The video's pictures come at `side` logical pixels; an ended video's
/// once it plays again.
fn size(app: &mut App, side: (f32, f32)) -> Task<Message> {
    if side.0 <= 0.0 || side.1 <= 0.0 {
        return Task::none();
    }
    if app
        .core
        .state
        .video
        .as_ref()
        .is_some_and(|video| video.ended)
    {
        app.shown.media.video.size_due = true;
        return Task::none();
    }
    send_size(app, side)
}

fn send_size(app: &mut App, side: (f32, f32)) -> Task<Message> {
    if side.0 <= 0.0 || side.1 <= 0.0 {
        return Task::none();
    }
    let (width, height) = pixels(side);
    app.dispatch(Action::Video(VideoCommand::Size { width, height }))
}

/// Where its pictures show: full screen, or its cell.
fn side(shown: &Shown) -> (f32, f32) {
    match shown.full {
        Some(full) if shown.expanded => full,
        _ => shown.cell,
    }
}

/// Back in its cell from full screen (the bar's button, a click beside it,
/// Escape): its pictures come at the cell's size again.
pub fn collapse(app: &mut App) -> Task<Message> {
    app.vm.shell.close(Layer::Video);
    let shown = &mut app.shown.media.video;
    shown.full = None;
    if !std::mem::take(&mut shown.expanded) || app.core.state.video.is_none() {
        return Task::none();
    }
    let cell = shown.cell;
    size(app, cell)
}

/// Full screen played its way out: what it showed goes.
pub fn gone(app: &mut App) {
    if app.vm.shell.opened(Layer::Video).is_none() {
        app.vm.chat.video = FullScreen::default();
    }
}

/// The display's scale or the zoom changed: the pictures come at the new
/// number of pixels (084240b6: soft video at 200%).
pub fn rescaled(app: &mut App) -> Task<Message> {
    let shown = &app.shown.media.video;
    if shown.playing.is_none() || app.core.state.video.is_none() {
        return Task::none();
    }
    size(app, side(shown))
}

/// The item and attachment playing, for its menu.
pub fn playing(shown: &Shown) -> Option<(&str, usize)> {
    (shown.playing.as_ref()).map(|playing| (playing.item.as_str(), playing.index))
}
