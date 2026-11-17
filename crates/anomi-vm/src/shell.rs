//! The window around the areas: how wide its panels are, which layers are
//! open over it, and the notes it shows over everything. The Slint side:
//! `YapfastWindow` (`ui/yapfast.slint:95`), its panel widths and their
//! callbacks (`src/ui/callbacks.rs:1964`-`:1983`), its layers (`:4560` to
//! the end) and `src/zoom.rs`.
//!
//! - **Panels.** The sidebar, the thread beside a channel, the member list
//!   and a call above a DM's messages keep the size their edge was dragged
//!   to; `anomi_screens::shell::Docking` keeps each within its bounds and
//!   decides what fits beside the conversation.
//! - **Layers.** [`Shell::layers`] lists what is open over the window, each
//!   a [`Layer`]: a dialog, a popover, a card, a menu, the settings. The
//!   shell draws them in Slint's order whatever order they opened in, so
//!   an area's shot opens its layer from its fixture with [`Shell::open`].
//!   A layer closed plays its way out and asks [`Intent::Gone`]; the app
//!   then takes it away with [`Shell::gone`].
//! - **Notes.** The zoom level for a moment after it changes, and files
//!   dragged over the window.

mod layers;
pub mod zoom;

#[cfg(test)]
mod tests;

pub use layers::{Layer, Opened};

use iced::Rectangle;

use crate::menu::Spot;

#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    /// The sidebar's width, as its edge was last dragged; the shell keeps
    /// what shows within its bounds (`sidebar-shown`, `:1470`).
    pub sidebar_width: f32,
    /// The member list's, or `None` for the style's (`members-width`,
    /// `:1711`; zero there).
    pub members_width: Option<f32>,
    /// The thread beside a channel's (`aside-width`, `:1705`), or `None`
    /// for its own.
    pub aside_width: Option<f32>,
    /// A call above a DM's messages, as its lower edge was dragged, or
    /// `None` to share the room with the messages (`call-height`, `:1717`).
    pub call_height: Option<f32>,
    /// A panel put away for want of room shows anyway, until the window
    /// changes size across its rule or another conversation opens
    /// (`squeeze-override`, `:1954`).
    pub squeezed: bool,
    /// A notice over the conversation, such as the connection stopping
    /// (`notice`), with Dismiss.
    pub notice: Option<String>,
    /// The account is connecting: the lists are still empty. The
    /// conversation shows what it shows with nothing open, as Slint's does
    /// (`NoConversation`, `:4094`).
    pub connecting: bool,
    /// No account is signed in: the sign-in screen shows in place of the
    /// rail, the lists and the conversation (`!workspace`).
    pub signed_out: bool,
    /// A call is going: its dock sits on the account card (`voice-visible`).
    pub voice_dock: bool,
    /// What is open over the window, in the order the shell draws it.
    pub layers: Vec<Opened>,
    /// The zoom level, as "110%", for a moment after it changes
    /// (`zoom-note`, `:5425`).
    pub zoom: Option<String>,
    /// Files dragged over the window, or a drop refused (`:5484`).
    pub drop: Option<Files>,
    /// The corner the stream in a corner sits in (`pip-left`, `pip-bottom`,
    /// `:4564`).
    pub pip: Corner,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            // `ClientStyle.sidebar-width` (`ui/yapfast/style.slint`).
            sidebar_width: 304.0,
            members_width: None,
            aside_width: None,
            call_height: None,
            squeezed: false,
            notice: None,
            connecting: true,
            signed_out: false,
            voice_dock: false,
            layers: Vec::new(),
            zoom: None,
            drop: None,
            pip: Corner::default(),
        }
    }
}

impl Shell {
    /// Opens a layer, or keeps it open, where it opens by itself.
    pub fn open(&mut self, layer: Layer) {
        self.put(layer, None);
    }

    /// Opens a layer from a spot: a popover under its button, a card beside
    /// its row, a menu at the pointer.
    pub fn open_at(&mut self, layer: Layer, at: Spot) {
        self.put(layer, Some(at));
    }

