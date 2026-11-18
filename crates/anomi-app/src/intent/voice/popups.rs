//! The popups a call opens and what they ask: the device menus at the
//! account card's carets, with the microphone's level (`select_microphone`,
//! `select_speaker`, `metering`, `src/ui/callbacks.rs:3605`-`:3860`), the
//! connection's details (`src/ui/call_link.rs`), the soundboard
//! (`:3661`-`:3735`), sharing the screen (`share`), a participant's volumes
//! (`participant_menu`), and the corner's place after a drag.

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::noise::Suppressor;
use yapfast_core::voice::{Command, Phase};
use yapfast_lang::tr;

use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::voice::devices::{self as menu, Kind, Noise};
use anomi_vm::voice::{self, connection, participant_menu, soundboard};

use super::command;
use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::project::voice::{device_menu, link};

pub(super) fn handle(app: &mut App, intent: voice::Intent) -> Task<Message> {
    use voice::Intent as I;
    match intent {
        I::Devices(intent) => devices(app, intent),
        I::Connection(intent) => connection(app, intent),
        I::Soundboard(intent) => sound(app, intent),
        I::ParticipantMenu(participant_menu::Intent::Volume {
            person,
            stream,
            volume,
            last,
        }) => {
            let kind = if stream {
                yapfast_core::volume::Kind::Stream
            } else {
                yapfast_core::volume::Kind::Voice
            };
            // Within a step of the default is the default, as Discord snaps it.
            let default = kind.default_gain();
            let gain = if (volume as f32 - yapfast_core::volume::to_slider(default)).abs() < 1.0 {
                default
            } else {
                yapfast_core::volume::from_slider(volume as f32)
            };
            yapfast_core::volume::set(kind, &person, gain);
            let card = &mut app.vm.voice.participant_menu;
            if stream {
                card.stream_volume = volume;
            } else {
                card.voice_volume = volume;
            }
            if last {
                app.layout.edit(|saved| {
                    let volumes = if stream {
                        &mut saved.stream_volumes
                    } else {
                        &mut saved.volumes
                    };
                    if gain == default {
                        volumes.remove(&person);
                    } else {
                        volumes.insert(person, gain);
                    }
                });
            }
            Task::none()
        }
        I::Share(intent) => super::share::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not voice's");
            Task::none()
        }
    }
}

/// The device menu at a caret of the account card: the microphone's, or
/// the headphones' (`InputMenu`, `OutputMenu`); the devices are listed again
/// as it opens.
pub fn open_devices(app: &mut App, kind: Kind, spot: Spot) -> Task<Message> {
    app.vm.voice.devices = device_menu(&app.core.state, &app.layout, kind);
    app.shown.voice.devices = Some(crate::project::voice::devices::listed(
        &app.core.state,
        kind,
    ));
    app.vm.account.input_open = kind == Kind::Input;
    app.vm.account.output_open = kind == Kind::Output;
    app.vm.shell.open_at(Layer::Devices, spot);
    command(app, Command::Devices)
}

fn devices(app: &mut App, intent: menu::Intent) -> Task<Message> {
    use menu::Intent as I;
    let kind = app.vm.voice.devices.kind;
    match intent {
        I::ToggleList => {
            let menu = &mut app.vm.voice.devices;
            menu.listing = !menu.listing;
            Task::none()
        }
        I::Select(index) => {
            let voice = &app.core.state.voice;
            let list = match kind {
                Kind::Input => &voice.microphones,
                Kind::Output => &voice.speakers,
            };
            let Some(id) = list.get(index).map(|device| device.id.clone()) else {
                return Task::none();
            };
            app.vm.voice.devices.choose(Some(index));
            command(
                app,
                match kind {
                    Kind::Input => Command::Microphone(id),
                    Kind::Output => Command::Speaker(id),
                },
            )
        }
        I::Volume(volume) => {
            app.vm.voice.devices.volume = volume;
            let gain = yapfast_core::volume::from_slider(volume.clamp(0, 200) as f32);
            match kind {
                Kind::Input => yapfast_core::volume::set_microphone(gain),
                Kind::Output => yapfast_core::volume::set_speakers(gain),
            }
            Task::none()
        }
        I::Commit(volume) => {
            let volume = Some(volume.clamp(0, 200) as u8);
            app.layout.edit(|saved| match kind {
                Kind::Input => saved.microphone_volume = volume,
                Kind::Output => saved.speaker_volume = volume,
            });
            Task::none()
        }
        I::Noise(noise) => {
            let chosen = match noise {
                Noise::DeepFilterNet => Suppressor::DeepFilterNet,
                Noise::RNNoise => Suppressor::RNNoise,
                Noise::Off => Suppressor::Off,
            };
            noise_suppression(app, chosen);
            Task::none()
        }
        I::Settings => {
            let closed = close(app, Layer::Devices);
            app.vm.settings.frame.page = anomi_vm::settings::frame::Page::Voice;
            app.vm.shell.open(Layer::Settings);
            closed
        }
        intent => {
            tracing::debug!(?intent, "not the devices'");
            Task::none()
        }
    }
}

