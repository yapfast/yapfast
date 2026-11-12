//! What plugins draw in the interface's slots, as the application keeps
//! it: one widget tree per plugin and slot, which the plugin runtime sets
//! and clears through [`Output`], and the UI shows only when a slot's count
//! of changes moved; and what they draw under messages, kept for the
//! [`DECORATIONS`] messages decorated last.

use std::{collections::VecDeque, sync::Arc};

use super::ui::{PluginName, Tone, Tree};

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
    /// A question a plugin asks, over the window until it's answered or
    /// dismissed. One per plugin: opening another replaces it.
    Dialog,
    /// What a plugin adds to the open person's profile card.
    ProfileBadges,
}

impl Slot {
    pub const ALL: [Self; 7] = [
        Self::Banner,
        Self::Toast,
        Self::Composer,
        Self::HoverBar,
        Self::Message,
        Self::Dialog,
        Self::ProfileBadges,
    ];

    /// Its name, as a plugin and its input name it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Banner => "banner",
            Self::Toast => "toast",
            Self::Composer => "composer",
            Self::HoverBar => "message.actions",
            Self::Message => "message",
            Self::Dialog => "dialog",
            Self::ProfileBadges => "profile.badges",
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
/// application as `Event::PluginSlot`. A plugin is named by its id.
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
    /// `hide` is what the plugin asked be kept out of sight.
    Decorate {
        plugin: String,
        message: String,
        tree: Option<Tree>,
        hide: Hidden,
    },
    /// What a plugin puts beside a person's name in the member list; `None`
    /// takes it away. It is answered for the rows on screen as the list scrolls,
    /// so the parent keeps it until the list's snapshot changes.
    MemberBadge {
        plugin: String,
        member: String,
        badge: Option<Badge>,
    },
}

/// How many messages' decorations are kept, the least recently decorated
/// going first; a message decorated again comes back when it's on screen.
pub const DECORATIONS: usize = 512;

/// How many people's badges are kept, the least recently asked about going
/// first: a scroll through a large server asks for each row once.
pub const BADGES: usize = 640;

/// How many plugins may badge one person. A name has room for a few, and the
/// rest of the list stays readable.
pub const BADGES_PER_MEMBER: usize = 3;

/// How many characters a badge may show: enough for a mark, not for a sentence.
const BADGE_CHARS: usize = 16;

/// A badge a plugin puts beside a person's name: a few characters and a tone,
/// not a widget tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub text: String,
    pub tone: Tone,
}

impl Badge {
    /// A plugin's answer, checked: a few characters on one line and a tone the
    /// theme knows, refused with the reason as a tree is.
    pub fn parse(value: &serde_json::Value) -> Result<Self, String> {
        let badge: yapfast_plugin_api::message::MemberBadge = serde_json::from_value(value.clone())
            .map_err(|error| error.to_string())?;
        let text = badge.text.trim();
        if text.is_empty() {
            return Err("its text is empty".to_owned());
        }
        if text.chars().count() > BADGE_CHARS {
            return Err(format!("its text is longer than {BADGE_CHARS} characters"));
        }
        if text.chars().any(char::is_control) {
            return Err("its text has a line break or control character".to_owned());
        }
        let tone = match badge.tone.as_str() {
            "" => Tone::Normal,
            name => serde_json::from_value(serde_json::Value::String(name.to_owned()))
                .map_err(|_| format!("its tone `{name}` isn't one the theme knows"))?,
        };
        Ok(Badge {
            text: text.to_owned(),
            tone,
        })
    }
}

/// One plugin's badge for one person, with the plugin's index in the names;
/// `badge` is `None` for a person it answered for and badged nothing, which is
/// kept so scrolling back doesn't ask again.
#[derive(Clone, Debug, PartialEq)]
struct Badged {
    member: String,
    plugin: usize,
    badge: Option<Badge>,
}

/// A plugin's tree under one message.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoration {
    pub message: String,
    pub plugin: usize,
    /// What the plugin draws under the row, when it drew something: a plugin
    /// that only filters a row answers a hide and no tree.
    pub tree: Option<Arc<Tree>>,
    /// The row is hidden from view, which `messages.filter` allows and which
    /// deletes nothing: turning the plugin off brings it back.
    pub hide: bool,
    /// The row's pictures and files are hidden, and its text is not.
    pub hide_media: bool,
}

/// What a plugin asked be kept out of sight for one message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hidden {
    pub row: bool,
    pub media: bool,
}

impl Hidden {
    /// Nothing hidden, or nothing left to hide.
    pub fn none(self) -> bool {
        !self.row && !self.media
    }
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
    /// What plugins badge people in the member list with, the least recently
    /// asked about first.
    badges: VecDeque<Badged>,
    /// The people whose badges changed since the UI last took them.
    badged: Vec<String>,
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

