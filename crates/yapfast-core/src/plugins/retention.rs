//! `messages.retain`: a row that is deleted stays on screen, flagged.
//!
//! A conversation's timeline is what the backend last sent: Discord drops a
//! deleted message from it and sends the list again, so by the time Yapfast
//! hears the new list the row is gone. A plugin that asked for `messages.retain`
//! wants the row to stay, marked deleted, so the person can see that something
//! was taken away and read what it said from the plugin's own store.
//!
//! So this is the one place the application ignores part of what the backend
//! sent: the rows it already had, and the new list no longer holds, are kept in
//! place and flagged, for at most `KEPT_ROWS` of them. It costs one set of
//! borrowed ids for the arriving list and one pass over each, and nothing at all
//! when no plugin asked for it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::chat::ChatMessage;

/// How a kept deleted row is drawn, as the retaining plugin chose with
/// `messages.retain_style`: any of these together. Red text, a soft red
/// ground and a "deleted" tag beside the time when no plugin said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub red_text: bool,
    pub red_ground: bool,
    pub strike: bool,
    pub faded: bool,
    pub tag: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            red_text: true,
            red_ground: true,
            strike: false,
            faded: false,
            tag: true,
        }
    }
}

/// The most rows one arrival keeps as deleted.
///
/// A moderator clearing a channel sends one list with a thousand rows missing,
/// and a copy of every one of them would grow the timeline by the channel's
/// whole past: past this, the oldest of them go, so what stays is the newest
/// page of what was taken away.
const KEPT_ROWS: usize = 200;

/// The arriving timeline with the rows it dropped kept, flagged deleted.
///
/// A kept row goes where it was: after the nearest row before it that is still
/// there, so a deletion leaves a gap in the conversation rather than a message
/// at the end. Nothing is kept that the arriving list already has, a row that
/// was already flagged deleted stays flagged rather than being flagged twice,
/// and a list that dropped more than `KEPT_ROWS` rows keeps the newest of
/// them.
pub fn keep(
    arriving: Vec<Arc<ChatMessage>>,
    current: &[Arc<ChatMessage>],
) -> Vec<Arc<ChatMessage>> {
    // The ids only, and borrowed from the rows themselves: a timeline arrives
    // on every message, and a copy of every id would be made and dropped each
    // time. A set rather than a list because this is asked once per row already
    // in memory.
    let present: HashSet<&str> = arriving
        .iter()
        .map(|message| message.item.as_str())
        .collect();
    // What this arrival is about to keep, so the oldest of a mass delete are the
    // ones let go.
    let mut over = current
        .iter()
        .filter(|old| !old.deleted && !present.contains(old.item.as_str()))
        .count()
        .saturating_sub(KEPT_ROWS);
    let mut kept: Vec<Arc<ChatMessage>> = Vec::with_capacity(arriving.len());
    // How far into the arriving list the walk has got, so each row is taken once
    // and in the order the backend sent it.
    let mut next = 0;
    for old in current {
        if present.contains(old.item.as_str()) {
            // Everything up to and including this row, as the backend sent it.
            while let Some(row) = arriving.get(next) {
                next += 1;
                let reached = row.item == old.item;
                kept.push(Arc::clone(row));
                if reached {
                    break;
                }
            }
            continue;
        }
        if old.deleted {
            continue;
        }
        if over > 0 {
            over -= 1;
            continue;
        }
        let mut gone = (**old).clone();
        gone.deleted = true;
        gone.reactions.clear();
        gone.can_reply = false;
        gone.can_edit = false;
        gone.can_delete = false;
        gone.can_react = false;
        gone.can_forward = false;
        kept.push(Arc::new(gone));
    }
    // The ids are borrowed from the list this moves out of.
    drop(present);
    kept.extend(arriving.into_iter().skip(next));
    kept
}

/// The most deleted rows kept for one conversation across restarts; past
/// this, the oldest go.
pub const KEPT_PER_ROOM: usize = 500;

