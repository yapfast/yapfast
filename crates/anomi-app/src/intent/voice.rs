//! Voice's intents: the dock, the stage and its tiles, the controls, the
//! corner, joining and answering (here), and the popups the call opens:
//! the devices, the connection, the soundboard and sharing (`popups`).
//! Ported from `src/ui/callbacks.rs:3061`-`:3881` and the dock's and the
//! stage's handlers in `ui/yapfast.slint`. A call's command goes to the
//! core, which answers with the call's state; what the app keeps beside it
//! (the tile shown large, full screen, the corner's place) re-projects the
//! call alone.

pub(crate) mod alone;
pub(crate) mod camera;
mod glance;
mod popups;
pub(crate) mod share;
#[cfg(test)]
mod tests;

use iced::{Task, window};
use yapfast_core::application::Action;
use yapfast_core::voice::{Command, StreamAudio};

use anomi_vm::shell::Layer;
use anomi_vm::voice::{self, controls, dock, incoming, join, pip, stage, tiles};

use crate::app::App;
use crate::message::Message;
use crate::project::voice::{Focus, Stage};

pub use popups::{close, noise_suppression, open_devices};

/// After each update: the input menu's meter, joining as set, and the wait
/// of an empty private call.
pub fn after(app: &mut App) -> Task<Message> {
    Task::batch([popups::after(app), alone::after(app)])
}

/// What the call's waits brought back.
#[derive(Clone, Debug)]
pub enum Due {
    /// What can be shared was listed.
    Listed(share::Listed),
    /// The preview's audio devices were listed (`Effect::ListDevices`).
    Devices(crate::message::Once<Result<yapfast_core::voice::Devices, String>>),
    /// A picture of a screen for the share menu.
    Glanced(glance::Glanced),
    /// The input menu's microphone can't be read, by the menu's level.
    Unread(anomi_vm::kind::Level),
    /// The wait of an empty private call ended (`alone`), by its number.
    Alone(u64),
}

/// A wait of the call's ended.
pub fn due(app: &mut App, due: Due) -> Task<Message> {
    match due {
        Due::Listed(listed) => share::listed(app, listed),
        Due::Devices(listed) => {
            crate::effects::devices::listed(app, listed);
            Task::none()
        }
        Due::Glanced(glanced) => glance::glanced(app, glanced),
        Due::Unread(level) => {
            popups::unread(app, &level);
            Task::none()
        }
        Due::Alone(number) => alone::ended(app, number),
    }
}

/// Voice's intents.
pub fn handle(app: &mut App, intent: voice::Intent) -> Task<Message> {
    use voice::Intent as I;
    match intent {
        I::Dock(intent) => dock(app, intent),
        I::Controls(intent) => controls(app, intent),
        I::Tiles(intent) => tiles(app, intent),
        I::Stage(stage::Intent::ShowAll) => focus(app, None),
        I::Stage(stage::Intent::Placed(placement)) => {
            app.shown.voice.placement = Some(placement);
            place(app);
            Task::none()
        }
        I::Pip(intent) => corner(app, intent),
        I::Join(join::Intent::Join) => match app.core.state.selected.clone() {
            Some(room) => app.dispatch(Action::RequestJoinVoice(room)),
            None => Task::none(),
        },
        I::Join(join::Intent::Answer { room, accept }) => answer(app, room, accept),
        I::Incoming(incoming::Intent::Accept(room)) => answer(app, room, true),
        I::Incoming(incoming::Intent::Decline(room)) => answer(app, room, false),
        I::Incoming(incoming::Intent::ConfirmSwitch) => app.dispatch(Action::ConfirmVoiceSwitch),
        I::Incoming(incoming::Intent::CancelSwitch) => app.dispatch(Action::CancelVoiceSwitch),
        intent => popups::handle(app, intent),
    }
}

fn answer(app: &mut App, room: String, accept: bool) -> Task<Message> {
    app.dispatch(if accept {
        Action::AcceptCall(room)
    } else {
        Action::DeclineCall(room)
    })
}

/// A command to the call.
pub(crate) fn command(app: &mut App, command: Command) -> Task<Message> {
    app.dispatch(Action::Voice(command))
}

fn dock(app: &mut App, intent: dock::Intent) -> Task<Message> {
    use dock::Intent as I;
    match intent {
        I::Leave => command(app, Command::Leave),
        I::DismissError => app.dispatch(Action::DismissVoiceError),
        I::ToggleCamera => camera::click(app),
        I::StopSharing => command(app, Command::StopLive),
        I::ToggleStreamSound => stream_sound(app),
        I::ToggleNoise => popups::toggle_noise(app),
        I::OpenShareMenu(spot) => popups::open_share(app, spot),
        I::OpenSoundboard(spot) => popups::open_soundboard(app, spot),
        I::OpenLink(spot) => popups::open_link(app, spot),
        I::ReadLink => popups::read_link(app),
        I::OpenCall => app.dispatch(Action::GoToCall),
        intent => {
            tracing::debug!(?intent, "not the dock's");
            Task::none()
        }
    }
}

