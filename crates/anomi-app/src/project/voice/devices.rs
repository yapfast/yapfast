//! The device menus at the account card's carets (`input-menu` and
//! `output-menu`, `ui/yapfast.slint:2068`, `:2096`), as Discord's: the
//! devices the call lists (`device_rows`, `src/ui.rs:3511`), the one in
//! use, its volume as the layout keeps it, and for the microphone with
//! Discord linked the input profile its processing makes. Made as the menu
//! opens, and as what it shows changes while it shows.

use std::collections::HashMap;
use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::voice::{self, Device};
use yapfast_lang::tr;

use anomi_vm::settings::voice::Profile;
use anomi_vm::voice::devices::{Devices, Kind, Labels};

use crate::layout::Layout;
use crate::project::settings::voice::profile;

/// What the list names the system's default by, in the device's id.
const DEFAULT_DEVICE: &str = "name:default: ";

/// A device as the menu names it: the system's default says so, as
/// Discord's "System: ...".
fn label(name: &str) -> String {
    match name.strip_prefix("default: ") {
        Some(device) => tr!("System: {}", device),
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

/// The menu for `kind`, as it opens or what it shows changes.
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
    match input_profile(state, layout, kind) {
        Some(chosen) => menu.profile(tr!("Input Profile"), profiles(), chosen),
        None => menu,
    }
}

/// The input profiles by name, in [`Profile::ALL`]'s order.
pub(crate) fn profiles() -> [String; 3] {
    [
        tr!("Voice Isolation"),
        tr!("Studio"),
        yapfast_lang::text_in("InputProfile", "Custom"),
    ]
    .map(String::from)
}

/// The profile the input menu shows: the microphone's, with Discord
/// linked, whose calls the processing is for.
fn input_profile(state: &AppState, layout: &Layout, kind: Kind) -> Option<Profile> {
    (kind == Kind::Input)
        .then(|| profile(state, layout))
        .flatten()
}

/// What the menu was made from, to make it again when it changes: the
/// devices, the one chosen, the profile.
pub type Listed = (usize, Option<String>, Option<Profile>);

/// What the menu for `kind` is made from now.
pub fn listed(state: &AppState, layout: &Layout, kind: Kind) -> Listed {
    let voice = &state.voice;
    let (list, chosen) = match kind {
        Kind::Input => (&voice.microphones, &voice.microphone),
        Kind::Output => (&voice.speakers, &voice.speaker),
    };
    let profile = input_profile(state, layout, kind);
    (Arc::as_ptr(list).addr(), chosen.clone(), profile)
}
