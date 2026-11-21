//! Channel Permissions' draft: the channel's overwrites as Discord has
//! them, and those the draft changes, whole.

use crate::chat::{ChannelOverwrite, OverwriteChange, RoleMember};

/// What an overwrite does with a permission: Discord's three choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwriteState {
    Deny,
    /// Neither: the server's roles decide.
    Neutral,
    Allow,
}

/// What Channel Permissions asks before doing it, or tells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverwriteConfirm {
    /// Taking a role's or member's overwrite off, by its id.
    Remove(String),
    /// Giving the channel its category's overwrites.
    Sync,
    /// A change refused because it would take the permission from this
    /// account too, with the name of the role or member it was for.
    DenySelf(String),
}

/// What this account may change in a channel's overwrites.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OverwriteRules {
    /// Its user id.
    pub me: String,
    /// It has every permission: the owner, or an administrator.
    pub all: bool,
    /// The channel's own overwrites give it Manage Permissions.
    pub granted: bool,
    /// Its permissions across the server.
    pub held: u64,
}

impl OverwriteRules {
    /// Whether this account can't change what an overwrite does with `bit`,
    /// as Discord's client decides: it may, with every permission or Manage
    /// Permissions from the channel itself; otherwise only a permission it
    /// holds across the server, and never Manage Permissions.
    pub fn locked(&self, bit: u32) -> bool {
        if self.all || self.granted {
            return false;
        }
        bit == super::MANAGE_ROLES || self.held & (1 << bit) == 0
    }
}

/// Add's picker: what was typed, and the members it found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddOverwrite {
    pub query: String,
    pub found: Vec<RoleMember>,
}

/// Channel Permissions' page.
#[derive(Clone, Debug)]
pub struct Overwrites {
    /// @everyone's id, the server's.
    pub everyone: String,
    /// The overwrites the latest snapshot had, which Discord's answers to
    /// changes can be ahead of.
    seen: Vec<ChannelOverwrite>,
    /// The overwrites as Discord has them.
    pub original: Vec<ChannelOverwrite>,
    /// Those the draft changes or adds, whole.
    pub edits: Vec<ChannelOverwrite>,
    /// The overwrite shown, by its id.
    pub selected: String,
    /// The names of the members with overwrites, as Discord gave them.
    pub names: Vec<RoleMember>,
    /// Members whose names were asked for.
    asked: Vec<String>,
    pub adding: Option<AddOverwrite>,
    pub confirm: Option<OverwriteConfirm>,
    /// The change Discord is taking.
    pub pending: Option<OverwriteChange>,
}

impl Overwrites {
    pub fn new(original: Vec<ChannelOverwrite>, everyone: String) -> Self {
        Self {
            selected: everyone.clone(),
            everyone,
            seen: original.clone(),
            original,
            edits: vec![],
            names: vec![],
            asked: vec![],
            adding: None,
            confirm: None,
            pending: None,
        }
    }

    pub fn is_everyone(&self, overwrite: &ChannelOverwrite) -> bool {
        !overwrite.member && overwrite.id == self.everyone
    }

    /// The overwrites as the draft has them.
    pub fn draft(&self) -> Vec<ChannelOverwrite> {
        let mut draft = self.original.clone();
        for edit in &self.edits {
            match draft.iter_mut().find(|known| known.id == edit.id) {
                Some(known) => *known = edit.clone(),
                None => draft.push(edit.clone()),
            }
        }
        draft
    }

    /// Whether the draft has an overwrite for this id.
    pub fn has(&self, id: &str) -> bool {
        self.original
            .iter()
            .chain(&self.edits)
            .any(|known| known.id == id)
    }

    /// The shown overwrite as the draft has it; @everyone's is empty when
    /// the channel has none.
    pub fn shown(&self) -> ChannelOverwrite {
        self.overwrite(&self.selected)
    }

    fn overwrite(&self, id: &str) -> ChannelOverwrite {
        self.edits
            .iter()
            .chain(&self.original)
            .find(|known| known.id == id)
            .cloned()
            .unwrap_or_else(|| ChannelOverwrite {
                id: id.to_owned(),
                ..Default::default()
            })
    }

