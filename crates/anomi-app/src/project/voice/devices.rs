//! The device menus at the account card's carets (`input-menu` and
//! `output-menu`, `ui/yapfast.slint:2068`, `:2096`): the devices the call
//! lists (`device_rows`, `src/ui.rs:3511`), the one in use, its volume as
//! the layout keeps it, and for the microphone the level and, in a Discord
//! call, noise suppression. Made as the menu opens, and as the devices
//! change while it shows.

use std::collections::HashMap;
use std::sync::Arc;

use iced::Task;
use yapfast_core::application::AppState;
use yapfast_core::network::Network;
use yapfast_core::noise::Suppressor;
use yapfast_core::voice::{self, Device};
use yapfast_lang::tr;

use anomi_vm::voice::devices::{Devices, Kind, Labels, Noise};

use super::Shown;
use crate::layout::Layout;
use crate::message::Message;

/// What the list names the system's default by, in the device's id.
const DEFAULT_DEVICE: &str = "name:default: ";

/// A device as the menu names it: the system's default says so.
fn label(name: &str) -> String {
    match name.strip_prefix("default: ") {
        Some(device) => tr!("Default ({})", device),
        None => name.to_owned(),
    }
}

/// The menu's rows and the one in use: a chosen device that isn't
/// connected shows last, as not connected, by the name `names` kept of it
/// while it was listed (`device_rows`).
pub(crate) fn rows(
    devices: &[Device],
    chosen: Option<&str>,
    names: &mut HashMap<String, String>,
) -> (Vec<String>, Option<usize>) {
    let mut rows: Vec<String> = devices.iter().map(|device| label(&device.name)).collect();
    for (device, row) in devices.iter().zip(&rows) {
        if names.get(&device.id) != Some(row) {
            names.insert(device.id.clone(), row.clone());
        }
    }
    let Some(chosen) = chosen else {
        return (rows, (!devices.is_empty()).then_some(0));
    };
    let default = |id: &str| id.starts_with(DEFAULT_DEVICE);
    let found = (devices.iter().position(|device| device.id == chosen)).or_else(|| {
        default(chosen)
            .then(|| devices.iter().position(|device| default(&device.id)))
            .flatten()
    });
    let index = found.unwrap_or_else(|| {
        let name = (names.get(chosen).cloned())
            .or_else(|| chosen.strip_prefix(voice::NAMED).map(label))
            .unwrap_or_else(|| tr!("Chosen device").to_owned());
        rows.push(tr!("{} (not connected)", name));
        rows.len() - 1
    });
    (rows, Some(index))
}

/// The menu for `kind`, as it opens or its devices change.
pub fn devices(
    state: &AppState,
    layout: &Layout,
    kind: Kind,
    names: &mut HashMap<String, String>,
) -> Devices {
    let voice = &state.voice;
    let (list, chosen, title, empty, volume) = match kind {
        Kind::Input => (
            &voice.microphones,
            voice.microphone.as_deref(),
            tr!("Input Device"),
            tr!("No microphones found. Connect one, then reopen this menu."),
            layout.microphone_volume,
        ),
        Kind::Output => (
            &voice.speakers,
            voice.speaker.as_deref(),
            tr!("Output Device"),
            tr!("No speakers found. Connect one, then reopen this menu."),
            layout.speaker_volume,
        ),
    };
    let labels = Labels {
        title: title.into(),
        empty: empty.into(),
        settings: tr!("Voice Settings").into(),
        none: tr!("None found").into(),
        default: tr!("System default").into(),
    };
    let (rows, current) = rows(list, chosen, names);
    let mut menu = Devices::new(kind, labels, &rows, current);
    menu.volume = volume.map_or(100, i32::from);
    menu.muted = voice.muted;
    let discord = voice.active()
        && (voice.room.as_deref()).is_some_and(|room| Network::of(room) == Network::Discord);
    if kind == Kind::Input {
        if !yapfast_core::meter::AVAILABLE {
            menu.note = tr!("The level meter isn't available on this system yet.").into();
        }
        menu.noise = discord.then_some(match layout.noise_suppression {
            Suppressor::DeepFilterNet => Noise::DeepFilterNet,
            Suppressor::RNNoise => Noise::RNNoise,
            Suppressor::Off => Noise::Off,
        });
    }
    menu
}

/// The devices the menu was made from, to make it again when they change.
pub fn listed(state: &AppState, kind: Kind) -> (usize, Option<String>, bool) {
    let voice = &state.voice;
    let (list, chosen) = match kind {
        Kind::Input => (&voice.microphones, &voice.microphone),
        Kind::Output => (&voice.speakers, &voice.speaker),
    };
    (Arc::as_ptr(list).addr(), chosen.clone(), voice.muted)
}

/// Measures the microphone for the input menu while it shows in front, and
/// stops otherwise; a new microphone is measured from then on.
pub fn open_meter(
    state: &AppState,
    devices: &Devices,
    showing: bool,
    shown: &mut Shown,
) -> Task<Message> {
    let microphone = state.voice.microphone.as_deref();
    let wanted = showing && devices.input();
    if !wanted {
        shown.meter = None;
        return Task::none();
    }
    if (shown.meter.as_ref()).is_some_and(|meter| meter.measures(microphone, &devices.level)) {
        return Task::none();
    }
    let (meter, task) = super::meter::start(microphone, &devices.level);
    shown.meter = Some(meter);
    task
}
