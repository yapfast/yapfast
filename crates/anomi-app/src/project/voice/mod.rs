//! The call from the core's state: the dock and the controls (`dock`), the
//! way in and the calls ringing (`join`), the stage's tiles (`tiles`, in
//! the order and focus of `focus`) and the corner, with their moving
//! pictures (`lives`). Task I18 (docs/iced/tasks/integration.md); the
//! Slint side is `show_voice` and `show_stage` (`src/ui/refresh.rs:4331`,
//! `:4382`). It runs under `Reach::VOICE` (who speaks, who's in, the
//! phase), the state and the pictures; a camera's first frame, `VIDEO`.
//!
//! A frame changes nothing here: it goes to its `Live` and asks a redraw
//! (`crate::media`). Only the tiles whose inputs changed are built again,
//! and nothing runs for the call while the window is behind or gone: the
//! feeds are told they don't show, the frames aren't followed, the clock
//! holds, and coming back says it all once.

pub mod devices;
mod dock;
mod focus;
mod join;
pub mod link;
mod lives;
mod meter;
mod soundboard;
#[cfg(test)]
pub(crate) mod tests;
mod tiles;

use yapfast_core::application::Application;
use yapfast_lang::tr;

use anomi_vm::Vm;
use anomi_vm::icon::Icon;
use anomi_vm::shell::Layer;
use anomi_vm::voice::pip::Stop;
use anomi_vm::voice::stage::{Placement, Stage as Tiles};

pub use devices::{devices as device_menu, open_meter};
pub use focus::{Focus, Pip, Stage};
pub use meter::Meter;
pub use soundboard::soundboard;

use crate::layout::Layout;
use crate::media::Media;
use crate::pictures::Requests;

/// What the call showed, and what the app keeps of it beside the core: the
/// tile shown large, full screen or with the others hidden, where the grid
/// placed its tiles, the call's timer and its pictures.
#[derive(Default)]
pub struct Shown {
    pub focus: Option<Focus>,
    watched: Option<String>,
    pub others_hidden: bool,
    pub placement: Option<Placement>,
    /// The corner shows this, and its tile is at this slot.
    pub pip: Option<(Pip, Option<usize>)>,
    pub timer: dock::Timer,
    tiles: tiles::Shown,
    pub lives: lives::Lives,
    /// The input menu's meter, while it shows in front, and the devices
    /// its menu was made from.
    pub meter: Option<Meter>,
    pub devices: Option<(usize, Option<String>, bool)>,
    /// Each device's name as the menus listed it, by id: a chosen one gone
    /// away shows by the name it had (`device_rows`' `names`).
    pub device_names: std::collections::HashMap<String, String>,
    /// The sound the stream carried before it was turned off, and the
    /// noise suppression before the dock's button turned it off.
    pub heard: yapfast_core::voice::StreamAudio,
    pub noise_last: yapfast_core::noise::Suppressor,
    /// The privacy code was copied then: "Copied" for two seconds.
    pub copied: Option<std::time::Instant>,
    /// The share menu's listing and sources.
    pub share: crate::intent::voice::share::Shown,
    /// The soundboard was made from this, while it shows.
    soundboard: Option<soundboard::Key>,
    /// The preview call's camera, while it films.
    pub camera: crate::effects::devices::Camera,
    /// The call was connected at the last update: joining unmuted waits
    /// for it.
    pub ready: bool,
    /// The wait that leaves an empty private call.
    pub alone: crate::intent::voice::alone::Alone,
    /// A call rings, until answered.
    #[cfg(feature = "discord")]
    ringer: Option<yapfast_core::ring::Ringer>,
}

impl Shown {
    /// Lets go of what the window showed, keeping the call's own state:
    /// the tile large, the timer, the ringing.
    pub fn forgotten(mut self) -> Self {
        self.tiles = tiles::Shown::default();
        self.lives = lives::Lives::default();
        self.meter = None;
        self.soundboard = None;
        self
    }
}