    /// `overwrite` with `bit` set to `state`.
    pub fn with(
        &self,
        overwrite: &ChannelOverwrite,
        bit: u32,
        state: OverwriteState,
    ) -> ChannelOverwrite {
        let flag = 1u64 << bit;
        let mut changed = overwrite.clone();
        changed.allow &= !flag;
        changed.deny &= !flag;
        match state {
            OverwriteState::Allow => changed.allow |= flag,
            OverwriteState::Deny => changed.deny |= flag,
            OverwriteState::Neutral => {}
        }
        changed
    }

    /// What an overwrite does with `bit`, as Discord's client reads it:
    /// allowing wins over denying.
    pub fn state(overwrite: &ChannelOverwrite, bit: u32) -> OverwriteState {
        let flag = 1u64 << bit;
        if overwrite.allow & flag != 0 {
            OverwriteState::Allow
        } else if overwrite.deny & flag != 0 {
            OverwriteState::Deny
        } else {
            OverwriteState::Neutral
        }
    }

    /// Puts an overwrite in the draft, or takes it out where it's as
    /// Discord has it.
    pub fn edit(&mut self, changed: ChannelOverwrite) {
        self.edits.retain(|known| known.id != changed.id);
        let original = self.original.iter().find(|known| known.id == changed.id);
        let same = match original {
            Some(original) => *original == changed,
            // @everyone with nothing set is as good as none.
            None => self.is_everyone(&changed) && changed.allow == 0 && changed.deny == 0,
        };
        if !same {
            self.edits.push(changed);
        }
    }

    pub fn changed(&self) -> bool {
        !self.edits.is_empty()
    }

    /// The draft's edits in an order Discord takes, one PUT at a time:
    /// those that only give first, then this account's own, then the rest,
    /// @everyone last. Discord checks each against what the ones before it
    /// left, and a denied View Channel denies Manage Permissions too, so
    /// Private Channel's deny for @everyone before the account's own allow
    /// would lock it out of the channel halfway.
    pub fn to_save(&self, me: &str) -> Vec<ChannelOverwrite> {
        let mut edits = self.edits.clone();
        edits.sort_by_key(|edit| {
            let (allow, deny) = self
                .original
                .iter()
                .find(|known| known.id == edit.id)
                .map_or((0, 0), |known| (known.allow, known.deny));
            if edit.deny & !deny == 0 && allow & !edit.allow == 0 {
                0
            } else if edit.member && edit.id == me {
                1
            } else if self.is_everyone(edit) {
                3
            } else {
                2
            }
        });
        edits
    }

    pub fn undo(&mut self) {
        self.edits.clear();
        if !self.has(&self.selected) {
            self.selected = self.everyone.clone();
        }
    }

    /// Whether @everyone, as the draft has it, is denied all of `access`,
    /// as Discord calls a channel private.
    pub fn private(&self, access: u64) -> bool {
        self.overwrite(&self.everyone).deny & access == access
    }

    /// Makes the channel private or not, as Discord's switch does: @everyone
    /// is denied `access`, the permissions that reach the channel, or no
    /// longer either way. Without every permission, this account keeps
    /// `access` with an overwrite of its own.
    pub fn make_private(&mut self, on: bool, access: u64, rules: &OverwriteRules) {
        let mut everyone = self.overwrite(&self.everyone);
        everyone.member = false;
        everyone.allow &= !access;
        if on {
            everyone.deny |= access;
        } else {
            everyone.deny &= !access;
        }
        self.edit(everyone);
        if on && !rules.all {
            let mut mine = self.overwrite(&rules.me);
            mine.member = true;
            mine.allow |= access;
            mine.deny &= !access;
            self.edit(mine);
        }
    }

    /// Takes the overwrites a new snapshot has, once they differ from the
    /// last snapshot's: a change Discord took already shows, and an older
    /// snapshot doesn't undo it. The draft's edits stay, but for those now
    /// as Discord has them.
    pub fn follow(&mut self, overwrites: Vec<ChannelOverwrite>) {
        if overwrites == self.seen {
            return;
        }
        self.seen = overwrites.clone();
        self.original = overwrites;
        let edits = std::mem::take(&mut self.edits);
        for edit in edits {
            self.edit(edit);
        }
        if !self.has(&self.selected) && self.pending.is_none() {
            self.selected = self.everyone.clone();
        }
    }

