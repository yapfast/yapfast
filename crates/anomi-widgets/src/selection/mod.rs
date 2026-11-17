//! Selecting text across the rows of a list, as a browser selects across a
//! page: a press in a row's text starts a selection, and dragging carries
//! it on through the texts and pictures around it, in reading order, into
//! rows the list builds as it scrolls. Ports `yapfast_selection.rs` in
//! `vendor/i-slint-core` (with the hooks into it in `items/text.rs`,
//! `items/flickable.rs`, `items/image.rs` and `window.rs`),
//! `copy_whole_messages` in `src/ui.rs:892` (the rows never built, copied
//! whole) and `on_selection_at` in `src/ui/callbacks.rs:2579` (what a menu
//! opened over the selection copies). `mark_autoscrolls` (`src/ui.rs:902`)
//! needs no glue here: the [`list`](crate::list) draws its own autoscroll
//! marker, and a selection leaves an autoscroll alone. Task F09.
//!
//! - **What it covers.** [`selection`] wraps what scrolls the rows, the
//!   timeline's [`list`](crate::list). Each row is wrapped in [`row`] with
//!   its index in the list and its key; inside it, every text that selects
//!   itself (the message [`paragraph`](crate::paragraph)'s blocks, iced's
//!   `text(..).selectable(true)`) is a part, as is a text drawn by a widget
//!   that selects nothing of itself, wrapped in [`whole`] (a styled name),
//!   and a picture wrapped in [`picture`], copied as `[Picture]`. Parts are
//!   read top to bottom, then left to right, where they show past the
//!   scrollables around them. Content without rows is one.
//! - **Kept by place, not by widget.** The list drops the rows it scrolled
//!   away, so the selection lives here: where the press began and where the
//!   pointer is, each a row's key, a part of it and a point in that part.
//!   Rows built again, as the list scrolls back, show the selection again.
//!   Each row keeps which selection it shows, so a layout touches no text
//!   of the rows that show it already, and what its parts were last given,
//!   so a drag acts only on the rows whose parts it changes (and the row it
//!   copies), a clear only on the rows that showed some: their paragraphs
//!   stay as the last frame drew them, and a frame redraws only what
//!   changed. A text shaped again while a selection is held (its message
//!   edited, the theme or a font changed) so shows no highlight until the
//!   selection changes what its row shows; Ctrl+C still copies it.
//! - **Pressing and dragging.** A left press in a part's text starts it: a
//!   point, or for a double or triple press the word or line there (a
//!   fourth starts again by points). The pointer over a part selects to
//!   there; between parts, to the end of the last one before it, above it or
//!   level with it and to its left; above them all, to the first one's
//!   start. A press on a link starts one too, and the link opens only when
//!   letting go selected nothing. A press the list takes (on its scrollbar,
//!   or ending an autoscroll) starts none. While the pointer is near the list's top
//!   or bottom edge, or past it, the list scrolls and the selection follows,
//!   only while the window is in front.
//! - **Copying.** Ctrl+C writes the selection to the clipboard: the rows at
//!   either end as they were shown, every row between whole from
//!   [`Selection::rows`]' text, so rows never built copy too. Letting go
//!   sends [`Selection::on_select`] with it, for the selection clipboard,
//!   which iced doesn't write. A right press sends [`Selection::on_menu`]
//!   with it when the row under the pointer is in it, for the menu opening
//!   there. A left press anywhere clears it, as do Escape (which still goes
//!   on, and only ends an autoscroll when one goes on) and [`clear`]. A
//!   focused text field with a selection of its own copies that instead:
//!   it takes Ctrl+C before the rows, or writes the clipboard after them.
//! - **Its own.** iced's window-wide selection (a drag that began outside
//!   any widget, Ctrl+C that nothing took) neither selects nor copies in
//!   the rows: their texts stay findable by their words, for tests.
//!
//! Nothing ticks: frames are asked for only while a drag holds the pointer
//! at an edge with the window in front.
//!
//! The timeline, for instance:
//!
//! ```ignore
//! let rows = &vm.rows;
//! let key = |at: usize| rows[at].key;
//! selection(
//!     list(rows.len(), key, |at| selection::row(at, key(at), message::view(&rows[at])), motion)
//!         .id(timeline_id),
//! )
//! .rows(rows.len(), key, |at| Some(message::copied(&rows[at])))
//! .on_select(Intent::Selected)
//! .on_menu(Intent::MenuSelection)
//! .motion(motion)
//! ```

