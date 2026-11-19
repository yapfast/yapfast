//! The camera button, in the dock and the call controls: on, off, and what
//! a click does with no camera to film with.
//!
//! The core knows the cameras from its device list (`voice::State::cameras`),
//! which the call's worker makes on `Command::Devices`. The app asks for it
//! as a call starts ([`starts_a_call`]), as a device menu or Voice & Video
//! opens, and with each click that turns the camera on, so the button's
//! unavailable look (`project::voice::camera_unavailable`) follows a camera
//! plugged in or taken out without anything polling.

use iced::Task;
use yapfast_core::application::{Action, AppState};
use yapfast_core::chat::Event;
use yapfast_core::voice::{self, CameraPhase, Command};

use crate::app::App;
use crate::message::Message;

/// What a click on the camera button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Click {
    /// Turn it on, or off (a click while it starts cancels it).
    Turn(bool),
    /// No camera is listed: see [`unavailable`].
    Unavailable,
}

/// What a click asks, from the call as the core has it.
pub(crate) fn click_on(voice: &voice::State) -> Click {
    if crate::project::voice::camera_unavailable(voice) {
        Click::Unavailable
    } else {
        Click::Turn(voice.camera_phase == CameraPhase::Off)
    }
}

/// The camera button pressed.
pub(super) fn click(app: &mut App) -> Task<Message> {
    match click_on(&app.core.state.voice) {
        Click::Turn(on) => turn(app, on),
        Click::Unavailable => unavailable(app),
    }
}

/// A click with no camera listed. Discord opens a dialog here ("Camera
/// unavailable", "Oops, we couldn't find any cameras.", OK); Anomi asks for
/// the camera all the same: one plugged in since the list was made films,
/// and with none the core's "No camera found" shows in the dock. To show a
/// dialog instead, open it here.
fn unavailable(app: &mut App) -> Task<Message> {
    turn(app, true)
}

/// The commands that turn the camera on or off: turning it on lists the
/// devices again, so the button follows what the core found.
pub(crate) fn commands(on: bool) -> Vec<Command> {
    if on {
        vec![Command::Video(true), Command::Devices]
    } else {
        vec![Command::Video(false)]
    }
}

fn turn(app: &mut App, on: bool) -> Task<Message> {
    let tasks: Vec<_> = (commands(on).into_iter())
        .map(|command| super::command(app, command))
        .collect();
    Task::batch(tasks)
}

/// The event starts a call: the voice state has a room where it had none.
pub(crate) fn starts_a_call(state: &AppState, event: &Event) -> bool {
    matches!(event, Event::Voice(call) if call.room.is_some() && state.voice.room.is_none())
}

/// Lists the devices once a call starts, for the camera button: the worker
/// lists them while it connects, so they come with the call's state.
pub(crate) fn list(app: &mut App) -> Task<Message> {
    let effects = app.core.dispatch(Action::Voice(Command::Devices));
    crate::effects::run(app, effects)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use yapfast_core::voice::{Device, Phase, State};

    use super::*;

    fn webcam() -> Arc<[Device]> {
        Arc::from([Device {
            id: "/dev/video0".into(),
            name: "Webcam".into(),
        }])
    }

    #[test]
    fn with_no_camera_a_click_is_the_unavailable_ones() {
        let mut voice = State::default();
        assert_eq!(click_on(&voice), Click::Unavailable);
        voice.cameras = webcam();
        assert_eq!(click_on(&voice), Click::Turn(true));
        voice.camera_phase = CameraPhase::On;
        assert_eq!(click_on(&voice), Click::Turn(false));
        // One that films, or starts, turns off though the list has none.
        voice.cameras = Arc::default();
        voice.camera_phase = CameraPhase::Starting;
        assert_eq!(click_on(&voice), Click::Turn(false));
    }

    #[test]
    fn turning_the_camera_on_lists_the_devices_again() {
        assert!(matches!(
            commands(true).as_slice(),
            [Command::Video(true), Command::Devices]
        ));
        assert!(matches!(
            commands(false).as_slice(),
            [Command::Video(false)]
        ));
    }

    #[test]
    fn a_call_starts_when_the_voice_state_first_has_a_room() {
        let mut state = AppState::default();
        let joining = State {
            phase: Phase::Joining,
            room: Some("discord:lounge".into()),
            ..State::default()
        };
        assert!(starts_a_call(&state, &Event::Voice(joining.clone())));
        assert!(!starts_a_call(&state, &Event::Voice(State::default())));
        state.voice = joining.clone();
        assert!(!starts_a_call(&state, &Event::Voice(joining)));
    }
}