/// A kept deleted row as it is written down, so it is shown again after a
/// restart: who wrote it, when, and what it said. Its reactions, pictures and
/// embeds aren't kept; the retaining plugin keeps what it wants of those.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Saved {
    pub item: String,
    pub id: String,
    pub author: String,
    pub sender: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(default)]
    pub bot: bool,
    #[serde(default)]
    pub webhook: bool,
    pub body: String,
    #[serde(default)]
    pub formatted: bool,
    pub time: String,
    pub timestamp: u64,
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub edited: bool,
    /// When it was seen to go, in Unix milliseconds, for its tag.
    pub deleted_at: u64,
}

impl Saved {
    fn of(message: &ChatMessage, deleted_at: u64) -> Saved {
        Saved {
            item: message.item.clone(),
            id: message.id.clone(),
            author: message.author.clone(),
            sender: message.sender.clone(),
            avatar: message.avatar.clone(),
            color: message.color,
            bot: message.bot,
            webhook: message.webhook,
            body: message.body.clone(),
            formatted: message.formatted,
            time: message.time.clone(),
            timestamp: message.timestamp,
            own: message.own,
            edited: message.edited,
            deleted_at,
        }
    }

    /// The row it was, flagged deleted, and nothing on it to press.
    pub fn row(&self) -> ChatMessage {
        ChatMessage {
            item: self.item.clone(),
            id: self.id.clone(),
            author: self.author.clone(),
            sender: self.sender.clone(),
            avatar: self.avatar.clone(),
            color: self.color,
            bot: self.bot,
            webhook: self.webhook,
            body: self.body.clone(),
            formatted: self.formatted,
            time: self.time.clone(),
            timestamp: self.timestamp,
            own: self.own,
            edited: self.edited,
            deleted: true,
            ..ChatMessage::default()
        }
    }
}

/// The deleted rows kept per conversation, written to
/// `<data>/plugins-retained/<conversation>.json` so they outlive a restart,
/// and read once per conversation per session, when it first loads while a
/// plugin holds `messages.retain`.
#[derive(Debug, Default)]
pub struct Kept {
    dir: PathBuf,
    rooms: HashMap<String, Vec<Saved>>,
}

impl Kept {
    pub fn new(dir: PathBuf) -> Kept {
        Kept {
            dir,
            rooms: HashMap::new(),
        }
    }

    /// A conversation's file: its id with anything but letters, digits, `-`
    /// and `_` written as `_`.
    fn path(&self, room: &str) -> PathBuf {
        let name: String = room
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{name}.json"))
    }

