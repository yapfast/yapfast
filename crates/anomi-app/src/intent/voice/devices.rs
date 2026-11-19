//! The device menus at the account card's carets, as Discord's
//! (`select_microphone`, `select_speaker`, `src/ui/callbacks.rs:3605`,
//! `:3637`): the pointer and the keys move through the menu and its
//! submenus, a device chosen is used, an input profile sets the
//! microphone's processing through Voice & video's own intents, the volume
//! is heard while dragged and kept when let go.

use iced::Task;
use yapfast_core::voice::Command;

use anomi_vm::menu::Spot;
use anomi_vm::settings::Intent::Voice;
use anomi_vm::settings::frame::Page;
use anomi_vm::settings::voice::{Intent as Setting, Profile};
use anomi_vm::shell::Layer;
use anomi_vm::voice::devices::{Intent, Kind};

use super::{close, command};
use crate::app::App;
use crate::message::Message;
use crate::project::voice::device_menu;
use crate::project::voice::devices::listed;

/// The device menu at a caret of the account card: the microphone's, or
/// the headphones' (`InputMenu`, `OutputMenu`); the devices are listed again
/// as it opens.
pub fn open(app: &mut App, kind: Kind, spot: Spot) -> Task<Message> {
    app.vm.voice.devices = device_menu(
        &app.core.state,
        &app.layout,
        kind,
        &mut app.shown.voice.device_names,
    );
    app.shown.voice.devices = Some(listed(&app.core.state, &app.layout, kind));
    app.vm.account.input_open = kind == Kind::Input;
    app.vm.account.output_open = kind == Kind::Output;
    app.vm.shell.open_at(Layer::Devices, spot);
    command(app, Command::Devices)
}

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let kind = app.vm.voice.devices.kind;
    match intent {
        Intent::Hover(_) | Intent::Key(_) => match app.vm.voice.devices.apply(&intent) {
            // A choice the keys made, as the click would.
            Some(chosen) => handle(app, chosen),
            None => Task::none(),
        },
        Intent::Select(index) => {
            let voice = &app.core.state.voice;
            let list = match kind {
                Kind::Input => &voice.microphones,
                Kind::Output => &voice.speakers,
            };
            let Some(id) = list.get(index).map(|device| device.id.clone()) else {
                return Task::none();
            };
            let closed = close(app, Layer::Devices);
            let used = command(
                app,
                match kind {
                    Kind::Input => Command::Microphone(id),
                    Kind::Output => Command::Speaker(id),
                },
            );
            Task::batch([closed, used])
        }
        Intent::Profile(Profile::Custom) => settings(app),
        Intent::Profile(profile) => {
            let closed = close(app, Layer::Devices);
            let profile = Setting::Profile(profile.index());
            let set = crate::intent::settings::handle(app, Voice(profile));
            Task::batch([closed, set])
        }
        Intent::Volume(volume) => {
            app.vm.voice.devices.volume = volume;
            let gain = yapfast_core::volume::from_slider(volume.clamp(0, 200) as f32);
            match kind {
                Kind::Input => yapfast_core::volume::set_microphone(gain),
                Kind::Output => yapfast_core::volume::set_speakers(gain),
            }
            Task::none()
        }
        Intent::Commit(volume) => {
            let volume = Some(volume.clamp(0, 200) as u8);
            app.layout.edit(|saved| match kind {
                Kind::Input => saved.microphone_volume = volume,
                Kind::Output => saved.speaker_volume = volume,
            });
            Task::none()
        }
        Intent::Settings => settings(app),
        intent => {
            tracing::debug!(?intent, "not the devices'");
            Task::none()
        }
    }
}

/// Voice & video in place of the menu: its Voice Settings, and Custom,
/// whose switches are there.
fn settings(app: &mut App) -> Task<Message> {
    let closed = close(app, Layer::Devices);
    Task::batch([closed, crate::intent::settings::open(app, Page::Voice)])
}

/// After each update, while the menu shows: it's made again when the
/// devices, the one in use or the profile changed, keeping where the
/// pointer and the keys were in it.
pub(super) fn follow(app: &mut App) {
    if !app.vm.shell.is_open(Layer::Devices) {
        return;
    }
    let Some(was) = &app.shown.voice.devices else {
        return;
    };
    let kind = app.vm.voice.devices.kind;
    let now = listed(&app.core.state, &app.layout, kind);
    if *was == now {
        return;
    }
    let mut menu = device_menu(
        &app.core.state,
        &app.layout,
        kind,
        &mut app.shown.voice.device_names,
    );
    menu.keep(&app.vm.voice.devices);
    app.vm.voice.devices = menu;
    app.shown.voice.devices = Some(now);
}
