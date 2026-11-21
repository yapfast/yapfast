//! The timeline's selection, as the app keeps it: by message rather than
//! by place, so it stays put as messages come and go, and what Ctrl+C
//! copies of it. The pieces that draw it and report presses and drags are
//! the kit's ([`crate::piece`]).

use yapfast_core::chat::ChatMessage;

use crate::piece::{Mark, Part, PartNo, Select, copied};
use crate::{markdown::FIRST_PART, timeline::RowLook};

// ---------------------------------------------------------------------------
// The timeline's selection, as the app keeps it

/// A place in the timeline's text by message rather than by its place, so
/// it stays put as messages come and go around it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sel {
    /// In the thread beside the channel.
    aside: bool,
    item: String,
    part: PartNo,
    line: usize,
    index: usize,
}

/// A piece's report: a press starts a selection, a drag moves its end.
pub fn select(app: &mut crate::app::App, select: Select) {
    let (Select::Start(mark, item) | Select::To(mark, item)) = &select;
    // A selection stays in the timeline it started in.
    let aside = mark.row >= crate::timeline::ASIDE;
    if let (Select::To(..), Some((from, _))) = (&select, &app.timeline.selection)
        && from.aside != aside
    {
        return;
    }
    // By its item: the row may have moved since the view was built.
    if crate::timeline::row_of(app, item).is_none() {
        return;
    }
    let at = Sel {
        aside,
        item: item.to_string(),
        part: mark.part,
        line: mark.line,
        index: mark.index,
    };
    match select {
        Select::Start(..) => {
            app.timeline.selection = Some((at.clone(), at));
            app.timeline.selecting = true;
        }
        Select::To(..) => {
            if app.timeline.selecting
                && let Some((_, to)) = &mut app.timeline.selection
            {
                *to = at;
            }
        }
    }
}

/// The button let go: a press that selected nothing selects nothing.
pub fn select_end(app: &mut crate::app::App) {
    app.timeline.selecting = false;
    if app
        .timeline
        .selection
        .as_ref()
        .is_some_and(|(from, to)| from == to)
    {
        app.timeline.selection = None;
    }
}

pub fn copied_selection(app: &crate::app::App) -> Option<String> {
    crate::timeline::lanes(app).copied()
}