/// Uses `chosen` to take out noise in Discord calls, and keeps it.
fn noise_suppression(app: &mut App, chosen: Suppressor) {
    if chosen != Suppressor::Off {
        app.shown.voice.noise_last = chosen;
    }
    yapfast_core::noise::set(chosen);
    app.layout.edit(|saved| saved.noise_suppression = chosen);
    if app.vm.voice.devices.noise.is_some() {
        app.vm.voice.devices.noise = Some(match chosen {
            Suppressor::DeepFilterNet => Noise::DeepFilterNet,
            Suppressor::RNNoise => Noise::RNNoise,
            Suppressor::Off => Noise::Off,
        });
    }
    app.show(Reach::VOICE);
}

/// The dock's noise button: off, then back to what it used.
pub(super) fn toggle_noise(app: &mut App) -> Task<Message> {
    let chosen = if app.layout.noise_suppression == Suppressor::Off {
        app.shown.voice.noise_last
    } else {
        Suppressor::Off
    };
    noise_suppression(app, chosen);
    Task::none()
}

/// The connection's details at the signal icon or the status.
pub(super) fn open_link(app: &mut App, spot: Spot) -> Task<Message> {
    link::read(&app.core.state, &mut app.vm.voice.connection);
    app.vm.shell.open_at(Layer::Connection, spot);
    Task::none()
}

/// The pointer came to the status: its ping is read fresh.
pub(super) fn read_link(app: &mut App) -> Task<Message> {
    link::read(&app.core.state, &mut app.vm.voice.connection);
    app.vm.voice.dock.ping = app.vm.voice.connection.ping.clone();
    Task::none()
}

fn connection(app: &mut App, intent: connection::Intent) -> Task<Message> {
    use connection::Intent as I;
    match intent {
        I::Open(spot) => open_link(app, spot),
        I::Read => read_link(app),
        I::Tab(tab) => {
            app.vm.voice.connection.tab = tab;
            Task::none()
        }
        I::Copy(code) => {
            app.vm.voice.connection.copied = true;
            app.shown.voice.copied = Some(std::time::Instant::now());
            iced::clipboard::write(code).discard()
        }
        intent => {
            tracing::debug!(?intent, "not the connection's");
            Task::none()
        }
    }
}

/// The soundboard above its button; its custom emoji are asked for.
pub(super) fn open_soundboard(app: &mut App, spot: Spot) -> Task<Message> {
    crate::project::voice::soundboard(
        &app.core.state,
        &mut app.vm.voice.soundboard,
        &mut app.pictures,
    );
    let shifted = Spot {
        anchor: spot.anchor,
        ..spot
    };
    app.vm.shell.open_at(Layer::Soundboard, shifted);
    Task::none()
}

fn sound(app: &mut App, intent: soundboard::Intent) -> Task<Message> {
    use soundboard::Intent as I;
    match intent {
        I::Play(id) => command(app, Command::Soundboard(id)),
        I::Preview(id) => command(app, Command::PreviewSound(id)),
        I::Favorite(id, favorite) => command(app, Command::FavoriteSound(id, favorite)),
        intent => {
            tracing::debug!(?intent, "not the soundboard's");
            Task::none()
        }
    }
}

