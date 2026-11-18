//! The header's search and its results (`HeaderSearch`, `SearchPanel`:
//! `search`, `clear`, `leave-search`, `jump-in-room`, `more-results`,
//! `close`), and the pins popover (`PinsPopover`: `jump`, `close`).
//!
//! Typing changes the field's text alone, which the view reads: nothing is
//! shown again and nothing reaches the core until Enter, whose search drops
//! the pages of the one before. Ending the search empties the field.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat::pins::Intent as PinsIntent;
use anomi_vm::chat::search::Intent;
use anomi_vm::shell::Layer;
use anomi_widgets::focus;

use crate::app::App;
use crate::intent::chat::open_link;
use crate::message::Message;

pub(super) fn search(app: &mut App, intent: Intent) -> Task<Message> {
    let field = &mut app.vm.chat.search;
    match intent {
        Intent::Edit(text) => {
            field.text = text;
            Task::none()
        }
        Intent::Submit => {
            let text = field.text.clone();
            app.dispatch(Action::Search(text))
        }
        Intent::Clear | Intent::Close => end(app),
        // Escape: the keys go back to the composer used last, and a
        // search with words ends.
        Intent::Leave { cleared } => {
            let ended = if cleared { end(app) } else { Task::none() };
            let aside = app.aside_last && app.core.state.aside.is_some();
            let composer = if aside {
                focus::ASIDE_COMPOSER
            } else {
                focus::COMPOSER
            };
            Task::batch([ended, focus::into(composer)])
        }
        Intent::Focus(focused) => {
            field.focused = focused;
            Task::none()
        }
        Intent::Jump { room, item } => app.dispatch(Action::JumpInRoom { room, item }),
        Intent::Link(link) => open_link(app, link),
        Intent::More => app.dispatch(Action::MoreResults),
        intent => {
            tracing::debug!(?intent, "not the search's");
            Task::none()
        }
    }
}

/// The field empties and the search ends.
fn end(app: &mut App) -> Task<Message> {
    app.vm.chat.search.text.clear();
    app.dispatch(Action::Search(String::new()))
}

pub(super) fn pins(app: &mut App, intent: PinsIntent) -> Task<Message> {
    match intent {
        // The popover closes first, then the message shows in its room.
        PinsIntent::Jump { room, item } => {
            let closed = crate::intent::header::close(app, Layer::Pins);
            Task::batch([closed, app.dispatch(Action::JumpInRoom { room, item })])
        }
        PinsIntent::Link(link) => open_link(app, link),
        PinsIntent::Close => crate::intent::header::close(app, Layer::Pins),
        intent => {
            tracing::debug!(?intent, "not the pins'");
            Task::none()
        }
    }
}

/// The header's field shows the magnifier alone, until used, in a
/// conversation narrower than the header's room for it (`compact:
/// root.conversation-room < 560px`), as the window's size and the
/// sidebar's say after any update.
pub(crate) fn compact(app: &mut App) {
    let (vm, width) = (&app.vm, app.size.width);
    let docking = anomi_screens::shell::Docking::new(&vm.shell, &vm.conversation.beside, width);
    let compact = docking.room < anomi_screens::header::NARROW;
    if app.vm.chat.search.compact != compact {
        app.vm.chat.search.compact = compact;
    }
}