impl crate::timeline::Lanes<'_> {
    /// The selection's two ends in timeline order, by the messages' places
    /// now.
    pub fn marks(self) -> Option<(Mark, Mark)> {
        let (a, b) = self.timeline.selection.as_ref()?;
        let place = |sel: &Sel| {
            let row = if sel.aside {
                crate::timeline::ASIDE
                    + self
                        .aside?
                        .messages
                        .iter()
                        .position(|message| message.item == sel.item)?
            } else {
                self.timeline
                    .messages
                    .iter()
                    .position(|message| message.item == sel.item)?
            };
            Some(Mark {
                row,
                part: sel.part,
                line: sel.line,
                index: sel.index,
            })
        };
        let (a, b) = (place(a)?, place(b)?);
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    /// The part of the selection in a message's piece.
    pub fn selected(self, row: usize, part: PartNo) -> Part {
        self.marks()
            .map_or(Part::None, |(from, to)| Part::of(from, to, row, part))
    }

    /// The timeline's selection as text: each message's author and time on
    /// a line, then its text, as far as the selection reaches, and
    /// `[Picture]` on a line for each picture it runs through.
    pub fn copied(self) -> Option<String> {
        let (from, to) = self.marks()?;
        let mut out = String::new();
        for row in from.row..=to.row {
            let (Some(message), Some(look)) = (self.row_message(row), self.row_look(row)) else {
                continue;
            };
            let line = copied_message(message, look, from, to, row);
            if !line.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&line);
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// What of one message the selection from `from` to `to` copies: its
/// author and time, then each part of its text, then its pictures when
/// the selection goes on into a later message.
fn copied_message(
    message: &ChatMessage,
    look: &RowLook,
    from: Mark,
    to: Mark,
    row: usize,
) -> String {
    let mut line = String::new();
    let parts = PartNo::try_from(look.body.parts.len()).unwrap_or(PartNo::MAX - FIRST_PART);
    for part in 0..FIRST_PART + parts {
        let shown = Part::of(from, to, row, part);
        if shown == Part::None {
            continue;
        }
        let (text, names): (String, Vec<&str>) = match part {
            0 => (message.author.clone(), Vec::new()),
            1 => (message.time.clone(), Vec::new()),
            _ => {
                let part = &look.body.parts[usize::from(part - FIRST_PART)];
                (part.plain(), part.emoji_names().collect())
            }
        };
        let piece = copied(&text, shown);
        // A custom emoji copies as its name, those before the selection
        // counted out.
        let piece = if names.is_empty() {
            piece
        } else {
            let before = match shown {
                Part::Range(from, _) => {
                    let start = text
                        .split('\n')
                        .take(from.0)
                        .map(|line| line.len() + 1)
                        .sum::<usize>()
                        + from.1;
                    text.get(..start.min(text.len()))
                        .map_or(0, |head| head.matches(crate::markdown::EMOJI_MARK).count())
                        .min(names.len())
                }
                _ => 0,
            };
            let mut names = names[before..].iter();
            piece
                .chars()
                .map(|c| match c {
                    crate::markdown::EMOJI_MARK => names
                        .next()
                        .map_or(c.to_string(), |name| (*name).to_owned()),
                    c => c.to_string(),
                })
                .collect()
        };
        if piece.is_empty() {
            continue;
        }
        if !line.is_empty() {
            line.push(if part >= FIRST_PART { '\n' } else { ' ' });
        }
        line.push_str(&piece);
    }
    // A selection running on past the message runs through the pictures
    // under its text.
    if to.row > row {
        let pictures = message.attachments.iter().filter(|attachment| {
            attachment.placement == yapfast_core::chat::Placement::Message
                && crate::media::pictured(attachment)
        });
        for _ in pictures {
            if !line.is_empty() {
                line.push('\n');
            }
            line.push_str("[Picture]");
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use yapfast_core::chat::ChatMessage;

    use super::{Mark, PartNo, copied_message};
    use crate::timeline::{RowLook, body_of};

    #[test]
    fn a_message_of_hundreds_of_parts_copies_whole() {
        // Alternating quoted and plain lines, each a part of its own.
        let body: Vec<String> = (0..600)
            .map(|line| {
                if line % 2 == 0 {
                    format!("> quoted {line}")
                } else {
                    format!("plain {line}")
                }
            })
            .collect();
        let message = ChatMessage {
            author: "Sam".into(),
            body: body.join("\n"),
            formatted: true,
            ..Default::default()
        };
        let look = RowLook {
            item: Arc::from(""),
            first: true,
            day: None,
            unread_start: false,
            highlighted: false,
            last: true,
            body: Arc::new(body_of(&message, 0, 0, 0, |_| false)),
            date: None,
            time: Arc::default(),
        };
        assert!(look.body.parts.len() > 300, "{}", look.body.parts.len());
        let start = Mark {
            row: 0,
            part: 0,
            line: 0,
            index: 0,
        };
        let end = Mark {
            row: 0,
            part: PartNo::MAX,
            line: usize::MAX,
            index: usize::MAX,
        };
        let copied = copied_message(&message, &look, start, end, 0);
        assert!(copied.contains("plain 599"), "{} bytes", copied.len());
    }

    #[test]
    fn a_selection_copies_the_pictures_it_runs_through() {
        let message = ChatMessage {
            author: "Sam".into(),
            time: "19:20".into(),
            body: "Look at this.".into(),
            attachments: vec![yapfast_core::chat::Attachment {
                name: "cat.png".into(),
                size: None,
                image: true,
                dimensions: Some((400, 300)),
                preview: None,
                preview_failed: false,
                placement: yapfast_core::chat::Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
                description: None,
            }],
            ..Default::default()
        };
        let look = RowLook {
            item: Arc::from(""),
            first: true,
            day: None,
            unread_start: false,
            highlighted: false,
            last: true,
            body: Arc::new(body_of(&message, 0, 0, 0, |_| false)),
            date: None,
            time: Arc::default(),
        };
        let mark = |row, part| Mark {
            row,
            part,
            line: if part == 0 { 0 } else { usize::MAX },
            index: if part == 0 { 0 } else { usize::MAX },
        };
        // From its author into the next message: the picture too.
        assert_eq!(
            copied_message(&message, &look, mark(0, 0), mark(1, 0), 0),
            "Sam 19:20\nLook at this.\n[Picture]"
        );
        // To the end of its text and no further: the text alone.
        assert_eq!(
            copied_message(&message, &look, mark(0, 0), mark(0, PartNo::MAX), 0),
            "Sam 19:20\nLook at this."
        );
    }
}
