//! How the timeline's rows group, for the projection to mark them: a group
//! of one person's messages within seven minutes, a new day's date, the
//! "New" divider, and a row's key. The Slint side: `continues`, `day` and
//! the loop of `show_timeline` (`src/ui/messages.rs:18`, `:36`, `:118`-`:149`).
//! The dates are the core's (`yapfast_core::clock::date` and `day`): the
//! projection hands each message's date in and writes the day's text where
//! [`Mark::new_day`] says.

use std::hash::{DefaultHasher, Hash, Hasher};

/// Messages from one person this close together share a group.
pub const GROUP_WINDOW: u64 = 7 * 60;

/// What grouping reads of a message.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sent<'a, D> {
    /// Who sent it: their id, or their name where it has none.
    pub sender: &'a str,
    /// When, in Unix seconds; 0 when unknown.
    pub timestamp: u64,
    /// A notice, as one that someone pinned a message.
    pub notice: bool,
    /// It replies to another message.
    pub reply: bool,
    /// It answers someone's command (`used`).
    pub command: bool,
    /// Its local calendar date; none when unknown.
    pub date: Option<D>,
    /// The "New" divider goes over it.
    pub unread: bool,
}

/// What a row starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    /// A group: its author shows again.
    pub first: bool,
    /// A new day: its date shows over it.
    pub new_day: bool,
}

/// A message continues the previous one's group when the same person sent
/// it within seven minutes, without replying to something else or
/// answering a command. A notice is a group of its own.
pub fn continues<D>(before: &Sent<'_, D>, message: &Sent<'_, D>) -> bool {
    if before.notice || message.notice {
        return false;
    }
    let close = before.timestamp == 0
        || message.timestamp == 0
        || message.timestamp.abs_diff(before.timestamp) <= GROUP_WINDOW;
    !message.reply && !message.command && before.sender == message.sender && close
}

/// Each message's marks, oldest first: a new day where its date differs
/// from the one before, and a group where a day starts, the "New" divider
/// is over it, or it doesn't continue the one before.
pub fn marks<'a, D: PartialEq + 'a>(messages: impl IntoIterator<Item = Sent<'a, D>>) -> Vec<Mark> {
    let messages = messages.into_iter();
    let mut marks = Vec::with_capacity(messages.size_hint().0);
    let mut before: Option<Sent<'a, D>> = None;
    for message in messages {
        let new_day = message.date.is_some()
            && before
                .as_ref()
                .is_none_or(|before| before.date != message.date);
        let first = new_day
            || message.unread
            || before
                .as_ref()
                .is_none_or(|before| !continues(before, &message));
        marks.push(Mark { first, new_day });
        before = Some(message);
    }
    marks
}

/// A row's key from its message's id: the same id, the same key, from one
/// projection to the next.
pub fn key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(sender: &str, timestamp: u64) -> Sent<'_, u32> {
        Sent {
            sender,
            timestamp,
            date: Some(1),
            ..Sent::default()
        }
    }

    fn firsts<'a>(messages: impl IntoIterator<Item = Sent<'a, u32>>) -> Vec<bool> {
        marks(messages).into_iter().map(|mark| mark.first).collect()
    }

    #[test]
    fn one_persons_messages_group_within_seven_minutes() {
        let at = 1_790_856_000;
        assert_eq!(
            firsts([
                sent("bob", at),
                sent("bob", at + 60),
                sent("bob", at + 60 + GROUP_WINDOW),
                sent("bob", at + 60 + GROUP_WINDOW + 1 + GROUP_WINDOW),
                sent("carol", at + 2_000),
            ]),
            [true, false, false, true, true]
        );
        // Without a time, the same person's messages group.
        assert_eq!(firsts([sent("bob", 0), sent("bob", at)]), [true, false]);
    }

    #[test]
    fn a_notice_a_reply_and_an_apps_answer_start_groups() {
        let at = 1_790_856_000;
        let notice = Sent {
            notice: true,
            ..sent("bob", at + 1)
        };
        let reply = Sent {
            reply: true,
            ..sent("bob", at + 3)
        };
        let answer = Sent {
            command: true,
            ..sent("bob", at + 4)
        };
        assert_eq!(
            firsts([
                sent("bob", at),
                notice,
                sent("bob", at + 2),
                reply,
                answer,
                sent("bob", at + 5),
            ]),
            [true, true, true, true, true, false]
        );
    }

    #[test]
    fn a_new_day_and_the_new_divider_show_the_author_again() {
        let at = 1_790_856_000;
        let tomorrow = Sent {
            date: Some(2),
            ..sent("bob", at + 60)
        };
        let unread = Sent {
            unread: true,
            ..tomorrow
        };
        assert_eq!(
            marks([sent("bob", at), tomorrow, unread]),
            [
                Mark {
                    first: true,
                    new_day: true
                },
                Mark {
                    first: true,
                    new_day: true
                },
                Mark {
                    first: true,
                    new_day: false
                },
            ]
        );
        // A message without a date starts no day.
        let undated = Sent {
            date: None,
            ..sent("bob", at)
        };
        assert_eq!(marks([undated]), [Mark::default().with_first()]);
    }

    #[test]
    fn a_key_is_its_ids() {
        assert_eq!(key("m1"), key("m1"));
        assert_ne!(key("m1"), key("m2"));
    }

    impl Mark {
        fn with_first(self) -> Self {
            Self {
                first: true,
                ..self
            }
        }
    }
}
