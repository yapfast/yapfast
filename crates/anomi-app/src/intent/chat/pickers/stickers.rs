//! The sticker picker's intents (`src/ui/stickers.rs:592`): the search
//! lists again, a sticker is sent as a message of its own, its star keeps it
//! on this device, the rail goes to a section, and the grid's scroll keeps
//! the pictures of the rows near the view and of every section's icon, the
//! rest let go of.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat::pickers::Tab;
use anomi_vm::chat::sticker_picker::Intent;
use anomi_vm::shell::Layer;
use anomi_widgets::list::{Place, operation::jump_to};

use super::showing;
use crate::app::App;
use crate::message::Message;

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let picker = &mut app.vm.chat.pickers.stickers;
    match intent {
        Intent::Search(text) => {
            picker.shown = (0, 0);
            app.dispatch(Action::SearchStickers(text))
        }
        Intent::Pick(id) => {
            let task = app.dispatch(Action::PickSticker(id));
            Task::batch([task, super::close(app, Layer::Expressions)])
        }
        Intent::Locked => app.dispatch(Action::UnlockNitro),
        // The foot stops naming it.
        Intent::Favorite(id) => {
            picker.hovered = None;
            app.dispatch(Action::FavoriteSticker(id))
        }
        Intent::Hover(hovered) => {
            picker.hovered = Some(hovered);
            Task::none()
        }
        Intent::Leave(left) => {
            if picker.hovered.as_ref() == Some(&left) {
                picker.hovered = None;
            }
            Task::none()
        }
        Intent::Jump(section) => {
            let Some(key) = picker.grid().heading_key(section) else {
                return Task::none();
            };
            let grid = anomi_chat::pickers::sticker::grid_id(&picker.query);
            jump_to(grid, key, Place::Top)
        }
        Intent::Shown(first, last) => {
            if picker.shown == (first, last) {
                return Task::none();
            }
            picker.shown = (first, last);
            picker.let_go_far();
            ask(app)
        }
        intent => {
            tracing::debug!(?intent, "not the sticker picker's");
            Task::none()
        }
    }
}

/// Keeps the pictures of the rows near the view and of every section's
/// icon, the rest let go of; nothing when that is what was asked last.
pub(super) fn ask(app: &mut App) -> Task<Message> {
    app.shown.pickers.stickers.ask = false;
    if !showing(app, Tab::Stickers) {
        return Task::none();
    }
    let shown = &mut app.shown.pickers.stickers;
    let held = shown.held(&app.vm.chat.pickers.stickers);
    if held == shown.asked {
        return Task::none();
    }
    shown.asked.clone_from(&held);
    app.dispatch(Action::StickersShown(held))
}