    /// Closes a layer: it plays its way out, then asks [`Intent::Gone`].
    pub fn close(&mut self, layer: Layer) {
        if let Some(opened) = self.layers.iter_mut().find(|o| o.layer == layer) {
            opened.open = false;
        }
    }

    /// A layer's way out ended: it goes, unless it opened again meanwhile.
    pub fn gone(&mut self, layer: Layer) {
        self.layers.retain(|o| o.layer != layer || o.open);
    }

    /// Takes a layer away at once, without its way out, as what Slint
    /// shows with a bare `if` goes: the stream in a corner, a ringing call,
    /// the call full screen (`:4597`).
    pub fn remove(&mut self, layer: Layer) {
        self.layers.retain(|o| o.layer != layer);
    }

    /// Whether a layer is open (not on its way out).
    pub fn is_open(&self, layer: Layer) -> bool {
        self.opened(layer).is_some_and(|o| o.open)
    }

    /// A layer that shows, open or on its way out.
    pub fn opened(&self, layer: Layer) -> Option<&Opened> {
        self.layers.iter().find(|o| o.layer == layer)
    }

    /// Keeps [`Self::layers`] in drawing order: a layer that opens again
    /// keeps its place and takes its new spot.
    fn put(&mut self, layer: Layer, at: Option<Spot>) {
        let opened = Opened {
            layer,
            open: true,
            at,
        };
        match self.layers.binary_search_by(|o| o.layer.cmp(&layer)) {
            Ok(index) => self.layers[index] = opened,
            Err(index) => self.layers.insert(index, opened),
        }
    }
}

/// A panel whose edge drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    /// The channel and DM list, from its right edge (`:4497`).
    Sidebar,
    /// The thread beside a channel, from its left edge (`:4517`).
    Aside,
    /// The member list, from its left edge (`:4538`).
    Members,
    /// A call above a DM's messages, from its lower edge (`:3489`).
    Call,
}

/// The corner of the conversation a floating stream keeps to: the top
/// right until it's dragged elsewhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Corner {
    pub left: bool,
    pub bottom: bool,
}

/// Files over the window, as Discord shows them: a card over the dimmed
/// window saying where they'll go, or why a drop was refused. Each says
/// whether that's the thread beside the conversation (`drop-aside`), the
/// card showing over it: a drop refused there keeps it, as Slint's keeps
/// the `drop-aside` the drag set (`src/ui.rs:2886`).
#[derive(Clone, Debug, PartialEq)]
pub enum Files {
    /// Files are dragged over: where they'll go (`drop-destination`).
    Over { destination: String, aside: bool },
    /// A drop was refused: why, and what to do (`drop-refusal`,
    /// `drop-refusal-help`), until Got It or a click.
    Refused {
        reason: String,
        help: String,
        aside: bool,
    },
}

impl Files {
    /// Whether they were bound for the thread beside the conversation.
    pub fn aside(&self) -> bool {
        match self {
            Self::Over { aside, .. } | Self::Refused { aside, .. } => *aside,
        }
    }
}

/// What the window's frame asks for: an edge dragged, a layer closed. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A panel's edge is dragged: its size as it follows the pointer, kept
    /// within its bounds.
    Resize(Panel, f32),
    /// The drag let go at this size, which `layout.json` keeps
    /// (`sidebar-resized`, `members-resized`, `aside-resized`,
    /// `call-resized`).
    Resized(Panel, f32),
    /// A double click on its edge: back to its own size, kept as unchosen.
    ResetSize(Panel),
    /// Escape, or a press outside it: the layer closes.
    Close(Layer),
    /// A closed layer's way out ended: it may go ([`Shell::gone`]).
    Gone(Layer),
    /// The notice's Dismiss (`dismiss-notice`).
    DismissNotice,
    /// Got It on a refused drop, or a click on its card.
    DismissDrop,
    /// A forum post's React to post: the picker opens by the button, whose
    /// bounds in the window this carries (`:3570`).
    ReactToPost(Rectangle),
    /// Follow the forum post open, or stop (`join-thread`, `leave-thread`).
    FollowPost(bool),
    /// Copy the forum post's link (`copy-link`).
    CopyPostLink,
}
