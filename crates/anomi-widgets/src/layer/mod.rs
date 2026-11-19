//! What opens over the window, in layers: dialogs, popovers, menus, cards,
//! the settings, the viewer. The Slint window shows each with an `if` over
//! the rest, in the order of its file (`YapfastWindow` in
//! `ui/yapfast.slint`, from `:4690`): a backdrop that dims or only catches
//! clicks, the card arriving and leaving by `PopupMotion`
//! (`ui/theme.slint:216`), a `FocusScope` that takes the keys and closes it
//! on Escape. [`Layers`] is that stack over the window's base:
//!
//! - **Order.** Layers draw in the order pushed, the last on top, each over
//!   a backdrop of its own ([`Backdrop`]).
//! - **Way in and out.** A layer grows from 96% as it opens, at its
//!   [`Pace`], and back as it closes, over `LEAVE`; while closing it still
//!   draws but takes nothing, and when it has gone it sends `on_gone`, so
//!   the app may drop what it showed. A still [`Motion`] lands at once.
//!   The card doesn't fade (see [`anomi_theme::motion::Popup`]); a dim
//!   backdrop does.
//! - **Clicks.** An open layer with a backdrop keeps the pointer from what
//!   lies under it, which sees no pointer at all (hovers clear) and no
//!   press; a press outside its card sends `on_close`.
//! - **Overlays.** What floats over the base, such as a message's hover
//!   bar, stays drawn under a layer that keeps the pointer from it, under
//!   the layer's card and taking nothing, as Slint keeps a message's bar
//!   under its menu; under a dim backdrop it goes (`passive`).
//! - **Keys.** The topmost open layer with a backdrop takes the keyboard,
//!   the input method and pastes; Escape it doesn't use sends its
//!   `on_close`, so Escape closes the top layer only. Keys it doesn't use
//!   and that aren't Escape go on to the app's own handlers (the window's
//!   shortcuts), as Slint's rejected keys bubble to the window's
//!   `FocusScope`, but never to the layers below.
//! - **Focus.** While such a layer is open, focus operations (`focus`,
//!   `focus_next`, Tab) see only it and the layers above it, so focus stays
//!   inside: asking to focus a widget under it finds nothing. What had the
//!   focus under it loses it, so no caret or ring shows there and it asks
//!   for no input method, and has it again once the layer closes. A layer
//!   on its way out puts its own focus away too.
//!
//! A layer is keyed: the same key keeps its state (how far it's open, its
//! widgets' state) from one view to the next, wherever it sits in the
//! stack. A new key arrives anew.

mod events;
mod focus;
mod passive;
mod state;
#[cfg(test)]
mod tests;
mod widget;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::{Point, Rectangle};

use anomi_theme::motion::Pace;
use anomi_theme::{Element, Motion, View};
use anomi_vm::menu::Spot;

use crate::menu::place::MARGIN;

/// The window's base and the layers over it.
pub struct Layers<'a, M> {
    base: Element<'a, M>,
    layers: Vec<Layer<'a, M>>,
    motion: Motion,
}

/// A stack of layers over `base`, the window's areas.
pub fn layers<'a, M>(base: impl View<M> + 'a, motion: Motion) -> Layers<'a, M> {
    Layers {
        base: base.boxed(),
        layers: Vec::new(),
        motion,
    }
}

impl<'a, M> Layers<'a, M> {
    /// Puts a layer on top.
    pub fn push(mut self, layer: Layer<'a, M>) -> Self {
        self.layers.push(layer);
        self
    }

    /// Puts a layer on top, if there's one to show.
    pub fn push_maybe(self, layer: Option<Layer<'a, M>>) -> Self {
        match layer {
            Some(layer) => self.push(layer),
            None => self,
        }
    }
}

/// One layer: what it shows and how it comes and goes.
pub struct Layer<'a, M> {
    key: u64,
    content: Element<'a, M>,
    open: bool,
    pace: Pace,
    place: Place,
    bottom_clearance: f32,
    backdrop: Backdrop,
    grow_from: Option<Point>,
    through: Option<Rectangle>,
    on_close: Option<M>,
    on_gone: Option<M>,
}

