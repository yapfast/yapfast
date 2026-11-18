//! The window frame's intents: the panels' edges, the layers over the
//! window, the notice and a refused drop; and the header's two switches the
//! layout keeps, the member list and a DM's profile
//! (`src/ui/callbacks.rs:1964`-`:2012`).
//!
//! A layer an area opened closes through [`close`]: the area whose model
//! holds the layer adds its arm there (its dialog's Cancel, its page's
//! Close), and clears its model on [`shell::Intent::Gone`] once the layer
//! played its way out. Any other layer just closes.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::shell::{self, Layer, Panel};

use crate::app::App;
use crate::message::Message;

/// The frame's intents.
pub fn handle(app: &mut App, intent: shell::Intent) -> Task<Message> {
    use shell::Intent;
    match intent {
        // Following the pointer: the window only.
        Intent::Resize(panel, size) => {
            set(&mut app.vm.shell, panel, Some(size));
            Task::none()
        }
        Intent::Resized(panel, size) => keep(app, panel, Some(size)),
        Intent::ResetSize(panel) => keep(app, panel, None),
        Intent::Close(layer) => close(app, layer),
        Intent::Gone(layer) => {
            app.vm.shell.gone(layer);
            Task::none()
        }
        Intent::DismissNotice => app.dispatch(Action::DismissNotice),
        Intent::DismissDrop => {
            app.vm.shell.drop = None;
            Task::none()
        }
        // The forum post's bar, the post's own (I13).
        intent @ (Intent::ReactToPost(_) | Intent::FollowPost(_) | Intent::CopyPostLink) => {
            super::chat::post_bar(app, intent)
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Closes a layer: through the area that holds it, which adds its arm
/// here, else as itself.
fn close(app: &mut App, layer: Layer) -> Task<Message> {
    app.vm.shell.close(layer);
    Task::none()
}

/// A drag let go, or a double click on the edge: the size, or the panel's
/// own, kept in the layout.
fn keep(app: &mut App, panel: Panel, size: Option<f32>) -> Task<Message> {
    set(&mut app.vm.shell, panel, size);
    app.layout.edit(|saved| {
        *match panel {
            Panel::Sidebar => &mut saved.sidebar_width,
            Panel::Aside => &mut saved.aside_width,
            Panel::Members => &mut saved.members_width,
            Panel::Call => &mut saved.call_height,
        } = size;
    });
    Task::none()
}

/// A panel's size in the window, `None` for its own.
fn set(shell: &mut shell::Shell, panel: Panel, size: Option<f32>) {
    match panel {
        Panel::Sidebar => {
            shell.sidebar_width = size.unwrap_or(shell::Shell::default().sidebar_width);
        }
        Panel::Aside => shell.aside_width = size,
        Panel::Members => shell.members_width = size,
        Panel::Call => shell.call_height = size,
    }
}

/// The header's member list button: the list shows or goes, and stays so
/// (`member-list-toggled`).
pub fn toggle_members(app: &mut App) -> Task<Message> {
    let hidden = !app.layout.members_hidden;
    app.layout.edit(|saved| saved.members_hidden = hidden);
    app.refresh();
    Task::none()
}

/// A one-to-one DM's profile button: the profile shows or goes beside it,
/// and stays so (`profile-panel-toggled`).
pub fn toggle_profile(app: &mut App) -> Task<Message> {
    let hidden = !app.layout.profile_panel_hidden;
    app.layout.edit(|saved| saved.profile_panel_hidden = hidden);
    app.dispatch(Action::ShowProfilePanel(!hidden))
}

#[cfg(test)]
mod tests {
    use anomi_vm::shell::{Intent, Panel};

    use crate::app::App;
    use crate::layout::Layout;

    #[test]
    fn dragged_edges_and_the_member_list_are_kept() {
        let folder = std::env::temp_dir().join(format!("anomi-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        {
            let mut app = App::for_tests(&folder);
            drop(super::handle(
                &mut app,
                Intent::Resize(Panel::Members, 300.0),
            ));
            assert_eq!(app.vm.shell.members_width, Some(300.0));
            assert_eq!(app.layout.members_width, None, "kept once let go");
            drop(super::handle(
                &mut app,
                Intent::Resized(Panel::Members, 310.0),
            ));
            drop(super::handle(
                &mut app,
                Intent::Resized(Panel::Sidebar, 250.0),
            ));
            drop(super::handle(&mut app, Intent::Resized(Panel::Call, 280.0)));
            drop(super::handle(&mut app, Intent::ResetSize(Panel::Call)));
            assert_eq!(app.vm.shell.call_height, None);
            drop(super::toggle_members(&mut app));
            assert!(app.layout.members_hidden);
        }
        // Dropping the app wrote what waited; the next start has it.
        let layout = Layout::read(&folder);
        assert_eq!(layout.members_width, Some(310.0));
        assert_eq!(layout.sidebar_width, Some(250.0));
        assert_eq!(layout.call_height, None);
        assert!(layout.members_hidden);
        let app = App::for_tests(&folder);
        assert_eq!(app.vm.shell.members_width, Some(310.0));
        assert_eq!(app.vm.shell.sidebar_width, 250.0);
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