    /// Takes away everything one plugin drew: its trees in every slot, and
    /// what it drew under messages, which is what disabling, suspending or
    /// uninstalling it does. False when it drew nothing.
    ///
    /// An instance evicted for being idle keeps its trees: it is still enabled,
    /// and the next time it runs it draws them again or doesn't.
    pub fn forget(&mut self, plugin: &str) -> bool {
        let Some(plugin) = self.index(plugin) else {
            return false;
        };
        let mut changed = false;
        for slot in Slot::ALL {
            let before = self.drawn.len();
            self.drawn
                .retain(|drawn| !(drawn.plugin == plugin && drawn.slot == slot));
            if self.drawn.len() != before {
                self.changes[slot as usize] += 1;
                changed = true;
            }
        }
        let kept: VecDeque<Decoration> = self
            .decorations
            .iter()
            .filter(|kept| kept.plugin != plugin)
            .cloned()
            .collect();
        if kept.len() != self.decorations.len() {
            let gone: Vec<String> = self
                .decorations
                .iter()
                .filter(|kept| kept.plugin == plugin)
                .map(|kept| kept.message.clone())
                .collect();
            self.decorations = kept;
            self.decorated.extend(gone);
            self.changes[Slot::Message as usize] += 1;
            changed = true;
        }
        let before = self.badges.len();
        if self.badges.iter().any(|kept| kept.plugin == plugin) {
            let gone: Vec<String> = self
                .badges
                .iter()
                .filter(|kept| kept.plugin == plugin)
                .map(|kept| kept.member.clone())
                .collect();
            self.badges.retain(|kept| kept.plugin != plugin);
            self.badged.extend(gone);
            self.changes[Slot::ProfileBadges as usize] += 1;
            changed = true;
        }
        debug_assert!(before >= self.badges.len());
        changed
    }
    /// Takes everything away, which is what an install, an uninstall or a roll
    /// back does: the instances that drew it are gone, and what is drawn next
    /// comes from the code the store holds now.
    pub fn clear_all(&mut self) -> bool {
        let drawn = !self.drawn.is_empty();
        let decorated = !self.decorations.is_empty();
        let badged = !self.badges.is_empty();
        if !drawn && !decorated && !badged {
            return false;
        }
        self.drawn.clear();
        self.badged
            .extend(self.badges.drain(..).map(|gone| gone.member));
        self.decorated
            .extend(self.decorations.drain(..).map(|gone| gone.message));
        for slot in Slot::ALL {
            self.changes[slot as usize] += 1;
        }
        true
    }

    /// How many times a slot changed, for the UI to tell whether to show it
    /// again.
    pub fn changes(&self, slot: Slot) -> u64 {
        self.changes[slot as usize]
    }

