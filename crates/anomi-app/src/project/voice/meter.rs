//! The microphone's level in the input menu (`metering` and `meter_tick`,
//! `src/ui/callbacks.rs:3838`-`:3860`). The microphone is measured only
//! while the menu shows in front: behind another window, or in the tray,
//! the measuring stops and the device closes. A task off the window's
//! thread reads it 20 times a second, as Slint's timer did, and stores it
//! in the menu's `Level`, which the meter reads as it draws; it asks a
//! redraw, with no message, only when the level lit or put out a segment,
//! so a level builds no view and a quiet microphone wakes nothing on the
//! window's thread. A microphone that can't be read says so in the menu
//! (`meter-note`, `src/ui/callbacks.rs:3851`).

use std::time::Duration;

use iced::Task;
use iced::futures::stream;
use iced::task::Handle;
use yapfast_core::meter::InputMeter;

use anomi_vm::kind::Level;

use crate::message::Message;

/// How often the level is read (`meter_tick`'s 50 ms).
const EVERY: Duration = Duration::from_millis(50);
/// The meter's segments (`anomi_voice::devices::meter::SEGMENTS`).
const SEGMENTS: f32 = 24.0;

/// The level as the meter can show it: whole segments.
pub fn shown(level: f32) -> f32 {
    ((level.clamp(0.0, 1.0) * SEGMENTS + 0.5).floor() / SEGMENTS).min(1.0)
}

/// The microphone measured for the menu, until dropped.
pub struct Meter {
    microphone: Option<String>,
    level: Level,
    _task: Handle,
}

impl Meter {
    /// Whether it measures `microphone` into `level`.
    pub fn measures(&self, microphone: Option<&str>, level: &Level) -> bool {
        self.microphone.as_deref() == microphone && &self.level == level
    }
}

/// What a reading brings to the window.
enum Read {
    /// The level lit or put out a segment: a redraw, with no message.
    Moved,
    /// The microphone can't be read: the menu says so, once.
    Unread,
}

/// Starts measuring `microphone` into `level`.
pub fn start(microphone: Option<&str>, level: &Level) -> (Meter, Task<Message>) {
    let input = InputMeter::start(microphone);
    let first = (input, level.clone(), false);
    let readings = stream::unfold(first, |(input, level, said)| async move {
        loop {
            tokio::time::sleep(EVERY).await;
            if !said && input.failed() {
                return Some((Read::Unread, (input, level, true)));
            }
            if level.set(shown(input.level())) {
                return Some((Read::Moved, (input, level, said)));
            }
        }
    });
    let measured = level.clone();
    let (task, handle) = Task::run(readings, |read| read)
        .then(move |read| match read {
            Read::Moved => iced_runtime::task::effect(iced_runtime::Action::Window(
                iced_runtime::window::Action::RedrawAll,
            )),
            Read::Unread => Task::done(Message::Voice(crate::intent::voice::Due::Unread(
                measured.clone(),
            ))),
        })
        .abortable();
    let meter = Meter {
        microphone: microphone.map(str::to_owned),
        level: level.clone(),
        _task: handle.abort_on_drop(),
    };
    (meter, task)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_is_kept_in_whole_segments() {
        assert_eq!(shown(0.0), 0.0);
        assert_eq!(shown(1.0), 1.0);
        assert_eq!(shown(0.5), 0.5);
        // Under half a segment's step, nothing new shows.
        assert_eq!(shown(0.5 + 0.4 / SEGMENTS), 0.5);
    }
}
