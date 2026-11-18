//! The pickers' intents: the emoji picker's, the GIFs' and the stickers',
//! and the tabs and the dragged edge of the picker above the composer
//! (`src/ui/callbacks.rs:426`, `:557`, `:575`, `:1856`-`:1902`;
//! `src/ui/gifs.rs`, `src/ui/stickers.rs`). Where they open from is
//! [`open`]'s; what a picked emoji goes to, [`emoji`]'s.
//!
//! Typing in a search changes the picker alone: the emoji are searched
//! here, and the stickers' and GIFs' searches reach the picker alone
//! (`Reach::PICKER`). A picker that closes lets go of what it held: the
//! core of its GIFs, stickers and custom emoji pictures, the view-model of
//! its rows once it played its way out.

mod anchor;
mod emoji;
mod gifs;
pub(crate) mod open;
mod stickers;
#[cfg(test)]
mod tests;

pub(crate) use open::{for_edit, for_message, for_poll, for_status};

use std::time::Duration;

use iced::{Rectangle, Task};

use anomi_vm::chat::composer::Expression;
use anomi_vm::chat::pickers::{self, Tab};
use anomi_vm::chat::{Intent, emoji_picker, sticker_picker};
use anomi_vm::shell::Layer;

use crate::app::{App, later};
use crate::message::Message;
use crate::project::chat::pickers as project;

/// What comes back to the pickers after a wait.
#[derive(Clone, Debug)]
pub enum Due {
    /// Where the composer's buttons and its top are, for the picker one of
    /// them asked for.
    Anchored {
        which: Expression,
        found: [Option<Rectangle>; 2],
    },
    /// A pause in typing into the GIFs' search ended, by the count of what
    /// was typed: the last searches.
    Search(u64),
    /// A picker went a moment ago: the heap gives back what it freed.
    Trim,
}

/// `Pickers`, `EmojiPicker`, `GifPicker` and `StickerPicker`.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Pickers(intent) => expressions(app, intent),
        Intent::EmojiPicker(intent) => emoji::handle(app, intent),
        Intent::GifPicker(intent) => gifs::handle(app, intent),
        Intent::StickerPicker(intent) => stickers::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not the pickers'");
            Task::none()
        }
    }
}

/// What came back after a wait.
pub(crate) fn due(app: &mut App, due: Due) -> Task<Message> {
    match due {
        Due::Anchored { which, found } => open::anchored(app, which, found),
        Due::Search(typed) => gifs::search_due(app, typed),
        Due::Trim => {
            trim();
            Task::none()
        }
    }
}

/// The picker above the composer's own: its tabs and its dragged edge.
fn expressions(app: &mut App, intent: pickers::Intent) -> Task<Message> {
    match intent {
        pickers::Intent::Tab(tab) => open::tab(app, tab),
        pickers::Intent::Drag(wanted) => {
            let width = app.vm.chat.pickers.dragged(wanted);
            app.vm.chat.pickers.width = width;
            fit(app);
            Task::none()
        }
        // Kept for the next start once changes stop, and the GIFs asked for
        // at their new sizes.
        pickers::Intent::Chosen(width) => {
            app.layout.edit(|saved| saved.picker_width = Some(width));
            gifs::ask(app)
        }
        intent => {
            tracing::debug!(?intent, "not the picker's");
            Task::none()
        }
    }
}

/// The grids fit their columns to the width the picker shows at.
fn fit(app: &mut App) {
    let pickers = &mut app.vm.chat.pickers;
    let width = pickers.shown();
    pickers.emoji.columns = emoji_picker::columns(width);
    pickers.stickers.columns = sticker_picker::columns(width);
}

/// Closes a picker through what it held: the core lets go of the GIFs and
/// stickers it listed and of the custom emoji pictures only it showed, and
/// the keys go back where they were (`expressions-open`, `picker-open`,
/// `ui/yapfast.slint:2476`, `:2527`).
pub(crate) fn close(app: &mut App, layer: Layer) -> Task<Message> {
    if !app.vm.shell.is_open(layer) {
        return Task::none();
    }
    app.vm.shell.close(layer);
    let purpose = match layer {
        Layer::Expressions => {
            let tab = app.vm.chat.pickers.tab;
            app.vm.chat.composer.open = None;
            let task = open::leave(app, tab);
            let back = anomi_widgets::focus::to(anomi_widgets::focus::COMPOSER);
            return Task::batch([task, back]);
        }
        _ => app.vm.chat.emoji_picker.purpose,
    };
    let task = emoji::let_go(app);
    Task::batch([task, emoji::refocus(purpose)])
}

/// A picker played its way out: its rows go, and once the frames without
/// it are drawn and the last pictures it asked for came and went, the heap
/// gives back what that freed (`let_go_after_picker`, `src/ui.rs:1248`):
/// glibc keeps it otherwise.
pub(crate) fn gone(app: &mut App) -> Task<Message> {
    project::gone(&mut app.vm, &mut app.shown.pickers);
    let after = |seconds| later(Duration::from_secs(seconds), Message::Pickers(Due::Trim));
    Task::batch([after(1), after(5)])
}

/// Gives the heap's free memory back to the system.
fn trim() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: glibc's malloc_trim only returns free pages of its own
        // heap; it takes no pointer.
        unsafe { malloc_trim(0) };
    }
}

/// What runs after every update: the GIFs or stickers listed again ask for
/// the pictures near the view. Nothing while neither was.
pub(crate) fn after(app: &mut App) -> Task<Message> {
    let shown = &app.shown.pickers;
    if shown.gifs.ask {
        return gifs::ask(app);
    }
    if shown.stickers.ask {
        return stickers::ask(app);
    }
    Task::none()
}

/// The picker above the composer shows `tab`'s grid.
fn showing(app: &App, tab: Tab) -> bool {
    app.vm.shell.is_open(Layer::Expressions) && app.vm.chat.pickers.tab == tab
}