/// Shows the call: see the module's doc.
pub fn show(
    core: &Application,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut Shown,
    media: &mut Media,
    requests: &mut Requests,
) {
    let state = &core.state;
    let voice = &state.voice;
    dock::show(state, &mut shown.timer, &mut vm.voice.dock);
    vm.voice.dock.noise_on = layout.noise_suppression != yapfast_core::noise::Suppressor::Off;
    dock::controls(state, &vm.voice.dock, &mut vm.voice.controls);
    vm.shell.voice_dock = voice.active() || voice.error.is_some();
    // A stream starts large, and leaves when watching it stops.
    let watching = voice
        .watching
        .as_ref()
        .map(|watching| watching.user.as_str());
    if watching != shown.watched.as_deref() {
        if let Some(user) = watching {
            shown.focus = Some(Focus::Stream(user.to_owned()));
        } else if matches!(&shown.focus, Some(Focus::Stream(user)) if shown.watched.as_ref() == Some(user))
        {
            shown.focus = None;
        }
        shown.watched = watching.map(str::to_owned);
    }
    let participants = if voice.active() {
        &voice.participants[..]
    } else {
        &[]
    };
    let stage = Stage {
        participants,
        sharing: voice.sharing.is_some(),
    };
    let focused = shown.focus.as_ref().and_then(|focus| stage.slot(focus));
    if focused.is_none() {
        shown.focus = None;
        shown.others_hidden = false;
        close(vm, Layer::FullStage);
    }
    shown.lives.follow(voice);
    let built = tiles::tiles(state, stage, &shown.lives, &mut shown.tiles, requests);
    let full = vm.shell.is_open(Layer::FullStage);
    let made = Tiles::new(built, focused, full, shown.others_hidden);
    if vm.voice.stage != made {
        vm.voice.stage = made;
    }
    vm.chrome.full_screen = full;
    join::show(state, layout, focused.is_some(), &mut vm.conversation);
    join::way_in(state, &vm.conversation, &mut vm.voice.join);
    let (ringing, asks) = join::incoming(state, layout, &mut vm.voice.incoming);
    open(vm, Layer::IncomingCall, ringing);
    open(vm, Layer::SwitchCall, asks);
    // The corner, while the call is off screen.
    let on_screen = matches!(
        vm.conversation.call,
        anomi_vm::conversation::Call::Here { .. }
    );
    let pip = stage.pip(shown.focus.as_ref(), watching);
    let cornered =
        voice.active() && pip != Pip::None && !on_screen && !full && !vm.shell.signed_out;
    shown.pip = cornered.then(|| (pip, stage.pip_slot(pip, watching)));
    corner(state, stage, shown, &mut vm.voice.pip);
    open(vm, Layer::Pip, cornered);
    place(core, vm, shown, media);
    // The soundboard, while it shows, as its sounds or pictures change.
    let board = vm
        .shell
        .is_open(Layer::Soundboard)
        .then(|| soundboard::key(state));
    if board.is_some() && board != shown.soundboard {
        soundboard(state, &mut vm.voice.soundboard, requests);
    }
    shown.soundboard = board;
    if !voice.active() {
        tiles::forget(&mut shown.tiles);
    }
}

/// Opens `layer` while `shown`, closes it otherwise.
fn open(vm: &mut Vm, layer: Layer, shown: bool) {
    if shown {
        vm.shell.open(layer);
    } else {
        close(vm, layer);
    }
}

fn close(vm: &mut Vm, layer: Layer) {
    if vm.shell.is_open(layer) {
        vm.shell.close(layer);
    }
}

/// What the corner shows (`StreamPip`, `ui/yapfast.slint:4566`).
fn corner(
    state: &yapfast_core::application::AppState,
    stage: Stage<'_>,
    shown: &Shown,
    made: &mut anomi_vm::voice::pip::Pip,
) {
    let voice = &state.voice;
    let lives = &shown.lives;
    let frame = |live: Option<&anomi_vm::kind::Live>| live.filter(|live| live.has_frame()).cloned();
    let mut pip = anomi_vm::voice::pip::Pip {
        drag: made.drag,
        ..Default::default()
    };
    match shown.pip.map(|(pip, _)| pip) {
        Some(Pip::Watched) => {
            if let Some(watched) = voice.watching.as_deref() {
                pip.title = tr!("{}'s screen", watched.name);
                pip.status = tiles::watch_status(watched);
                pip.frame = frame(lives.watched.as_ref().map(|live| &live.1));
                pip.viewers = tiles::viewers(
                    state,
                    stage.participants,
                    &watched.viewers,
                    Some(&watched.user),
                );
                pip.stop = Some(Stop {
                    icon: Icon::X,
                    label: tr!("Stop watching").into(),
                });
            }
        }
        Some(Pip::Shared) => {
            let sharing = voice.sharing.as_ref();
            let phase = sharing.map(|sharing| sharing.phase);
            pip.title = tr!("Your screen").into();
            pip.status = tiles::share_status(phase).into();
            pip.frame = frame(lives.own.as_ref().map(|live| &live.1));
            pip.live = phase == Some(yapfast_core::voice::SharePhase::Live);
            let ids = sharing.map_or(&[][..], |sharing| &sharing.viewers);
            pip.viewers = tiles::viewers(state, stage.participants, ids, None);
            pip.stop = Some(Stop {
                icon: Icon::ScreenShareOff,
                label: tr!("Stop sharing").into(),
            });
        }
        Some(Pip::Camera(index)) => {
            if let Some(person) = stage.participants.get(index) {
                pip.title = crate::project::account::display_part(&person.user).to_owned();
                pip.frame = frame(lives.camera(&person.id));
            }
        }
        Some(Pip::None) | None => {}
    }
    if *made != pip {
        *made = pip;
    }
}

