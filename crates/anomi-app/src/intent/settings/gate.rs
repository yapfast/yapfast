//! The voice gate's meter and the microphone test on Voice & video
//! (`gate-metering`, `gate-meter-tick`, `src/ui/callbacks.rs:3861`-`:3905`;
//! `gate-mode`, `ui/yapfast.slint:2034`). The core's `GateMeter` opens the
//! microphone, measures it as Discord calls' gate hears it, and plays it
//! back while the test runs. It runs only while the page shows in front,
//! with a Discord account linked: behind another window, in the tray, on
//! another page or with Settings closed, it's dropped and the microphone
//! closes, and a test going on stops.
//!
//! A task off the window's thread reads the meter 20 times a second and
//! stores the level in the page's `Level`, which the meters read as they
//! draw; it asks a redraw, with no message, only when the level moved a
//! whole dB, and a message only when the gate opened or closed or the
//! meter or the speakers failed, which the page says in words.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use iced::Task;
use iced::futures::stream;
use iced::task::Handle;
use yapfast_core::discord_audio::{self, GateMeter};
use yapfast_lang::tr;

use anomi_vm::kind::Level;
use anomi_vm::settings::frame::Page;
use anomi_vm::shell::Layer;

use super::Due;
use crate::app::App;
use crate::layout::InputMode;
use crate::message::Message;

/// How often the meter is read (the 50 ms timer).
const EVERY: Duration = Duration::from_millis(50);

/// What the page wants of the gate now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The level, for the sensitivity's handle.
    Meter,
    /// The level, and the microphone played back.
    Test,
}

/// The gate measured for the page, until dropped.
pub struct Gate {
    meter: Arc<Mutex<GateMeter>>,
    mode: Mode,
    microphone: Option<String>,
    speaker: Option<String>,
    level: Level,
    _task: Handle,
}

/// What the meter heard that the page says in words, for the level it was
/// measured into.
#[derive(Clone, Debug)]
pub struct Heard {
    level: Level,
    sending: bool,
    failed: bool,
    playback_failed: bool,
}

/// The mode the page wants: the test while it's asked for outside a call,
/// else the meter in voice activity, else nothing.
fn wanted(app: &App) -> Option<Mode> {
    let page = &app.vm.settings.voice;
    let showing = app.vm.shell.is_open(Layer::Settings)
        && app.vm.settings.frame.page == Page::Voice
        && page.discord
        && app.motion.front
        && !app.hidden;
    if !showing {
        return None;
    }
    if page.microphone_testing && !page.calling {
        Some(Mode::Test)
    } else {
        (app.layout.input_mode == InputMode::VoiceActivity).then_some(Mode::Meter)
    }
}

/// After each update: starts, changes or stops the gate as the page wants.
pub(super) fn follow(app: &mut App) -> Task<Message> {
    let mode = wanted(app);
    let voice = &app.core.state.voice;
    let kept = &mut app.shown.settings.kept;
    let Some(mode) = mode else {
        if kept.gate.take().is_some() {
            stopped(app);
        }
        return Task::none();
    };
    let level = &app.vm.settings.voice.gate;
    if let Some(gate) = kept.gate.as_mut()
        && gate.microphone == voice.microphone
        && gate.level == *level
    {
        if gate.mode != mode || (mode == Mode::Test && gate.speaker != voice.speaker) {
            let mut meter = gate.meter.lock().unwrap_or_else(PoisonError::into_inner);
            match mode {
                Mode::Test => meter.play(voice.speaker.as_deref()),
                Mode::Meter => meter.silence(),
            }
            (gate.mode, gate.speaker) = (mode, voice.speaker.clone());
        }
        return Task::none();
    }
    // Another microphone, or none measured yet: measured anew, the test
    // going on from the new one.
    let mut meter = GateMeter::start(voice.microphone.as_deref());
    if mode == Mode::Test {
        meter.play(voice.speaker.as_deref());
    }
    let meter = Arc::new(Mutex::new(meter));
    let task = read(meter.clone(), level.clone());
    let (task, handle) = task.abortable();
    kept.gate = Some(Gate {
        meter,
        mode,
        microphone: voice.microphone.clone(),
        speaker: voice.speaker.clone(),
        level: level.clone(),
        _task: handle.abort_on_drop(),
    });
    task
}

/// The gate stopped: the page shows no level, and a test going on is over.
fn stopped(app: &mut App) {
    let page = &mut app.vm.settings.voice;
    page.gate.set_decibels(-100.0);
    page.gate_sending = false;
    page.gate_note.clear();
    page.microphone_test_note.clear();
    page.microphone_testing = false;
}

/// Reads the meter into `level` 20 times a second.
fn read(meter: Arc<Mutex<GateMeter>>, level: Level) -> Task<Message> {
    let first = (meter, level, None::<(bool, bool, bool)>);
    let readings = stream::unfold(first, |(meter, level, said)| async move {
        loop {
            tokio::time::sleep(EVERY).await;
            let (decibels, now) = {
                let meter = meter.lock().unwrap_or_else(PoisonError::into_inner);
                let now = (meter.sending(), meter.failed(), meter.playback_failed());
                (meter.level(), now)
            };
            let moved = level.set_decibels(decibels.max(-100.0).round());
            if said != Some(now) {
                let (sending, failed, playback_failed) = now;
                let heard = Heard {
                    level: level.clone(),
                    sending,
                    failed,
                    playback_failed,
                };
                return Some((Some(heard), (meter, level, Some(now))));
            }
            if moved {
                return Some((None, (meter, level, said)));
            }
        }
    });
    Task::run(readings, |heard| heard).then(|heard| match heard {
        Some(heard) => Task::done(Message::Settings(Due::Gate(heard))),
        None => iced_runtime::task::effect(iced_runtime::Action::Window(
            iced_runtime::window::Action::RedrawAll,
        )),
    })
}

/// The gate opened or closed, or the meter or the speakers failed: the
/// page says so, while it still shows that gate.
pub(super) fn heard(app: &mut App, heard: Heard) {
    let page = &mut app.vm.settings.voice;
    if page.gate != heard.level {
        return;
    }
    page.gate_sending = heard.sending;
    page.gate_note = if !discord_audio::AVAILABLE {
        tr!("Discord voice and its level aren't available on this system yet.")
    } else if heard.failed {
        tr!("Yapfast can't read this microphone. Check that it's connected, or choose another.")
    } else {
        ""
    }
    .to_owned();
    page.microphone_test_note = if heard.playback_failed {
        tr!("Yapfast can't play your microphone. Check your speakers, or choose others.")
    } else {
        ""
    }
    .to_owned();
}
