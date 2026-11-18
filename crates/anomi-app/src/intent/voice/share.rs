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

use iced::Task;
use yapfast_core::stream_worker::{self, Shareable, Source};
use yapfast_core::voice::{self, Command, StreamAudio, StreamQuality};

use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Intent, Resolution, Screen, Share, Sound};

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

/// The share button: list what can be shared, then open the menu; a second
/// click while it lists forgets it.
pub fn open(app: &mut App, spot: Spot) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    if shown.waiting.take().is_some() || app.vm.shell.is_open(Layer::Share) {
        return super::popups::close(app, Layer::Share);
    }
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
    shown.sources = (screens.iter().chain(&windows))
        .map(|listed| listed.source.clone())
        .chain(dialog.then_some(Source::Screen))
        .chain(cameras.iter().map(|listed| listed.source.clone()))
        .collect();
    let voice = &app.core.state.voice;
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
        switching: voice.sharing.is_some(),
        source: (voice.sharing.is_none() || !shown.sources.is_empty()).then_some(0),
        resolution: match voice.quality.resolution {
            voice::Resolution::P720 => Resolution::P720,
            voice::Resolution::P1080 => Resolution::P1080,
            voice::Resolution::Source => Resolution::Source,
        },
        fps: voice.quality.fps,
        high_quality: voice.quality.high_quality,
        sound: match voice.stream_audio {
            StreamAudio::Off => Sound::None,
            _ => Sound::All,
        },
        ..Share::default()
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

/// The menu closed: a listing still waited for is forgotten.
pub fn closed(app: &mut App) -> Task<Message> {
    app.shown.voice.share.waiting = None;
    app.shown.voice.share.glance = None;
    Task::none()
}

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let share = &mut app.vm.voice.share;
    match intent {
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
            let quality = StreamQuality {
                resolution: match resolution {
                    Resolution::P720 => voice::Resolution::P720,
                    Resolution::P1080 => voice::Resolution::P1080,
                    Resolution::Source => voice::Resolution::Source,
                },
                fps: fps.clamp(1, 60),
                high_quality,
            };
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
    Task::none()
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