/// Tells the feeds where they show and follows the pictures in sight, in
/// front (`place_videos`); the grid's placement and the corner say where.
pub fn place(core: &Application, vm: &Vm, shown: &Shown, media: &mut Media) {
    let voice = &core.state.voice;
    let participants = if voice.active() {
        &voice.participants[..]
    } else {
        &[]
    };
    let stage = Stage {
        participants,
        sharing: voice.sharing.is_some(),
    };
    let covered = [
        Layer::Settings,
        Layer::ServerSettings,
        Layer::ChannelSettings,
        Layer::Viewer,
        Layer::Video,
    ]
    .into_iter()
    .any(|layer| vm.shell.is_open(layer));
    let full = vm.shell.is_open(Layer::FullStage);
    let in_call = matches!(
        vm.conversation.call,
        anomi_vm::conversation::Call::Here { .. }
    );
    let sight = lives::Sight {
        front: vm.chrome.focused && voice.active(),
        grid: full || (in_call && !covered),
        placement: shown.placement,
        pip: shown
            .pip
            .map(|(pip, _)| pip)
            .filter(|_| vm.shell.is_open(Layer::Pip) && !covered),
        scale: anomi_widgets::picture::scale(),
    };
    let sources = shown.lives.place(voice, stage, sight);
    crate::media::follow_call(media, sources);
}

/// The second's clock: the connection's numbers while they show, said
/// again; "Copied" goes after two seconds.
pub fn tick(state: &yapfast_core::application::AppState, vm: &mut Vm, shown: &mut Shown) {
    if vm.shell.is_open(Layer::Connection) {
        let copied = shown.copied.is_some_and(|at| at.elapsed() < COPIED);
        if !copied {
            shown.copied = None;
            vm.voice.connection.copied = false;
        }
        link::read(state, &mut vm.voice.connection);
    }
}

/// How long the privacy code says it was copied.
const COPIED: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether the connection's numbers show, which the second's clock says
/// again; the call's time is a clock of its own (`anomi_widgets::clock`).
pub fn ticking(vm: &Vm) -> bool {
    vm.shell.is_open(Layer::Connection)
}

/// The window goes into the tray: its tiles and their pictures go, the
/// feeds stop decoding for it, the meter closes.
pub fn hide(core: &Application, vm: &mut Vm, shown: &mut Shown, media: &mut Media) {
    shown.lives.hide(&core.state.voice);
    shown.meter = None;
    tiles::forget(&mut shown.tiles);
    vm.voice.stage = Tiles::default();
    vm.voice.pip = Default::default();
    crate::media::follow_call(media, Vec::new());
}

/// A DM call ringing this account rings, until it's answered, unless the
/// account doesn't want to be disturbed (`ring`, `src/ui/shell.rs:494`).
pub fn ring(state: &yapfast_core::application::AppState, shown: &mut Shown) {
    #[cfg(feature = "discord")]
    {
        use yapfast_core::ring::{Ringer, Tone};
        if join::ringing(state).next().is_none() || state.do_not_disturb() {
            shown.ringer = None;
        } else if shown.ringer.is_none() {
            shown.ringer = Ringer::start(Tone::Incoming, state.voice.speaker.as_deref())
                .inspect_err(|error| tracing::warn!(%error, "could not ring"))
                .ok();
        }
    }
    #[cfg(not(feature = "discord"))]
    let _ = (state, shown);
}