    fn room(&mut self, room: &str) -> &mut Vec<Saved> {
        if !self.rooms.contains_key(room) {
            let read = std::fs::read(self.path(room))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Vec<Saved>>(&bytes).ok())
                .unwrap_or_default();
            self.rooms.insert(room.to_owned(), read);
        }
        self.rooms.entry(room.to_owned()).or_default()
    }

    fn write(&self, room: &str) {
        let Some(rows) = self.rooms.get(room) else {
            return;
        };
        let path = self.path(room);
        let written = if rows.is_empty() {
            match std::fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            }
        } else {
            std::fs::create_dir_all(&self.dir).and_then(|()| {
                let bytes = serde_json::to_vec(rows).map_err(std::io::Error::other)?;
                let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
                std::io::Write::write_all(&mut file, &bytes)?;
                file.persist(&path).map(|_| ()).map_err(|error| error.error)
            })
        };
        if let Err(error) = written {
            tracing::warn!(%error, "couldn't keep a conversation's deleted messages");
        }
    }

    /// The arriving timeline with the rows this conversation kept from an
    /// earlier run put back, flagged, by their time: only those inside the
    /// window that arrived (at or after its first row), and none the list
    /// already holds. Their deletion times go into `at`.
    pub fn restore(
        &mut self,
        room: &str,
        arriving: Vec<Arc<ChatMessage>>,
        at: &mut HashMap<String, u64>,
    ) -> Vec<Arc<ChatMessage>> {
        let Some(first) = arriving.first().map(|message| message.timestamp) else {
            return arriving;
        };
        let saved = self.room(room);
        if saved.is_empty() {
            return arriving;
        }
        let present: HashSet<&str> = arriving
            .iter()
            .map(|message| message.item.as_str())
            .collect();
        let mut back: Vec<&Saved> = saved
            .iter()
            .filter(|row| row.timestamp >= first && !present.contains(row.item.as_str()))
            .collect();
        drop(present);
        if back.is_empty() {
            return arriving;
        }
        back.sort_by_key(|row| row.timestamp);
        let mut merged = Vec::with_capacity(arriving.len() + back.len());
        let mut back = back.into_iter().peekable();
        for message in arriving {
            while let Some(row) = back.next_if(|row| row.timestamp < message.timestamp) {
                at.entry(row.item.clone()).or_insert(row.deleted_at);
                merged.push(Arc::new(row.row()));
            }
            merged.push(message);
        }
        for row in back {
            at.entry(row.item.clone()).or_insert(row.deleted_at);
            merged.push(Arc::new(row.row()));
        }
        merged
    }

    /// Writes down the rows of `messages` flagged deleted that this
    /// conversation hasn't kept yet, the newest [`KEPT_PER_ROOM`]; nothing is
    /// written when none is new.
    pub fn remember(
        &mut self,
        room: &str,
        messages: &[Arc<ChatMessage>],
        at: &HashMap<String, u64>,
    ) {
        let saved = self.room(room);
        let known: HashSet<String> = saved.iter().map(|row| row.item.clone()).collect();
        let new: Vec<Saved> = messages
            .iter()
            .filter(|message| message.deleted && !known.contains(&message.item))
            .map(|message| {
                Saved::of(
                    message,
                    at.get(&message.item)
                        .copied()
                        .unwrap_or_else(crate::clock::now_ms),
                )
            })
            .collect();
        if new.is_empty() {
            return;
        }
        saved.extend(new);
        saved.sort_by_key(|row| row.timestamp);
        let over = saved.len().saturating_sub(KEPT_PER_ROOM);
        saved.drain(..over);
        self.write(room);
    }

    /// Forgets one kept row: the person's "Hide this deleted message".
    pub fn forget(&mut self, room: &str, item: &str) {
        let saved = self.room(room);
        let before = saved.len();
        saved.retain(|row| row.item != item);
        if saved.len() != before {
            self.write(room);
        }
    }
}

