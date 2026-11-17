//! anomi-people's areas driven headless through iced's simulator: one module
//! per area, its task's (docs/iced/tasks.md), each testing its intents over
//! its fixtures. One test binary for the crate, so a change links once. F01
//! keeps this file.

/// What every area's tests share: the simulator's settings with the
/// interface's fonts, a window in front, and the intents it gathered.
#[allow(dead_code)]
mod support {
    use std::borrow::Cow;

    use iced::{Settings, Size};
    use iced_test::Simulator;

    use anomi_theme::{Motion, Renderer, Theme, fonts, typography};

    /// The golden screenshots' window.
    pub const SIZE: Size = Size::new(1400.0, 860.0);

    pub fn settings() -> Settings {
        Settings {
            fonts: fonts::ALL.iter().map(|font| Cow::Borrowed(*font)).collect(),
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

mod active_now;
mod activity;
mod add_friend;
mod add_people;
mod badges;
mod cards;
mod editor;
mod editor_preview;
mod friends;
mod full_profile;
mod members;
mod name_style;
mod person;
mod profile;
mod requests;
mod snapshots;
mod status;
