use super::*;

impl Session {
    /// Draws a tree in a slot, or says why the interface has no place for it.
    ///
    /// Nothing here reaches the interface at once: a drawing joins the frame
    /// that is being gathered, so a plugin that draws on every event still costs
    /// one update per frame.
    pub(super) fn paint_slot(&mut self, plugin: &PluginId, request: SetSlot) -> HostReturn {
        let Some(slot) = Slot::from_id(&request.slot) else {
            let named: Vec<&str> = Slot::ALL
                .into_iter()
                .filter(|slot| !slot.buttons_only() && *slot != Slot::Message)
                .map(Slot::id)
                .collect();
            return HostReturn::Denied {
                reason: format!(
                    "Yapfast has no slot called `{}`; the slots are {}",
                    request.slot,
                    named.join(", ")
                ),
            };
        };
        if slot == Slot::Message {
            return HostReturn::Denied {
                reason: "what a plugin draws under a message isn't a slot: it answers \
                         `message.decorate` with a tree for each message it decorates"
                    .to_owned(),
            };
        }
        let tree = match crate::plugins::ui::Tree::parse(&request.tree.to_string()) {
            Ok(tree) => tree,
            Err(reason) => {
                return HostReturn::Denied {
                    reason: format!("the tree can't be drawn: {reason}"),
                };
            }
        };
        self.painting.queue(
            Instant::now(),
            slots::Output::SetSlot {
                plugin: plugin.to_string(),
                slot,
                tree,
            },
        );
        answered(SlotDrawn::yes())
    }

    /// Takes a plugin's tree out of a slot.
    pub(super) fn clear_paint(&mut self, plugin: &PluginId, slot: &str) -> HostReturn {
        let Some(slot) = Slot::from_id(slot) else {
            return HostReturn::Denied {
                reason: format!("Yapfast has no slot called `{slot}`"),
            };
        };
        if slot == Slot::Message {
            return HostReturn::Denied {
                reason: "a decoration goes away by answering `message.decorate` with no tree \
                         for that message"
                    .to_owned(),
            };
        }
        self.painting.queue(
            Instant::now(),
            slots::Output::ClearSlot {
                plugin: plugin.to_string(),
                slot,
            },
        );
        answered(SlotDrawn::yes())
    }

    /// A line for the person under the plugin's name, in the banner slot or the
    /// toast's.
    pub(super) fn paint_line(&mut self, plugin: &PluginId, slot: Slot, text: &str) -> HostReturn {
        if text.trim().is_empty() {
            return HostReturn::Error {
                message: "the line to show is empty".to_owned(),
            };
        }
        let tree = serde_json::json!({
            "rows": [{ "cells": [{ "kind": "text", "text": text }] }]
        });
        let tree = match crate::plugins::ui::Tree::parse(&tree.to_string()) {
            Ok(tree) => tree,
            Err(reason) => {
                return HostReturn::Denied {
                    reason: format!("the line can't be drawn: {reason}"),
                };
            }
        };
        self.painting.queue(
            Instant::now(),
            slots::Output::SetSlot {
                plugin: plugin.to_string(),
                slot,
                tree,
            },
        );
        answered(SlotDrawn::yes())
    }
}
