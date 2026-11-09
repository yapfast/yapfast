//! What plugins draw in the interface's slots, as the application keeps
//! it: one widget tree per plugin and slot, which the plugin runtime sets
//! and clears through [`Output`], and the UI shows only when a slot's count
//! of changes moved; and what they draw under messages, kept for the
//! [`DECORATIONS`] messages decorated last.

use std::{collections::VecDeque, sync::Arc};

use super::ui::{PluginName, Tree};

/// A place in the interface where plugins draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// A notice above the conversation.
    Banner,
    /// A toast in the window's corner, gone after a few seconds.
    Toast,
    /// Buttons beside the composer's.
    Composer,
    /// Buttons on a message's hover bar, acting on that message.
    HoverBar,
    /// Under a message, one tree for each message a plugin decorates.
    Message,
}

impl Slot {
    pub const ALL: [Self; 5] = [
        Self::Banner,
        Self::Toast,
        Self::Composer,
        Self::HoverBar,
        Self::Message,
    ];

    /// Its name, as a plugin and its input name it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Banner => "banner",
            Self::Toast => "toast",
            Self::Composer => "composer",
            Self::HoverBar => "message.actions",
            Self::Message => "message",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.id() == id)
    }

    /// It draws only a plugin's buttons, in a row of Yapfast's own.
    pub const fn buttons_only(self) -> bool {
        matches!(self, Self::Composer | Self::HoverBar)
    }
}

/// What the plugin runtime asks of the interface, which reaches the
/// application as `Event::Plugin`. A plugin is named by its id.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    SetSlot {
        plugin: String,
        slot: Slot,
        tree: Tree,
    },
    ClearSlot {
        plugin: String,
        slot: Slot,
    },
    /// What a plugin draws under a message, by the message's id; `None`
    /// takes it away. The runtime decorates again a message that's edited.
    Decorate {
        plugin: String,
        message: String,
        tree: Option<Tree>,
    },
}

/// How many messages' decorations are kept, the least recently decorated
/// going first; a message decorated again comes back when it's on screen.
pub const DECORATIONS: usize = 512;

/// A plugin's tree under one message.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoration {
    pub message: String,
    pub plugin: usize,
    pub tree: Arc<Tree>,
}

/// One plugin's tree in one slot; `plugin` is its index in the names.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub plugin: usize,
    pub slot: Slot,
    pub tree: Arc<Tree>,
}

/// Every plugin's trees, and a count of changes per slot.
#[derive(Clone, Debug, Default)]
pub struct Slots {
    names: Vec<PluginName>,
    drawn: Vec<Drawn>,
    changes: [u64; Slot::ALL.len()],
    /// The oldest first.
    decorations: VecDeque<Decoration>,
    /// The messages whose decorations changed since the UI last took them.
    decorated: Vec<String>,
}

impl Slots {
    pub fn new(names: Vec<PluginName>) -> Self {
        Self {
            names,
            ..Self::default()
        }
    }

    pub fn names(&self) -> &[PluginName] {
        &self.names
    }

    fn index(&self, plugin: &str) -> Option<usize> {
        self.names.iter().position(|name| name.id == plugin)
    }

    /// Shows a plugin's tree in a slot, in place of its last one there.
    /// False when nothing changed: the same tree, or a plugin not enabled.
    /// A slot of buttons keeps only its buttons.
    pub fn set(&mut self, plugin: &str, slot: Slot, mut tree: Tree) -> bool {
        let Some(plugin) = self.index(plugin) else {
            return false;
        };
        if slot.buttons_only() {
            for row in &mut tree.rows {
                row.cells
                    .retain(|cell| cell.kind == super::ui::Kind::Button);
            }
            tree.rows.retain(|row| !row.cells.is_empty());
        }
        let place = self
            .drawn
            .iter()
            .position(|drawn| drawn.plugin == plugin && drawn.slot == slot);
        match place {
            Some(place) if *self.drawn[place].tree == tree => return false,
            Some(place) => self.drawn[place].tree = Arc::new(tree),
            None => self.drawn.push(Drawn {
                plugin,
                slot,
                tree: Arc::new(tree),
            }),
        }
        self.changes[slot as usize] += 1;
        true
    }

    /// Takes a plugin's tree out of a slot; false when it had none there.
    pub fn clear(&mut self, plugin: &str, slot: Slot) -> bool {
        let Some(plugin) = self.index(plugin) else {
            return false;
        };
        let before = self.drawn.len();
        self.drawn
            .retain(|drawn| !(drawn.plugin == plugin && drawn.slot == slot));
        let cleared = self.drawn.len() != before;
        if cleared {
            self.changes[slot as usize] += 1;
        }
        cleared
    }

    /// What plugins draw in a slot, in the order they first drew there.
    pub fn drawn(&self, slot: Slot) -> impl Iterator<Item = &Drawn> {
        self.drawn.iter().filter(move |drawn| drawn.slot == slot)
    }

    /// How many times a slot changed, for the UI to tell whether to show it
    /// again.
    pub fn changes(&self, slot: Slot) -> u64 {
        self.changes[slot as usize]
    }

