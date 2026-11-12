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

use std::collections::HashSet;
use std::sync::Arc;

use crate::chat::ChatMessage;

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
        let again = keep(kept.to_vec(), &kept);
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
