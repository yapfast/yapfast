//! The popups a call opens and what they ask: the device menus at the
//! account card's carets (`devices`), the connection's details
//! (`src/ui/call_link.rs`), the soundboard (`src/ui/callbacks.rs:3661`-
//! `:3735`), sharing the screen (`share`), a participant's volumes
//! (`participant_menu`), and the corner's place after a drag.

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::noise::Suppressor;
use yapfast_core::voice::{Command, Phase};

use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::voice::{self, connection, participant_menu, soundboard};

use super::command;
use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::project::voice::link;

pub(super) fn handle(app: &mut App, intent: voice::Intent) -> Task<Message> {
    use voice::Intent as I;
    match intent {
        I::Devices(intent) => super::devices::handle(app, intent),
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

/// Uses `chosen` to take out noise in Discord calls, and keeps it.
pub fn noise_suppression(app: &mut App, chosen: Suppressor) {
    if chosen != Suppressor::Off {
        app.shown.voice.noise_last = chosen;
    }
    yapfast_core::noise::set(chosen);
    app.layout.edit(|saved| saved.noise_suppression = chosen);
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

/// After each update: the device menu follows what it shows; a call
/// joined unmutes or deafens as the account card set.
pub fn after(app: &mut App) -> Task<Message> {
    super::devices::follow(app);
    joined(app)
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
