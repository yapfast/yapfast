//! Apps' commands in the window: the `/` picker, the options of the command
//! being written, and the "Apps" submenus. A stub that phase 1b of the bot
//! commands fills in (see bots-gap-analysis.md): its callbacks reach the
//! core's stub actions, and nothing fills its properties yet.

use super::*;

/// Binds the command picker's and the "Apps" submenus' callbacks.
pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_slash_query(move |query| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::SlashQuery(query.to_string()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_pick_command(move |index| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::PickCommand(index.max(0) as usize));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_set_command_option(move |index, value| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::SetCommandOption {
                    index: index.max(0) as usize,
                    value: value.to_string(),
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_run_app_command(move |command, target| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::RunAppCommand {
                    command: command.to_string(),
                    target: (!target.is_empty()).then(|| target.to_string()),
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_dismiss_ephemeral(move |item| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::DismissEphemeral(item.to_string()));
        }
    });
}

impl Adapter {
    /// Fills the command picker, the option pills and the "Apps" submenus
    /// from the core's state. Nothing to fill yet.
    pub(super) fn show_app_commands(&mut self, _window: &YapfastWindow) {}
}
