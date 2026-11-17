//! anomi-voice's areas driven headless through iced's simulator: one module
//! per area, its task's (docs/iced/tasks.md), each testing its intents over
//! its fixtures. One test binary for the crate, so a change links once. F01
//! keeps this file.

/// What every area's tests share: the simulator's settings with the
/// interface's fonts, a window in front, and the intents it gathered.
#[allow(dead_code)]
mod support {
    use iced::{Settings, Size};
    use iced_test::Simulator;

    use anomi_theme::{Motion, Renderer, Theme, fonts, typography};

    /// The golden screenshots' window.
    pub const SIZE: Size = Size::new(1400.0, 860.0);

    /// The simulator's settings. The interface's fonts go in as the program
    /// puts them (`fonts::install`: Twemoji before any system emoji font,
    /// the name faces at every weight), so emoji and names draw as in the
    /// app; the interface's face is the one for text that names none.
    pub fn settings() -> Settings {
        fonts::install();
        Settings {
            font: typography::UI,
            ..Settings::default()
        }
    }

    /// A window in front, where things may move.
    pub fn front() -> Motion {
        Motion {
            front: true,
            ..Motion::default()
        }
    }

    /// The messages a simulator gathered.
    pub fn asked<Message>(ui: Simulator<'_, Message, Theme, Renderer>) -> Vec<Message> {
        ui.into_messages().collect()
    }
}

mod connection;
mod controls;
mod devices;
mod dock;
mod incoming;
mod join;
mod participant_menu;
mod pip;
mod share;
mod snapshots;
mod soundboard;
mod stage;
mod tiles;
