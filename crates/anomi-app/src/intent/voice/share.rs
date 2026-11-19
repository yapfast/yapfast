//! Sharing the screen: the share menu, opened at its button once what can
//! be shared is listed (`list_share_sources`, `src/ui/callbacks.rs:3104`),
//! and its picks, which go live or switch what's shared (`go_live`,
//! `switch_live`, `stream_sound_changed`). Listing starts a capturer for
//! screens and one for windows and looks at each camera, so it runs on the
//! core's blocking pool; the menu opens for the listing it waited for, and
//! a second click, Escape or leaving forgets it (upstream's 084240b6). On a
//! Wayland session the system's dialog picks the screen or window: the
//! menu's first choice asks it, before the cameras. The screens' pictures
//! come while the menu shows (`glance`).
//!
//! While live the same buttons, and the gears on "Your screen" and on this
//! account's stream's tile, open the stream's own menu at once, as
//! Discord's client does: a choice of frame rate, resolution, encoding or
//! sound changes the stream at once (`Command::Quality`, which the Slint
//! menu didn't offer) and the menu stays open; "Change Windows" lists the
//! sources in its place, or, where only the system's dialog picks, opens
//! it straight away as Slint's `show-share-menu` did; "Stop Streaming"
//! ends the stream.

use iced::Task;
use yapfast_core::stream_worker::{self, Shareable, Source};
use yapfast_core::voice::{self, Command, SharePhase, StreamAudio, StreamQuality};

use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Intent, Live, Resolution, Screen, Share, Sound};

use super::command;
use crate::app::App;
use crate::message::{Message, Once};

/// The listing the menu waits for, and what the menu's sources are.
#[derive(Default)]
pub struct Shown {
    /// The listing waited for, by its number, and where the menu opens.
    waiting: Option<(u64, Spot)>,
    asked: u64,
    /// The sources in the menu's order: screens, windows, the dialog,
    /// cameras.
    pub(super) sources: Vec<Source>,
    /// The screens' pictures being taken for the menu of that listing.
    glance: Option<(u64, iced::task::Handle)>,
}

impl Shown {
    /// Whether the menu of listing `asked` shows and takes its pictures.
    pub(super) fn glancing(&self, asked: u64) -> bool {
        self.glance
            .as_ref()
            .is_some_and(|(glancing, _)| *glancing == asked)
    }
}

/// What the listing found.
#[derive(Clone, Debug)]
pub struct Listed(pub u64, pub Once<Vec<Shareable>>);

/// The share button: while live, the stream's own menu at once; else what
/// can be shared is listed, then the menu opens. A second click closes it,
/// or forgets the listing it waits for.
pub fn open(app: &mut App, spot: Spot) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    if shown.waiting.take().is_some() || app.vm.shell.is_open(Layer::Share) {
        return super::popups::close(app, Layer::Share);
    }
    let voice = &app.core.state.voice;
    if let Some(sharing) = &voice.sharing {
        let live = Live::new(sharing.phase == SharePhase::Live);
        let share = Share {
            live: Some(live),
            ..picks(voice)
        };
        app.vm.voice.share = anomi_voice::share::built(share);
        app.vm.shell.open_at(Layer::Share, spot);
        return Task::none();
    }
    list(app, spot)
}

/// Lists what can be shared, for the menu to open at `spot` once it's in.
fn list(app: &mut App, spot: Spot) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    shown.asked += 1;
    let asked = shown.asked;
    shown.waiting = Some((asked, spot));
    // Listed as the task runs, not before.
    let runtime = app.runtime.clone();
    let listing = async move { runtime.spawn_blocking(stream_worker::shareable).await };
    Task::perform(listing, move |listed| {
        Message::Voice(super::Due::Listed(Listed(
            asked,
            Once::new(listed.unwrap_or_default()),
        )))
    })
}

/// The listing came: the menu opens, if it still waits for this one.
pub fn listed(app: &mut App, Listed(asked, listed): Listed) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    let Some((waited, spot)) = shown.waiting else {
        return Task::none();
    };
    let Some(listed) = listed.take().filter(|_| waited == asked) else {
        return Task::none();
    };
    shown.waiting = None;
    let (mut screens, mut windows, mut cameras) = (Vec::new(), Vec::new(), Vec::new());
    for listed in listed {
        match listed.source {
            Source::Display(_) => screens.push(listed),
            Source::Camera(_) => cameras.push(listed),
            _ => windows.push(listed),
        }
    }
    let dialog = stream_worker::portal_picks();
    let nothing = screens.is_empty() && windows.is_empty() && cameras.is_empty();
    if dialog && nothing && app.core.state.voice.sharing.is_some() {
        // Only the system's dialog picks what to switch to: it opens now.
        stream_worker::pick(Source::Screen);
        let task = super::popups::close(app, Layer::Share);
        return Task::batch([task, command(app, Command::SwitchLive)]);
    }
    shown.sources = (screens.iter().chain(&windows))
        .map(|listed| listed.source.clone())
        .chain(dialog.then_some(Source::Screen))
        .chain(cameras.iter().map(|listed| listed.source.clone()))
        .collect();
    let titles = |listed: &[Shareable]| -> Vec<String> {
        listed.iter().map(|listed| listed.title.clone()).collect()
    };
    let share = Share {
        screens: (screens.iter())
            .map(|listed| Screen {
                title: listed.title.clone(),
                picture: None,
            })
            .collect(),
        windows: titles(&windows),
        cameras: titles(&cameras),
        dialog,
        ..picks(&app.core.state.voice)
    };
    app.vm.voice.share = anomi_voice::share::built(share);
    app.vm.shell.open_at(Layer::Share, spot);
    let shown = &app.shown.voice.share;
    let Some((pictures, handle)) = super::glance::start(app, asked, &shown.sources) else {
        return Task::none();
    };
    app.shown.voice.share.glance = Some((asked, handle));
    pictures
}