pub(super) fn open_share(app: &mut App, spot: Spot) -> Task<Message> {
    super::share::open(app, spot)
}

/// A layer of the call closes: what it held goes with it.
pub fn close(app: &mut App, layer: Layer) -> Task<Message> {
    let task = match layer {
        Layer::Devices => {
            app.vm.account.input_open = false;
            app.vm.account.output_open = false;
            app.shown.voice.meter = None;
            app.shown.voice.devices = None;
            Task::none()
        }
        Layer::FullStage => return super::full_screen(app, false),
        Layer::SwitchCall => return app.dispatch(Action::CancelVoiceSwitch),
        Layer::Share => super::share::closed(app),
        _ => Task::none(),
    };
    if app.vm.shell.is_open(layer) {
        app.vm.shell.close(layer);
    }
    task
}

/// After each update: the input menu measures the microphone while it
/// shows in front; its rows follow the devices listed.
pub fn after(app: &mut App) -> Task<Message> {
    let showing = app.vm.shell.is_open(Layer::Devices);
    if showing && let Some(listed) = &app.shown.voice.devices {
        let kind = app.vm.voice.devices.kind;
        let now = crate::project::voice::devices::listed(&app.core.state, kind);
        if *listed != now {
            let (listing, level) = (
                app.vm.voice.devices.listing,
                app.vm.voice.devices.level.clone(),
            );
            app.vm.voice.devices = device_menu(&app.core.state, &app.layout, kind);
            app.vm.voice.devices.listing = listing;
            app.vm.voice.devices.level = level;
            app.shown.voice.devices = Some(now);
        }
    }
    let front = showing && app.motion.front && !app.hidden;
    let (state, menu) = (&app.core.state, &app.vm.voice.devices);
    let meter = crate::project::voice::open_meter(state, menu, front, &mut app.shown.voice);
    Task::batch([meter, joined(app)])
}

/// The call connected: it's joined deafened, or unmuted, as the account
/// card set for the next call (`changed voice-ready`,
/// `ui/yapfast.slint:2008`).
fn joined(app: &mut App) -> Task<Message> {
    let voice = &app.core.state.voice;
    let ready = matches!(voice.phase, Phase::Connected | Phase::Reconnecting);
    if !ready || std::mem::replace(&mut app.shown.voice.ready, ready) {
        app.shown.voice.ready = ready;
        return Task::none();
    }
    if app.layout.deafen_on_join() {
        if !voice.deafened {
            return app.dispatch(Action::DeafenOnJoin);
        }
    } else if app.layout.unmute_on_join() && voice.muted {
        return app.dispatch(Action::UnmuteOnJoin);
    }
    Task::none()
}

/// The input menu's microphone can't be read, as its meter found: the menu
/// says so while it still shows that meter (`meter-note`).
pub(super) fn unread(app: &mut App, level: &anomi_vm::kind::Level) {
    let menu = &mut app.vm.voice.devices;
    if app.vm.shell.is_open(Layer::Devices) && menu.level == *level {
        menu.note = tr!(
            "Yapfast can't read this microphone. Check that it's connected, or choose another."
        )
        .into();
    }
}

/// The corner let go: it sits in the corner nearest to where it was.
pub(super) fn drop_corner(app: &mut App) {
    let drag = std::mem::take(&mut app.vm.voice.pip.drag);
    let size = app.size;
    let (width, height) = (voice::pip::Pip::WIDTH, voice::pip::Pip::HEIGHT);
    let left = 72.0 + app.vm.shell.sidebar_width + 16.0;
    let right = size.width - width - 16.0;
    let top = anomi_theme::metrics::HEADER_HEIGHT + 16.0;
    let bottom = size.height - height - 16.0;
    let corner = &mut app.vm.shell.pip;
    let x = if corner.left { left } else { right } + drag.x;
    let y = if corner.bottom { bottom } else { top } + drag.y;
    corner.left = x < (left + right) / 2.0;
    corner.bottom = y >= (top + bottom) / 2.0;
}