/// The stream's sound off, then back to what it carried (`toggle-stream-sound`).
fn stream_sound(app: &mut App) -> Task<Message> {
    let now = app.core.state.voice.stream_audio;
    let sound = if now == StreamAudio::Off {
        app.shown.voice.heard
    } else {
        app.shown.voice.heard = now;
        StreamAudio::Off
    };
    command(app, Command::StreamAudio(sound))
}

fn controls(app: &mut App, intent: controls::Intent) -> Task<Message> {
    use controls::Intent as I;
    match intent {
        I::ToggleMute => crate::intent::account::handle(app, anomi_vm::account::Intent::ToggleMute),
        I::ToggleDeafen => {
            crate::intent::account::handle(app, anomi_vm::account::Intent::ToggleDeafen)
        }
        I::ToggleCamera => camera::click(app),
        I::StopSharing => command(app, Command::StopLive),
        I::OpenShareMenu(spot) => popups::open_share(app, spot),
        I::OpenSoundboard(spot) => popups::open_soundboard(app, spot),
        I::Leave => command(app, Command::Leave),
        intent => {
            tracing::debug!(?intent, "not the controls'");
            Task::none()
        }
    }
}

fn tiles(app: &mut App, intent: tiles::Intent) -> Task<Message> {
    use tiles::Intent as I;
    match intent {
        I::Focus(key) => {
            let chosen = match key {
                tiles::Key::OwnScreen => Focus::OwnScreen,
                tiles::Key::Stream(id) => Focus::Stream(id),
                tiles::Key::Person(id) => {
                    let voice = &app.core.state.voice;
                    let stage = Stage {
                        participants: &voice.participants,
                        sharing: voice.sharing.is_some(),
                    };
                    let slot = stage.slot(&Focus::Person(id.clone()));
                    match slot.and_then(|slot| stage.focus_at(slot)) {
                        Some(focus) => focus,
                        None => Focus::Person(id),
                    }
                }
            };
            focus(app, Some(chosen))
        }
        I::Watch(id) => command(app, Command::Watch(id)),
        I::StopWatching => command(app, Command::StopWatching),
        I::FullScreen(on) => full_screen(app, on),
        I::HideOthers(hidden) => {
            app.shown.voice.others_hidden = hidden;
            app.show(crate::project::Reach::VOICE);
            Task::none()
        }
        I::Menu(user, spot) => match app.core.state.voice.room.clone() {
            Some(room) => crate::intent::menus::open_voice(app, room, user, spot),
            None => Task::none(),
        },
        intent => {
            tracing::debug!(?intent, "not the tiles'");
            Task::none()
        }
    }
}

/// Shows `focus` large, or the grid again; hiding the others and full
/// screen end with it.
fn focus(app: &mut App, focus: Option<Focus>) -> Task<Message> {
    let task = if focus.is_none() && app.vm.shell.is_open(Layer::FullStage) {
        full_screen(app, false)
    } else {
        Task::none()
    };
    if focus.is_none() {
        app.shown.voice.others_hidden = false;
    }
    app.shown.voice.focus = focus;
    app.show(crate::project::Reach::VOICE);
    task
}

/// The large tile over the whole window, in the desktop's full screen, or
/// back (`full-screen`; upstream's 5d85412c): the window's bar hides.
pub(crate) fn full_screen(app: &mut App, on: bool) -> Task<Message> {
    if on {
        app.vm.shell.open(Layer::FullStage);
    } else if app.vm.shell.is_open(Layer::FullStage) {
        app.vm.shell.close(Layer::FullStage);
    }
    app.show(crate::project::Reach::VOICE);
    let mode = if on {
        window::Mode::Fullscreen
    } else {
        window::Mode::Windowed
    };
    match app.window {
        Some(id) => window::set_mode(id, mode),
        None => Task::none(),
    }
}

/// The feeds and the frames follow where the tiles are now.
fn place(app: &mut App) {
    if !app.hidden {
        let shown = &mut app.shown;
        crate::project::voice::place(&app.core, &app.vm, &shown.voice, &mut shown.media);
    }
}

fn corner(app: &mut App, intent: pip::Intent) -> Task<Message> {
    use pip::Intent as I;
    match intent {
        I::Open => {
            let slot = app.shown.voice.pip.and_then(|(_, slot)| slot);
            let voice = &app.core.state.voice;
            let stage = Stage {
                participants: &voice.participants,
                sharing: voice.sharing.is_some(),
            };
            app.shown.voice.focus = slot.and_then(|slot| stage.focus_at(slot));
            app.dispatch(Action::GoToCall)
        }
        I::Stop if app.core.state.voice.watching.is_some() => command(app, Command::StopWatching),
        I::Stop => command(app, Command::StopLive),
        I::Drag(offset) => {
            app.vm.voice.pip.drag = offset;
            Task::none()
        }
        I::Dropped => {
            popups::drop_corner(app);
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not the corner's");
            Task::none()
        }
    }
}