/// The menu's picks as the call has them: the stream's quality and sound.
/// While live nothing is picked, so Switch waits for a pick and a change
/// of sound or quality switches nothing (`open-share-menu`).
fn picks(voice: &voice::State) -> Share {
    Share {
        switching: voice.sharing.is_some(),
        source: voice.sharing.is_none().then_some(0),
        resolution: menu_resolution(voice.quality.resolution),
        fps: voice.quality.fps,
        high_quality: voice.quality.high_quality,
        sound: match voice.stream_audio {
            StreamAudio::Off => Sound::None,
            _ => Sound::All,
        },
        ..Share::default()
    }
}

/// The menu's resolution for the core's.
pub(crate) fn menu_resolution(resolution: voice::Resolution) -> Resolution {
    match resolution {
        voice::Resolution::P720 => Resolution::P720,
        voice::Resolution::P1080 => Resolution::P1080,
        voice::Resolution::Source => Resolution::Source,
    }
}

/// The menu closed: a listing still waited for is forgotten.
pub fn closed(app: &mut App) -> Task<Message> {
    app.shown.voice.share.waiting = None;
    app.shown.voice.share.glance = None;
    Task::none()
}

/// After each update: the menu of a stream follows it. It closes as the
/// stream ends; in the stream's menu "Change Windows" waits until the
/// stream is live, and the sound follows the dock's button.
pub fn follow(app: &mut App) -> Task<Message> {
    let share = &app.vm.voice.share;
    if !share.switching || !app.vm.shell.is_open(Layer::Share) {
        return Task::none();
    }
    let voice = &app.core.state.voice;
    let Some(sharing) = &voice.sharing else {
        return super::popups::close(app, Layer::Share);
    };
    let Some(live) = &share.live else {
        return Task::none();
    };
    let changeable = sharing.phase == SharePhase::Live;
    let silent = voice.stream_audio == StreamAudio::Off;
    if live.changeable == changeable && (share.sound == Sound::None) == silent {
        return Task::none();
    }
    let share = &mut app.vm.voice.share;
    if let Some(live) = share.live.as_mut() {
        live.changeable = changeable;
    }
    if (share.sound == Sound::None) != silent {
        share.sound = if silent { Sound::None } else { Sound::All };
    }
    rebuild(app);
    Task::none()
}

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let share = &mut app.vm.voice.share;
    let retuned = matches!(
        intent,
        Intent::Resolution(_) | Intent::Fps(_) | Intent::HighQuality(_)
    );
    match intent {
        Intent::Hover(_) | Intent::Key(_) => {
            // A key that chose a row does as a click on it would.
            let chosen = (share.live.as_mut()).and_then(|live| live.apply(&intent));
            return match chosen {
                Some(chosen) => handle(app, chosen),
                None => Task::none(),
            };
        }
        Intent::Change => {
            let spot = (app.vm.shell.opened(Layer::Share)).and_then(|opened| opened.at);
            return list(app, spot.unwrap_or_default());
        }
        Intent::Stop => {
            let task = super::popups::close(app, Layer::Share);
            return Task::batch([task, command(app, Command::StopLive)]);
        }
        Intent::Source(source) => share.source = Some(source),
        Intent::Resolution(resolution) => share.resolution = resolution,
        Intent::Fps(fps) => share.fps = fps,
        Intent::HighQuality(on) => share.high_quality = on,
        Intent::Apps(open) => share.apps_open = open,
        Intent::Sound(sound) => {
            share.sound = sound;
            let audio = match sound {
                Sound::None => StreamAudio::Off,
                Sound::All | Sound::App(_) => StreamAudio::All,
            };
            if audio != StreamAudio::Off {
                app.shown.voice.heard = audio;
            }
            rebuild(app);
            return command(app, Command::StreamAudio(audio));
        }
        Intent::GoLive {
            source,
            resolution,
            fps,
            high_quality,
        } => {
            pick(app, source, true);
            let quality = quality(resolution, fps, high_quality);
            let task = super::popups::close(app, Layer::Share);
            return Task::batch([task, command(app, Command::GoLive(quality))]);
        }
        Intent::Switch(source) => {
            pick(app, source, false);
            let task = super::popups::close(app, Layer::Share);
            return Task::batch([task, command(app, Command::SwitchLive)]);
        }
        intent => tracing::debug!(?intent, "not the share menu's"),
    }
    rebuild(app);
    let share = &app.vm.voice.share;
    if retuned && share.switching {
        let quality = quality(share.resolution, share.fps, share.high_quality);
        return command(app, Command::Quality(quality));
    }
    Task::none()
}

/// The core's quality for the menu's choices.
fn quality(resolution: Resolution, fps: u32, high_quality: bool) -> StreamQuality {
    StreamQuality {
        resolution: match resolution {
            Resolution::P720 => voice::Resolution::P720,
            Resolution::P1080 => voice::Resolution::P1080,
            Resolution::Source => voice::Resolution::Source,
        },
        fps: fps.clamp(1, 60),
        high_quality,
    }
}

/// What's shared next: the source at `index` of the menu; going live
/// without one asks the system's dialog.
fn pick(app: &App, index: usize, live: bool) {
    match app.shown.voice.share.sources.get(index) {
        Some(source) => stream_worker::pick(source.clone()),
        None if live => stream_worker::pick(Source::Screen),
        None => {}
    }
}

fn rebuild(app: &mut App) {
    let share = std::mem::take(&mut app.vm.voice.share);
    app.vm.voice.share = anomi_voice::share::built(share);
}