/// A layer showing `content`, open, centred, at a popover's pace, over a
/// clear backdrop. `key` names it across views.
pub fn layer<'a, M>(key: impl Hash, content: impl View<M> + 'a) -> Layer<'a, M> {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    Layer {
        key: hasher.finish(),
        content: content.boxed(),
        open: true,
        pace: Pace::Popover,
        place: Place::Centre,
        bottom_clearance: MARGIN,
        backdrop: Backdrop::Clear,
        grow_from: None,
        through: None,
        on_close: None,
        on_gone: None,
    }
}

impl<'a, M> Layer<'a, M> {
    /// Whether it shows; `false` while it plays its way out.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// How fast it arrives.
    pub fn pace(mut self, pace: Pace) -> Self {
        self.pace = pace;
        self
    }

    /// A dialog's pace and its dimmed backdrop (`rgba(0, 0, 0, 0.6)`, the
    /// dialogs' in `ui/dialogs.slint:45`).
    pub fn dialog(self) -> Self {
        self.pace(Pace::Dialog).backdrop(Backdrop::Dim(DIALOG_DIM))
    }

    pub fn place(mut self, place: Place) -> Self {
        self.place = place;
        self
    }

    /// How far a card placed [`Place::At`] a spot stays from the window's
    /// bottom: [`MARGIN`], as every other edge, unless said. A header's
    /// popover asks for
    /// [`HEADER_CLEARANCE`](crate::menu::place::HEADER_CLEARANCE).
    pub fn bottom_clearance(mut self, clearance: f32) -> Self {
        self.bottom_clearance = clearance;
        self
    }

    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// The point it grows from, in the window, when its place doesn't say:
    /// a menu filling the window grows from where it opened.
    pub fn grow_from(mut self, point: Point) -> Self {
        self.grow_from = Some(point);
        self
    }

    /// A part of the window where it leaves the pointer to what's under it,
    /// backdrop or not: the card another layer draws that belongs with this
    /// one, such as a participant's volumes over the rows of their menu.
    pub fn through(mut self, area: Rectangle) -> Self {
        self.through = Some(area);
        self
    }

    /// What Escape or a press outside its card sends.
    pub fn on_close(mut self, message: M) -> Self {
        self.on_close = Some(message);
        self
    }

    /// What it sends once its way out has ended.
    pub fn on_gone(mut self, message: M) -> Self {
        self.on_gone = Some(message);
        self
    }
}

/// A dialog's backdrop: black at 60%.
pub const DIALOG_DIM: f32 = 0.6;

/// Where a layer's card goes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Place {
    /// In the middle of the window, as a dialog; laid out within the
    /// window less its margins.
    #[default]
    Centre,
    /// Over the whole window, as the settings or the viewer; the content
    /// places itself (a context menu does), and a press it doesn't take is
    /// outside it.
    Fill,
    /// At a spot, flipping to the other side without room and kept
    /// [`MARGIN`] inside the window, as a popover or a card
    /// (`anomi_widgets::menu::place`); [`Layer::bottom_clearance`] from its
    /// bottom. Above or below its spot, a card taller than the room on
    /// either side is laid out again to that room.
    At(Spot),
}

/// What lies between a layer and what's under it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Backdrop {
    /// Nothing: what's under it still takes the pointer around its card,
    /// and the keys. For what only shows, such as a notice.
    None,
    /// An invisible one that catches the pointer, as a popover's
    /// full-window `TouchArea`: a press on it closes the layer.
    #[default]
    Clear,
    /// Black at this alpha, fading with the layer: 0.6 for a dialog, 0.7
    /// for the settings, 0.9 for the viewer.
    Dim(f32),
}

impl Backdrop {
    /// It keeps the pointer and the keys from what's under it.
    fn blocks(self) -> bool {
        !matches!(self, Self::None)
    }
}
