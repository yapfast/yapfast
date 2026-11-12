use super::*;

impl Session {
    /// Hands a plugin what the person did with something it drew, with
    /// `yf_input`: a menu row it added, a widget it drew, a notice of its that
    /// was closed.
    ///
    /// Nothing answers this, so it goes the way a timer's tick goes: with no id
    /// and nothing to expire. The interface changed when the person clicked, so
    /// waiting on a plugin here would only delay what already happened; a
    /// plugin that wants to be seen says so with a host call or by drawing
    /// again, which is the same thing it does everywhere else.
    pub(super) fn input(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        input: yapfast_plugin_api::message::InputEvent,
    ) {
        self.activate(shared, plugin);
        let message = ToHelper::Input {
            plugin: plugin.clone(),
            input,
        };
        self.send_message(&message, FrameKind::Request);
    }

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