    /// Takes a change Discord took, ahead of the snapshot that shows it.
    pub fn took(&mut self, change: OverwriteChange) {
        match change {
            OverwriteChange::Put(put) => {
                for overwrite in put {
                    self.edits.retain(|known| known.id != overwrite.id);
                    match self
                        .original
                        .iter_mut()
                        .find(|known| known.id == overwrite.id)
                    {
                        Some(known) => *known = overwrite,
                        None => self.original.push(overwrite),
                    }
                }
            }
            OverwriteChange::Delete(id) => {
                self.original.retain(|known| known.id != id);
                self.edits.retain(|known| known.id != id);
                if self.selected == id {
                    self.selected = self.everyone.clone();
                }
            }
            OverwriteChange::Sync(overwrites) => {
                self.original = overwrites;
                self.edits.clear();
                if !self.has(&self.selected) {
                    self.selected = self.everyone.clone();
                }
            }
        }
    }

    /// Members with overwrites whose names weren't asked for yet, which it
    /// notes as asked.
    pub fn unnamed(&mut self) -> Vec<String> {
        let unnamed: Vec<String> = self
            .original
            .iter()
            .chain(&self.edits)
            .filter(|known| known.member)
            .map(|known| known.id.clone())
            .filter(|id| {
                !self.names.iter().any(|known| known.id == *id) && !self.asked.contains(id)
            })
            .collect();
        self.asked.extend(unnamed.iter().cloned());
        unnamed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overwrite(id: &str, member: bool, allow: u64, deny: u64) -> ChannelOverwrite {
        ChannelOverwrite {
            id: id.into(),
            member,
            allow,
            deny,
        }
    }

    #[test]
    fn a_change_back_to_discords_leaves_the_draft_unchanged() {
        let mut page = Overwrites::new(vec![overwrite("20", false, 1 << 11, 0)], "10".into());
        page.selected = "20".into();
        let denied = page.with(&page.shown(), 11, OverwriteState::Deny);
        assert_eq!((denied.allow, denied.deny), (0, 1 << 11));
        page.edit(denied);
        assert!(page.changed());
        let back = page.with(&page.shown(), 11, OverwriteState::Allow);
        page.edit(back);
        assert!(!page.changed());
        // @everyone has none, and neutral is as good as none.
        page.selected = "10".into();
        let neutral = page.with(&page.shown(), 11, OverwriteState::Neutral);
        page.edit(neutral);
        assert!(!page.changed());
    }

    #[test]
    fn private_denies_everyone_access_and_keeps_it_for_this_account() {
        let mut page = Overwrites::new(vec![], "10".into());
        let rules = OverwriteRules {
            me: "1".into(),
            ..Default::default()
        };
        let access = (1 << 10) | (1 << 20);
        page.make_private(true, access, &rules);
        assert!(page.private(access));
        assert_eq!(
            page.draft(),
            [
                overwrite("10", false, 0, access),
                overwrite("1", true, access, 0)
            ]
        );
        page.make_private(false, access, &rules);
        assert!(!page.private(access));
        assert_eq!(page.edits, [overwrite("1", true, access, 0)]);
    }

    #[test]
    fn an_older_snapshot_doesnt_undo_what_discord_took() {
        let before = vec![overwrite("20", false, 0, 0)];
        let mut page = Overwrites::new(before.clone(), "10".into());
        page.selected = "20".into();
        let allowed = page.with(&page.shown(), 11, OverwriteState::Allow);
        page.edit(allowed.clone());
        page.took(OverwriteChange::Put(page.edits.clone()));
        assert!(!page.changed());
        page.follow(before);
        assert_eq!(
            page.original,
            std::slice::from_ref(&allowed),
            "the same snapshot as before"
        );
        let elsewhere = overwrite("20", false, 0, 1 << 11);
        page.follow(vec![elsewhere.clone()]);
        assert_eq!(page.original, [elsewhere]);
    }

    #[test]
    fn locks_follow_discords_rules() {
        let held = OverwriteRules {
            held: 1 << 11,
            ..Default::default()
        };
        assert!(!held.locked(11) && held.locked(13) && held.locked(28));
        let granted = OverwriteRules {
            granted: true,
            ..held
        };
        assert!(!granted.locked(13) && !granted.locked(28));
    }
}