mod act;
mod acts;
mod drag;
mod edge;
mod events;
mod guard;
mod marks;
mod model;
mod pass;
#[cfg(test)]
mod tests;
mod widget;

use iced::Task;
use iced::advanced::widget::{Id, Operation, operate};

use anomi_theme::prelude::*;

pub use marks::{Mark, Row, picture, row, whole};
use model::{Rows, State};

/// The rows of `content` select together: see the module's documentation.
pub struct Selection<'a, Message> {
    content: Element<'a, Message>,
    id: Option<Id>,
    rows: Option<Rows<'a>>,
    on_copy: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_menu: Option<Box<dyn Fn(Option<String>) -> Message + 'a>>,
    motion: Motion,
}

/// Lets a drag select across `content`'s rows, as one piece until
/// [`Selection::rows`] says how its rows copy.
pub fn selection<'a, Message>(content: impl View<Message> + 'a) -> Selection<'a, Message> {
    Selection {
        content: content.boxed(),
        id: None,
        rows: None,
        on_copy: None,
        on_select: None,
        on_menu: None,
        motion: Motion::default(),
    }
}

impl<'a, Message> Selection<'a, Message> {
    /// Names it for [`clear`].
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The rows as the list holds them: how many, each one's key, and how
    /// each copies when all of it is selected, given its index. It should
    /// read as the row's parts do when copied: texts beside each other
    /// joined by a space, the rest by line breaks, and `[Picture]` for each
    /// picture (`message_text`, `src/ui.rs:924`). `text` is called only
    /// while copying, for the rows between the selection's ends: build the
    /// text from the row then, as `message_text` does, rather than keep a
    /// second copy of every message's text in the view-model.
    pub fn rows(
        mut self,
        count: usize,
        key: impl Fn(usize) -> u64 + 'a,
        text: impl Fn(usize) -> Option<String> + 'a,
    ) -> Self {
        self.rows = Some(Rows {
            count,
            key: Box::new(key),
            text: Box::new(text),
        });
        self
    }

    /// What Ctrl+C sends besides writing the clipboard.
    pub fn on_copy(mut self, on_copy: impl Fn(String) -> Message + 'a) -> Self {
        self.on_copy = Some(Box::new(on_copy));
        self
    }

    /// What letting go of a drag that selected something sends, with what
    /// it selected: for the selection clipboard (X11's and Wayland's
    /// primary), which iced doesn't write.
    pub fn on_select(mut self, on_select: impl Fn(String) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(on_select));
        self
    }

    /// What a right press sends, before the menu there opens: the selection
    /// as copied when it covers part of the row under the pointer, else
    /// `None` (`selection-at`, `ui/yapfast.slint:1690`).
    pub fn on_menu(mut self, on_menu: impl Fn(Option<String>) -> Message + 'a) -> Self {
        self.on_menu = Some(Box::new(on_menu));
        self
    }

    /// Whether the window is in front, for scrolling at the edges.
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }
}

/// Clears the selection of the [`Selection`] with the id, as a menu's Copy
/// does once it has copied it (the press on its row cleared it in Slint).
pub fn clear<T: Send + 'static>(id: impl Into<Id>) -> Task<T> {
    operate(forget(id.into()))
}

/// The operation under [`clear`], for tests.
pub fn forget<T>(id: Id) -> impl Operation<T> {
    Forget(id)
}

struct Forget(Id);

impl<T> Operation<T> for Forget {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn custom(&mut self, id: Option<&Id>, _bounds: iced::Rectangle, state: &mut dyn std::any::Any) {
        if id == Some(&self.0)
            && let Some(state) = state.downcast_mut::<State>()
        {
            state.clear = true;
        }
    }
}
