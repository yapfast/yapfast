//! The voice dock on the account card and the call controls under the
//! stage, from the core's call (`show_voice`, `src/ui/refresh.rs:4382`;
//! the timer, `src/ui/call_timer.rs`, whose time the dock reads as it
//! draws). A handful of fields, set rather than keyed; while sharing, the
//! stream's quality as "Your screen" says it.

use std::time::Instant;

use yapfast_core::application::AppState;
use yapfast_core::network::Network;
use yapfast_core::voice::{self, CameraPhase, Phase, Quality, SharePhase, StreamAudio};
use yapfast_lang::tr;

use anomi_vm::kind::Elapsed;
use anomi_vm::voice::controls::Controls;
use anomi_vm::voice::dock::{Dock, Signal, Tone};

use crate::intent::voice::share::menu_resolution;

/// The call timed in the dock: its room, and when it connected there.
#[derive(Debug, Default)]
pub struct Timer {
    since: Option<(String, Instant)>,
}

impl Timer {
    /// Times the call in `room` once it connects; a call that moves to
    /// another room starts again (`time_call`).
    fn follow(&mut self, room: Option<&str>) {
        let same = match (&self.since, room) {
            (Some((timed, _)), Some(room)) => timed == room,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.since = room.map(|room| (room.to_owned(), Instant::now()));
        }
    }

    /// How long it has lasted, which the dock reads as it draws; none with
    /// no call.
    pub fn time(&self) -> Option<Elapsed> {
        self.since.as_ref().map(|(_, since)| Elapsed::Since(*since))
    }
}

/// Whether the call is Discord's.
fn discord(voice: &voice::State) -> bool {
    (voice.room.as_deref()).is_some_and(|room| Network::of(room) == Network::Discord)
}

/// The dock, from the call.
pub fn show(state: &AppState, timer: &mut Timer, dock: &mut Dock) {
    let voice = &state.voice;
    let ready = matches!(voice.phase, Phase::Connected | Phase::Reconnecting);
    timer.follow(voice.room.as_deref().filter(|_| ready));
    let status = match voice.phase {
        Phase::Disconnected => tr!("Voice disconnected"),
        Phase::Joining => tr!("Joining voice..."),
        Phase::Connected if state.preview => tr!("Preview voice"),
        Phase::Connected => tr!("Voice connected"),
        Phase::Reconnecting => tr!("Reconnecting voice..."),
        Phase::Leaving => tr!("Leaving voice..."),
    };
    let detail = if state.preview {
        tr!("{} · Sample", voice.name)
    } else if voice.end_to_end {
        tr!("{} · End-to-end encrypted", voice.name)
    } else {
        tr!("{} · Secure connection", voice.name)
    };
    let sharing = voice.sharing.as_ref();
    let measured = voice.active() && voice.link.is_some();
    let made = Dock {
        status: status.into(),
        time: timer.time(),
        detail,
        name: voice.name.clone(),
        tone: match voice.phase {
            Phase::Connected => Tone::Connected,
            Phase::Disconnected => Tone::Disconnected,
            _ => Tone::Working,
        },
        active: voice.active(),
        ready,
        leaving: voice.phase == Phase::Leaving,
        signal: measured.then_some(match voice.link_quality {
            Quality::Unknown => Signal::Unknown,
            Quality::Fine => Signal::Fine,
            Quality::Average => Signal::Average,
            Quality::Bad => Signal::Bad,
        }),
        ping: dock.ping.clone(),
        warning: voice.warning.clone().unwrap_or_default(),
        error: voice.error.clone().unwrap_or_default(),
        can_share: can_share(state),
        sharing: sharing.is_some(),
        share_live: sharing.is_some_and(|sharing| sharing.phase == SharePhase::Live),
        share_status: super::tiles::share_status(sharing.map(|sharing| sharing.phase)).into(),
        share_watchers: sharing.map_or(0, |sharing| sharing.viewers.len()),
        share_quality: sharing
            .map(|_| {
                let resolution = menu_resolution(voice.quality.resolution);
                anomi_voice::share::quality(resolution, voice.quality.fps)
            })
            .unwrap_or_default(),
        share_sound_on: voice.stream_audio != StreamAudio::Off,
        can_film: voice.can_film,
        camera_on: voice.camera_phase != CameraPhase::Off,
        camera_starting: voice.camera_phase == CameraPhase::Starting,
        camera_unavailable: camera_unavailable(voice),
        can_soundboard: voice.soundboard.is_some() && voice.active(),
        noise_on: dock.noise_on,
        can_noise: discord(voice) && voice.active(),
    };
    if *dock != made {
        *dock = made;
    }
}

/// No camera to film with: the core's list of cameras has none and none
/// films. The list is made again as a call starts, as a device menu or
/// Voice & Video opens and as the camera is asked for
/// (`intent::voice::camera`), so it follows a camera plugged in or taken
/// out without anything polling.
pub(crate) fn camera_unavailable(voice: &voice::State) -> bool {
    voice.cameras.is_empty() && voice.camera_phase == CameraPhase::Off
}

/// Sharing a screen can start: a Discord call, connected, not the preview.
fn can_share(state: &AppState) -> bool {
    let voice = &state.voice;
    !state.preview && voice.phase == Phase::Connected && discord(voice)
}

/// The pill under the stage, as the dock's buttons say.
pub fn controls(state: &AppState, dock: &Dock, controls: &mut Controls) {
    let voice = &state.voice;
    let made = Controls {
        muted: voice.muted,
        deafened: voice.deafened,
        ready: dock.ready,
        leaving: dock.leaving,
        can_share: dock.can_share,
        sharing: dock.sharing,
        can_film: dock.can_film,
        camera_on: dock.camera_on,
        camera_starting: dock.camera_starting,
        camera_unavailable: dock.camera_unavailable,
        can_soundboard: dock.can_soundboard,
    };
    if *controls != made {
        *controls = made;
    }
}
