//! The camera button, in the dock and the call controls: on, off, and what
//! a click does with no camera to film with.
//!
//! The core knows the cameras from its device list (`voice::State::cameras`),
//! which the call's worker makes on `Command::Devices`. The app asks for it
//! as a call starts ([`starts_a_call`]), as a device menu or Voice & Video
//! opens, and with each click that turns the camera on, so the button's
//! unavailable look (`project::voice::camera_unavailable`) follows a camera
//! plugged in or taken out without anything polling.
//!
//! A click while the button looks unavailable lists the cameras again
//! first, as one may have been plugged in since the list was made
//! ([`listed`]): one found films as a click on it does, and with none
//! Discord's dialog opens, "Camera unavailable" with its OK
//! (`Layer::CameraUnavailable`, `anomi_screens::shell::camera`). Nothing
//! goes to the call then, so the dock shows no notice; its notice stays for
//! the call's other errors.

use iced::Task;
use yapfast_core::application::{Action, AppState};
use yapfast_core::chat::Event;
use yapfast_core::voice::{self, CameraPhase, Command};

use anomi_vm::shell::Layer;

use super::Due;
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

/// A click with no camera listed: the cameras are listed again on the
/// core's blocking pool, as listing opens each device, and [`listed`]
/// answers.
fn unavailable(app: &mut App) -> Task<Message> {
    let runtime = app.runtime.clone();
    // Listed as the task runs, not before.
    let listing = async move {
        runtime
            .spawn_blocking(|| !yapfast_core::camera::list().is_empty())
            .await
    };
    Task::perform(listing, |found| {
        Message::Voice(Due::Cameras(found.unwrap_or(false)))
    })
}

/// The cameras listed again after a click with none listed: one plugged in
/// since films, as the click does with it listed; with none, Discord's
/// dialog opens with its OK focused, and nothing goes to the call. A call
/// left, or a camera turned on, since the click asks nothing.
pub(crate) fn listed(app: &mut App, found: bool) -> Task<Message> {
    let voice = &app.core.state.voice;
    if !voice.active() || voice.camera_phase != CameraPhase::Off {
        return Task::none();
    }
    if found {
        return turn(app, true);
    }
    app.vm.shell.open(Layer::CameraUnavailable);
    app.shown.focus = Some(anomi_screens::shell::camera::OK);
    Task::none()
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
    use yapfast_lang::tr;

    use super::*;
    use crate::project::voice::tests::in_call;

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

    /// The shell's close, which Escape, the X, "OK", Enter and a press
    /// beside the dialog all send, and the way out that follows.
    fn dismiss(app: &mut App) {
        use anomi_vm::shell::Intent;
        let layer = Layer::CameraUnavailable;
        drop(crate::intent::shell::handle(app, Intent::Close(layer)));
        drop(crate::intent::shell::handle(app, Intent::Gone(layer)));
    }

    #[test]
    fn with_none_found_the_click_opens_the_dialog_and_no_notice() {
        let (mut app, folder) = in_call("camera-none");
        drop(click(&mut app));
        let open = |app: &App| app.vm.shell.is_open(Layer::CameraUnavailable);
        assert!(!open(&app), "not before the cameras are listed");
        drop(listed(&mut app, false));
        assert!(open(&app));
        assert_eq!(app.shown.focus, Some(anomi_screens::shell::camera::OK));
        assert_eq!(app.core.state.voice.error, None, "nothing went to the call");
        assert!(
            app.vm.voice.dock.error.is_empty(),
            "the dock shows no notice"
        );
        dismiss(&mut app);
        assert!(app.vm.shell.opened(Layer::CameraUnavailable).is_none());
        // Answered after the call ended, the listing opens nothing.
        app.core.state.voice.phase = Phase::Disconnected;
        drop(listed(&mut app, false));
        assert!(!open(&app));
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn with_a_camera_the_click_turns_it_on_and_opens_nothing() {
        let (mut app, folder) = in_call("camera-found");
        // The commands went to the call, which a test has none of.
        let sent = |app: &App| {
            app.core.state.voice.error.as_deref()
                == Some(tr!("Voice is unavailable. Reconnect and retry."))
        };
        // Plugged in since the list was made: found as the click lists.
        drop(listed(&mut app, true));
        assert!(sent(&app));
        assert!(!app.vm.shell.is_open(Layer::CameraUnavailable));
        // Listed already: the click turns it on at once.
        app.core.state.voice.error = None;
        app.core.state.voice.cameras = webcam();
        drop(click(&mut app));
        assert!(sent(&app));
        assert!(!app.vm.shell.is_open(Layer::CameraUnavailable));
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }
}
