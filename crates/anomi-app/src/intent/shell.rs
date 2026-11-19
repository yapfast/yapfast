//! The window frame's intents: the panels' edges, the layers over the
//! window, the notice and a refused drop; and the header's two switches the
//! layout keeps, the member list and a DM's profile
//! (`src/ui/callbacks.rs:1964`-`:2012`).
//!
//! A layer an area opened closes through [`close`]: the area whose model
//! holds the layer adds its arm there (its dialog's Cancel, its page's
//! Close), and clears its model on [`shell::Intent::Gone`] once the layer
//! played its way out. Any other layer just closes. The window going behind
//! another closes the transient ones the same way ([`unfocused`]).

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
            match layer {
                Layer::MessageMenu => {
                    super::menus::message::gone(app);
                    super::chat::let_go(app);
                }
                Layer::Forward => crate::project::forward_gone(&mut app.vm, &mut app.shown),
                // A bot's form or menu, the poll being made, a card (I08).
                Layer::BotModal | Layer::BotSelect | Layer::Poll | Layer::MessageCard => {
                    super::chat::bots::gone(app, layer);
                }
                Layer::Expressions | Layer::EmojiPicker => {
                    super::chat::let_go(app);
                    return super::chat::pickers::gone(app);
                }
                // The picture or the video large lets go of it (I09).
                Layer::Viewer | Layer::Video => crate::media::gone(app, layer),
                // The person's settings hold nothing until they open (I19).
                Layer::Settings => super::settings::gone(app),
                // A server's and a channel's settings, and their dialogs (I20).
                layer if super::server_settings::holds(layer) => {
                    super::server_settings::gone(app, layer);
                }
                // The editor's pictures go with it (I17).
                Layer::ProfileEditor => {
                    let (state, editor) = (&app.core.state, &mut app.shown.editor);
                    crate::project::editor::gone(state, &mut app.vm, editor);
                }
                Layer::FolderSettings => super::rail::folder::gone(app),
                _ => {}
            }
            if layer == Layer::Inbox {
                crate::project::inbox_gone(&mut app.vm, &mut app.shown);
            }
            crate::project::popover_gone(layer, &mut app.vm, &mut app.shown);
            crate::project::places_gone(layer, &mut app.vm, &mut app.shown);
            if layer == Layer::Reactions {
                crate::project::reactions_gone(&mut app.vm, &mut app.shown);
            }
            let (state, people) = (&app.core.state, &mut app.shown.people);
            crate::project::people::gone(state, layer, &mut app.vm, people);
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
    match layer {
        // The core lets the dialog go, and the projection closes it (I03).
        Layer::Status => super::status::close(app),
        // The header's popovers: the core lets go what they list (I04).
        Layer::Pins | Layer::Threads | Layer::AddPeople => super::header::close(app, layer),
        Layer::Inbox => {
            super::inbox::close(app);
            Task::none()
        }
        // The prompt's request fails, and the linking over the settings
        // stops; the projection closes them as the core lets them go (I05).
        Layer::Mfa => super::sign_in::cancel_mfa(app),
        Layer::Link => super::sign_in::close_link(app),
        // The dialogs about places close through the core, and the
        // projection closes them as it lets them go (I06).
        Layer::Switcher
        | Layer::NewDm
        | Layer::Invite
        | Layer::Join
        | Layer::Event
        | Layer::Gift
        | Layer::OAuth
        | Layer::Leave
        | Layer::LeavingLink => super::places::close(app, layer),
        // The core lets the window go, and the projection closes it (I07).
        Layer::Reactions => app.dispatch(Action::CloseReactions),
        // The core stops forwarding, and the projection closes it (I10).
        Layer::Forward => super::forward::close(app),
        // A bot's form or menu, and the poll being made, close through the
        // core, which the projection follows; a card as itself (I08).
        Layer::BotModal | Layer::BotSelect | Layer::Poll | Layer::MessageCard => {
            super::chat::bots::close(app, layer)
        }
        // The core lets go of what the picker listed (I12).
        Layer::Expressions | Layer::EmojiPicker => super::chat::pickers::close(app, layer),
        // The core lets the picture go, and the projection closes it; the
        // video goes back to its cell (I09).
        Layer::Viewer | Layer::Video => crate::media::close(app, layer),
        // The profiles, the dialogs and cards about people close through
        // the core, which the projection follows (I16).
        Layer::Profile
        | Layer::FullProfile
        | Layer::Person
        | Layer::AddFriend
        | Layer::RoleCard
        | Layer::Card => super::people::close(app, layer),
        // The name style window, a picker or the text being edited go
        // first; then the editor, unless it has changes to save (I17).
        Layer::ProfileEditor => super::editor::escape(app),
        // The call's popups and full screen, and the question before
        // switching calls (I18).
        Layer::Devices
        | Layer::Connection
        | Layer::Share
        | Layer::Soundboard
        | Layer::FullStage
        | Layer::SwitchCall => super::voice::close(app, layer),
        // A folder's settings go with the colour picker over them,
        // applying nothing.
        Layer::FolderSettings => {
            super::rail::folder::close(app);
            Task::none()
        }
        // A participant's card goes with the menu under it (I10).
        Layer::ParticipantMenu => {
            super::menus::close(app);
            app.vm.shell.close(layer);
            Task::none()
        }
        // What Settings started ends with it, linking Discord too (I19).
        Layer::Settings => super::settings::close(app),
        // A server's and a channel's settings and their dialogs ask their
        // page, which the projection follows (I20).
        layer if super::server_settings::holds(layer) => super::server_settings::close(app, layer),
        layer => {
            app.vm.shell.close(layer);
            Task::none()
        }
    }
}

/// The window went behind another (its own `Unfocused`, through
/// `window::focused`): what hangs off it closes, as Discord's menus and
/// popouts do, each through its own close as Escape closes it, so the core
/// lets go what it showed: the window's menu with its submenus and the
/// participant's card over it, every transient layer ([`Layer::transient`])
/// from the top, and the composer's suggestions. What the person is in the
/// middle of stays. Nothing awaited hangs on what closes here: a file
/// dialog's answer goes to the core or the composer, and Go Live closes its
/// menu before the system's dialog asks.
pub(crate) fn unfocused(app: &mut App) -> Task<Message> {
    if app.vm.menu.open {
        super::menus::close(app);
    }
    let open: Vec<Layer> = (app.vm.shell.layers.iter())
        .filter(|opened| opened.open && opened.layer.transient())
        .map(|opened| opened.layer)
        .collect();
    let mut tasks = Vec::with_capacity(open.len() + 1);
    for layer in open.into_iter().rev() {
        // One may have closed with another before it.
        if app.vm.shell.is_open(layer) {
            tasks.push(close(app, layer));
        }
    }
    tasks.push(super::chat::composer::close_suggestions(app));
    Task::batch(tasks)
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
