//! The tray icon's choices (`chosen` in `src/ui/shell.rs`): Open Anomi
//! shows the window, Disconnect leaves the call, Quit quits; and a tray
//! that comes or goes decides what closing the window does. The icon itself
//! is `anomi_desktop::tray`'s, and `platform::show` keeps its dot.

use anomi_desktop::tray::Choice;
use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::voice;

use crate::app::App;
use crate::message::Message;

/// What the person chose in the tray, or what it said.
pub fn chosen(app: &mut App, choice: Choice) -> Task<Message> {
    match choice {
        Choice::Open => {
            let token = anomi_desktop::tray::take_activation_token();
            super::window::open(app, token)
        }
        Choice::Disconnect => app.dispatch(Action::Voice(voice::Command::Leave)),
        Choice::Quit => app.quit(),
        // Without a tray to open it from, the window can't stay closed.
        Choice::Available if app.window.is_none() && !app.desktop.tray_shows() => {
            tracing::info!("the tray went; opening the window");
            super::window::open(app, None)
        }
        Choice::Available => Task::none(),
    }
}

/// The menu and tooltip in the language chosen since.
pub fn reword(app: &App) {
    if let Some(tray) = &app.desktop.tray {
        tray.reword();
    }
}
