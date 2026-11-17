//! anomi-screens' areas driven headless through iced's simulator: one module
//! per area, its task's (docs/iced/tasks.md), each testing its intents over
//! its fixtures: a click on a row asks to open it, a click on a server asks
//! for it, typing in the search field filters. One test binary for the
//! crate, so a change links once. F01 keeps this file.

/// What every area's tests share: the simulator's settings with the
/// interface's fonts, a window in front, the whole window over a view-model,
/// and the intents it gathered.
#[allow(dead_code)]
mod support {
    use std::borrow::Cow;

    use iced::{Settings, Size};
    use iced_test::Simulator;

    use anomi_theme::{Element, Motion, Renderer, Theme, fonts, typography};
    use anomi_vm::{Intent, Vm};

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

    /// The whole window over a view-model, in front.
    pub fn window(vm: &Vm) -> Element<'_, Intent> {
        use iced::Widget as _;
        anomi_screens::window(vm, front()).boxed()
    }

    /// A simulator of the whole window at the golden size.
    pub fn simulate(vm: &Vm) -> Simulator<'_, Intent, Theme, Renderer> {
        Simulator::with_size(settings(), SIZE, window(vm))
    }
}

mod account;
mod browse;
mod channels;
mod chrome;
mod dms;
mod event;
mod gift;
mod header;
mod inbox;
mod invite;
mod join;
mod leave;
mod leaving_link;
mod menu;
mod new_dm;
mod oauth;
mod rail;
mod server_header;
mod shell;
mod sidebar;
mod sign_in;
mod snapshots;
mod switcher;