/// Whether a row a plugin kept is still the one on screen, which is what the
/// interface asks when it draws a row flagged deleted.
pub fn is_kept(message: &ChatMessage) -> bool {
    message.deleted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(item: &str, deleted: bool) -> Arc<ChatMessage> {
        Arc::new(ChatMessage {
            item: item.to_owned(),
            body: format!("body of {item}"),
            deleted,
            ..ChatMessage::default()
        })
    }

    #[test]
    fn a_row_the_new_timeline_dropped_stays_where_it_was_and_is_flagged() {
        let current = vec![
            message("1", false),
            message("2", false),
            message("3", false),
        ];
        let arriving = vec![message("1", false), message("3", false)];
        let kept = keep(arriving, &current);
        let items: Vec<&str> = kept.iter().map(|message| message.item.as_str()).collect();
        assert_eq!(items, vec!["1", "2", "3"]);
        assert!(kept[1].deleted, "the one that went is flagged");
        assert!(!kept[0].deleted);
        assert!(!kept[2].deleted);
        assert_eq!(kept[1].body, "body of 2");
    }

    #[test]
    fn a_row_at_the_end_stays_at_the_end_and_nothing_is_kept_twice() {
        let current = vec![message("1", false), message("2", false)];
        let arriving = vec![message("1", false)];
        let kept = keep(arriving, &current);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].item, "2");
        assert!(kept[1].deleted);

        // The same list again keeps nothing new: the row is already flagged,
        // and a row that arrives is never replaced by a kept copy.
        let again = keep(kept.clone(), &kept);
        assert_eq!(again.len(), 2);
    }

    #[test]
    fn a_row_that_comes_back_is_the_backends_own() {
        let current = vec![message("1", false), message("2", true)];
        let arriving = vec![message("1", false), message("2", false)];
        let kept = keep(arriving, &current);
        assert_eq!(kept.len(), 2);
        assert!(!kept[1].deleted, "the backend's row wins over a kept copy");
    }

    #[test]
    fn a_whole_conversation_that_went_keeps_its_rows_in_order() {
        let current = vec![message("1", false), message("2", false)];
        let kept = keep(Vec::new(), &current);
        let items: Vec<&str> = kept.iter().map(|message| message.item.as_str()).collect();
        assert_eq!(items, vec!["1", "2"]);
        assert!(kept.iter().all(|message| message.deleted));
    }

    fn at(item: &str, timestamp: u64, deleted: bool) -> Arc<ChatMessage> {
        Arc::new(ChatMessage {
            timestamp,
            ..(*message(item, deleted)).clone()
        })
    }

    #[test]
    fn kept_rows_come_back_after_a_restart_in_their_place() {
        let folder = tempfile::tempdir().expect("a folder");
        let dir = folder.path().join("plugins-retained");
        let mut times = HashMap::new();
        times.insert("d:2".to_owned(), 77);
        {
            let mut kept = Kept::new(dir.clone());
            let rows = vec![
                at("d:1", 10, false),
                at("d:2", 20, true),
                at("d:3", 30, false),
            ];
            kept.remember("discord:1:2", &rows, &times);
            // Nothing new: nothing written again, and nothing twice.
            kept.remember("discord:1:2", &rows, &times);
        }
        assert!(dir.join("discord_1_2.json").exists());

        // A new run: the backend's list doesn't have it, and it goes back
        // between its neighbours, flagged, with when it went.
        let mut kept = Kept::new(dir.clone());
        let mut restored_at = HashMap::new();
        let back = kept.restore(
            "discord:1:2",
            vec![at("d:1", 10, false), at("d:3", 30, false)],
            &mut restored_at,
        );
        let items: Vec<&str> = back.iter().map(|row| row.item.as_str()).collect();
        assert_eq!(items, vec!["d:1", "d:2", "d:3"]);
        assert!(back[1].deleted && !back[1].can_reply);
        assert_eq!(back[1].body, "body of d:2");
        assert_eq!(restored_at.get("d:2"), Some(&77));

        // A window that starts after it leaves it out, and another
        // conversation has none.
        let later = kept.restore("discord:1:2", vec![at("d:3", 30, false)], &mut restored_at);
        assert_eq!(later.len(), 1);
        let other = kept.restore("discord:1:9", vec![at("d:1", 10, false)], &mut restored_at);
        assert_eq!(other.len(), 1);

        kept.forget("discord:1:2", "d:2");
        assert!(
            !dir.join("discord_1_2.json").exists(),
            "the last one hidden"
        );
    }

    #[test]
    fn a_conversation_keeps_its_newest_rows() {
        let folder = tempfile::tempdir().expect("a folder");
        let mut kept = Kept::new(folder.path().to_owned());
        let rows: Vec<_> = (0..KEPT_PER_ROOM as u64 + 20)
            .map(|row| at(&format!("d:{row}"), row, true))
            .collect();
        kept.remember("r", &rows, &HashMap::new());
        let saved = kept.room("r");
        assert_eq!(saved.len(), KEPT_PER_ROOM);
        assert_eq!(saved[0].item, "d:20");
    }

    #[test]
    fn a_mass_delete_keeps_the_newest_page_of_what_went() {
        let current: Vec<Arc<ChatMessage>> = (0..KEPT_ROWS + 50)
            .map(|row| message(&row.to_string(), false))
            .collect();
        let kept = keep(Vec::new(), &current);
        assert_eq!(kept.len(), KEPT_ROWS, "one arrival keeps one page");
        assert!(kept.iter().all(|message| message.deleted));
        // The oldest of them went first, so what a person reads is the newest.
        let oldest_kept = kept.first().map(|message| message.item.as_str());
        assert_eq!(oldest_kept, Some("50"));
    }
}
