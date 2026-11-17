//! What the stack keeps of each layer between views: how far it's open,
//! where its card went, whether it said it had gone, whose focus it put
//! away; and matching each layer of a new view to its state by key.

use std::time::Instant;

use iced::advanced::widget::Tree;
use iced::{Point, Rectangle};

use anomi_theme::Motion;
use anomi_theme::motion::Popup;

use super::Layer;
use super::focus::Kept;

/// One layer's state.
pub(super) struct Slot {
    pub key: u64,
    pub popup: Popup,
    /// Its card in the window, and the point it grows from.
    pub card: Rectangle,
    pub origin: Point,
    /// It has sent `on_gone` for its last way out.
    pub gone: bool,
    /// Its focus, put away while a layer above it is open or it closes.
    pub focus: Kept,
}

#[derive(Default)]
pub(super) struct State {
    /// By layer, in the order of the tree's children after the base.
    pub slots: Vec<Slot>,
    /// The frame the layers are drawn at.
    pub now: Option<Instant>,
    /// The base's focus, put away while a layer with a backdrop is open.
    pub base_focus: Kept,
}

impl State {
    pub fn now(&self) -> Instant {
        self.now.unwrap_or_else(Instant::now)
    }

    /// The topmost layer that is open and keeps the pointer and the keys
    /// from what's under it.
    pub fn top<M>(&self, layers: &[Layer<'_, M>]) -> Option<usize> {
        (0..layers.len())
            .rev()
            .find(|&index| layers[index].backdrop.blocks() && self.slots[index].popup.is_open())
    }

    /// Whether layer `index` is open and not under `top`: what focus, the
    /// keys and its own overlays may reach.
    pub fn inside(&self, index: usize, top: Option<usize>) -> bool {
        self.slots[index].popup.is_open() && top.is_none_or(|top| index >= top)
    }
}

/// Matches the new view's layers to the old ones by key, keeping each
/// one's widget tree and state, and heads each for open or closed.
pub(super) fn diff<M>(tree: &mut Tree, layers: &mut [Layer<'_, M>], motion: Motion) {
    let mut old_trees: Vec<Option<Tree>> = tree.children.drain(1..).map(Some).collect();
    let state = tree.state.downcast_mut::<State>();
    let mut old_slots: Vec<Option<Slot>> = state.slots.drain(..).map(Some).collect();
    let now = Instant::now();
    for layer in layers.iter_mut() {
        let found = old_slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|slot| slot.key == layer.key));
        let (mut slot, mut child) = match found {
            Some(index) => (
                old_slots[index].take().expect("found"),
                old_trees[index]
                    .take()
                    .unwrap_or_else(|| Tree::new(&layer.content)),
            ),
            None => (
                Slot {
                    key: layer.key,
                    popup: Popup::closed(layer.pace),
                    card: Rectangle::default(),
                    origin: Point::ORIGIN,
                    gone: false,
                    focus: Kept::default(),
                },
                Tree::new(&layer.content),
            ),
        };
        if layer.open {
            slot.gone = false;
        }
        slot.popup.go(layer.open, motion, now);
        child.diff(&mut layer.content);
        state.slots.push(slot);
        tree.children.push(child);
    }
}