    /// Draws a plugin's tree under a message, or takes it away; false when
    /// nothing changed.
    pub fn decorate(&mut self, plugin: &str, message: &str, tree: Option<Tree>) -> bool {
        let Some(plugin) = self.index(plugin) else {
            return false;
        };
        let place = self
            .decorations
            .iter()
            .position(|kept| kept.plugin == plugin && kept.message == message);
        let old = place.and_then(|place| self.decorations.remove(place));
        match tree {
            None if old.is_none() => return false,
            None => {}
            Some(tree) => {
                let unchanged = old.as_ref().is_some_and(|old| *old.tree == tree);
                self.decorations.push_back(Decoration {
                    message: message.to_owned(),
                    plugin,
                    tree: old
                        .filter(|_| unchanged)
                        .map_or_else(|| Arc::new(tree), |old| old.tree),
                });
                if self.decorations.len() > DECORATIONS {
                    let gone = self.decorations.pop_front();
                    self.decorated.extend(gone.map(|gone| gone.message));
                }
                if unchanged {
                    return false;
                }
            }
        }
        self.decorated.push(message.to_owned());
        self.changes[Slot::Message as usize] += 1;
        true
    }

    /// What plugins draw under a message, in the order they drew it.
    pub fn decorations<'a>(&'a self, message: &'a str) -> impl Iterator<Item = &'a Decoration> {
        self.decorations
            .iter()
            .filter(move |kept| kept.message == message)
    }

    /// The messages whose decorations changed since this was last called.
    pub fn take_decorated(&mut self) -> Vec<String> {
        std::mem::take(&mut self.decorated)
    }

    pub fn apply(&mut self, output: Output) -> bool {
        match output {
            Output::SetSlot { plugin, slot, tree } => self.set(&plugin, slot, tree),
            Output::ClearSlot { plugin, slot } => self.clear(&plugin, slot),
            Output::Decorate {
                plugin,
                message,
                tree,
            } => self.decorate(&plugin, &message, tree),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::ui::Kind, *};

    fn tree(json: &str) -> Tree {
        Tree::parse(json).unwrap()
    }

    fn slots() -> Slots {
        Slots::new(vec![
            PluginName {
                id: "a.one".into(),
                name: "One".into(),
            },
            PluginName {
                id: "a.two".into(),
                name: "Two".into(),
            },
        ])
    }

    #[test]
    fn a_slot_counts_only_real_changes() {
        let mut slots = slots();
        let hello = tree(r#"{ "rows": [{ "cells": [{ "text": "Hello" }] }] }"#);
        assert!(slots.set("a.one", Slot::Banner, hello.clone()));
        assert!(
            !slots.set("a.one", Slot::Banner, hello.clone()),
            "the same tree"
        );
        assert!(
            !slots.set("a.gone", Slot::Banner, hello.clone()),
            "not enabled"
        );
        assert!(slots.set("a.two", Slot::Banner, hello));
        assert_eq!(slots.changes(Slot::Banner), 2);
        assert_eq!(slots.changes(Slot::Toast), 0);
        let order: Vec<usize> = slots
            .drawn(Slot::Banner)
            .map(|drawn| drawn.plugin)
            .collect();
        assert_eq!(order, [0, 1]);
        assert!(slots.clear("a.one", Slot::Banner));
        assert!(!slots.clear("a.one", Slot::Banner));
        assert_eq!(slots.changes(Slot::Banner), 3);
        assert_eq!(slots.drawn(Slot::Banner).count(), 1);
    }

    #[test]
    fn decorations_follow_their_message_and_the_oldest_go() {
        let mut slots = slots();
        let badge = tree(r#"{ "rows": [{ "cells": [{ "kind": "badge", "text": "Link" }] }] }"#);
        assert!(slots.decorate("a.one", "m1", Some(badge.clone())));
        assert!(
            !slots.decorate("a.one", "m1", Some(badge.clone())),
            "the same"
        );
        assert!(slots.decorate("a.two", "m1", Some(badge.clone())));
        assert_eq!(slots.decorations("m1").count(), 2);
        assert_eq!(slots.take_decorated(), ["m1", "m1"]);
        assert!(slots.take_decorated().is_empty());
        assert!(slots.decorate("a.one", "m1", None));
        assert!(!slots.decorate("a.one", "m1", None));
        assert_eq!(slots.decorations("m1").count(), 1);
        for index in 0..DECORATIONS {
            slots.decorate("a.one", &format!("x{index}"), Some(badge.clone()));
        }
        assert_eq!(slots.decorations("m1").count(), 0, "the least recent went");
        assert!(slots.take_decorated().contains(&"m1".to_owned()));
    }

    #[test]
    fn a_slot_of_buttons_keeps_only_buttons() {
        let mut slots = slots();
        slots.set(
            "a.one",
            Slot::HoverBar,
            tree(
                r#"{ "rows": [{ "cells": [{ "text": "Words" }, { "kind": "button", "id": "pin", "text": "Keep" }] },
                              { "cells": [{ "kind": "toggle", "id": "t" }] }] }"#,
            ),
        );
        let drawn = slots.drawn(Slot::HoverBar).next().unwrap();
        assert_eq!(drawn.tree.rows.len(), 1);
        assert_eq!(drawn.tree.rows[0].cells.len(), 1);
        assert_eq!(drawn.tree.rows[0].cells[0].kind, Kind::Button);
    }
}