    /// Draws a plugin's tree under a message, or takes it away; false when
    /// nothing changed.
    pub fn decorate(
        &mut self,
        plugin: &str,
        message: &str,
        tree: Option<Tree>,
        hide: Hidden,
    ) -> bool {
        let Some(plugin) = self.index(plugin) else {
            return false;
        };
        let place = self
            .decorations
            .iter()
            .position(|kept| kept.plugin == plugin && kept.message == message);
        let old = place.and_then(|place| self.decorations.remove(place));
        let tree = tree.map(Arc::new);
        // Nothing to draw and nothing hidden is not an entry: a plugin that
        // answered nothing leaves the row as it was.
        if tree.is_none() && hide.none() {
            if old.is_none() {
                return false;
            }
        } else {
            let unchanged = old.as_ref().is_some_and(|old| {
                old.hide == hide.row && old.hide_media == hide.media && old.tree == tree
            });
            self.decorations.push_back(Decoration {
                message: message.to_owned(),
                plugin,
                tree: old.filter(|_| unchanged).and_then(|old| old.tree).or(tree),
                hide: hide.row,
                hide_media: hide.media,
            });
            if self.decorations.len() > DECORATIONS {
                let gone = self.decorations.pop_front();
                self.decorated.extend(gone.map(|gone| gone.message));
            }
            if unchanged {
                return false;
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

    /// What is hidden for one message, across every plugin that asked: a row
    /// any of them hides is hidden, and so is one's media. The interface asks
    /// per row, so a row nobody filtered costs one scan of the decorations kept
    /// for it and nothing else.
    pub fn hidden(&self, message: &str) -> Hidden {
        let mut hidden = Hidden::default();
        for kept in self
            .decorations
            .iter()
            .filter(|kept| kept.message == message)
        {
            hidden.row |= kept.hide;
            hidden.media |= kept.hide_media;
        }
        hidden
    }

    /// The messages whose decorations changed since this was last called.
    pub fn take_decorated(&mut self) -> Vec<String> {
        std::mem::take(&mut self.decorated)
    }

    /// Sets, changes or takes away a plugin's badge for one person; false when
    /// nothing changed. A person keeps at most [`BADGES_PER_MEMBER`] badges, and
    /// an answer beyond that is dropped.
    ///
    /// An answer of none is kept too: the person was asked, so scrolling back
    /// to them asks nothing again.
    pub fn badge(&mut self, plugin: &str, member: &str, badge: Option<Badge>) -> bool {
        let Some(plugin) = self.index(plugin) else {
            return false;
        };
        let place = self
            .badges
            .iter()
            .position(|kept| kept.plugin == plugin && kept.member == member);
        let old = place.and_then(|place| self.badges.remove(place));
        if badge.is_some()
            && old.as_ref().is_none_or(|old| old.badge.is_none())
            && self
                .badges
                .iter()
                .filter(|kept| kept.member == member && kept.badge.is_some())
                .count()
                >= BADGES_PER_MEMBER
        {
            // The row keeps the badges it had: a person's name has room for a
            // few, and the plugin's answer is the one that came last.
            if let Some(old) = old {
                self.badges.push_back(old);
            }
            return false;
        }
        let was = old.as_ref().and_then(|old| old.badge.clone());
        self.badges.push_back(Badged {
            member: member.to_owned(),
            plugin,
            badge: badge.clone(),
        });
        while self.badges.len() > BADGES {
            if let Some(gone) = self.badges.pop_front() {
                self.badged.push(gone.member);
            }
        }
        if was == badge {
            return false;
        }
        self.badged.push(member.to_owned());
        self.changes[Slot::ProfileBadges as usize] += 1;
        true
    }

    /// What plugins badge a person with, in the order they first answered, each
    /// with its plugin's name for the row's screen reader.
    pub fn badges<'a>(
        &'a self,
        member: &'a str,
    ) -> impl Iterator<Item = (&'a PluginName, &'a Badge)> {
        self.badges
            .iter()
            .filter(move |kept| kept.member == member)
            .filter_map(|kept| Some((self.names.get(kept.plugin)?, kept.badge.as_ref()?)))
    }

    /// How many plugins have answered for a person, badged or not: a member the
    /// member list shows asks each of the rest again.
    pub fn members_asked(&self, member: &str) -> usize {
        self.badges
            .iter()
            .filter(|kept| kept.member == member)
            .count()
    }

    /// The people whose badges changed since this was last called.
    pub fn take_badged(&mut self) -> Vec<String> {
        std::mem::take(&mut self.badged)
    }

    /// Forgets every badge, which is what a new member list's snapshot does: a
    /// person's roles or name changed, so what a plugin said about them may no
    /// longer hold. The rows keep what they show until the list is built again.
    pub fn members_changed(&mut self) -> bool {
        if self.badges.is_empty() {
            return false;
        }
        self.badges.clear();
        true
    }

    pub fn apply(&mut self, output: Output) -> bool {
        match output {
            Output::SetSlot { plugin, slot, tree } => self.set(&plugin, slot, tree),
            Output::ClearSlot { plugin, slot } => self.clear(&plugin, slot),
            Output::Decorate {
                plugin,
                message,
                tree,
                hide,
            } => self.decorate(&plugin, &message, tree, hide),
            Output::MemberBadge {
                plugin,
                member,
                badge,
            } => self.badge(&plugin, &member, badge),
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
        assert!(slots.decorate("a.one", "m1", Some(badge.clone()), Hidden::default()));
        assert!(
            !slots.decorate("a.one", "m1", Some(badge.clone()), Hidden::default()),
            "the same"
        );
        assert!(slots.decorate("a.two", "m1", Some(badge.clone()), Hidden::default()));
        assert_eq!(slots.decorations("m1").count(), 2);
        assert_eq!(slots.take_decorated(), ["m1", "m1"]);
        assert!(slots.take_decorated().is_empty());
        assert!(slots.decorate("a.one", "m1", None, Hidden::default()));
        assert!(!slots.decorate("a.one", "m1", None, Hidden::default()));
        assert_eq!(slots.decorations("m1").count(), 1);
        for index in 0..DECORATIONS {
            slots.decorate(
                "a.one",
                &format!("x{index}"),
                Some(badge.clone()),
                Hidden::default(),
            );
        }
        assert_eq!(slots.decorations("m1").count(), 0, "the least recent went");
        assert!(slots.take_decorated().contains(&"m1".to_owned()));
    }

    #[test]
    fn a_row_is_hidden_when_any_plugin_says_so() {
        let mut slots = slots();
        let badge = tree(r#"{ "rows": [{ "cells": [{ "kind": "badge", "text": "Link" }] }] }"#);
        assert!(slots.hidden("m1").none(), "nothing is hidden to begin with");
        slots.decorate(
            "a.one",
            "m1",
            Some(badge.clone()),
            Hidden {
                row: false,
                media: true,
            },
        );
        assert_eq!(
            slots.hidden("m1"),
            Hidden {
                row: false,
                media: true
            }
        );
        slots.decorate(
            "a.two",
            "m1",
            None,
            Hidden {
                row: true,
                media: false,
            },
        );
        assert_eq!(
            slots.hidden("m1"),
            Hidden {
                row: true,
                media: true
            },
            "one plugin's row and another's media"
        );
        assert!(slots.hidden("m2").none(), "another row is untouched");
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
