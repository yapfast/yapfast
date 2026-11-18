//! A channel's topic as text: its mentions linked, its custom emoji as
//! pictures once they came (`topic_text`, `src/ui/messages.rs:1349`). Made
//! again only when the topic changes, or the pictures of its custom emoji
//! arrive; the header's line and the popover share it.

use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::Topic;

use anomi_vm::text::{self, CustomEmoji, Mention, Source, Text};

use crate::pictures::{self, Requests};
use crate::project::rows;

/// The topic made last, and from what.
pub(crate) struct Shown {
    topic: Arc<Topic>,
    /// The pictures' count it was made at, for a topic with custom emoji.
    pictures: Option<u64>,
    text: Text,
    line: Option<Text>,
}

/// The topic in full, and on one line where it has line breaks.
pub(crate) fn text(
    state: &AppState,
    topic: &Arc<Topic>,
    shown: &mut Option<Shown>,
    requests: &mut Requests,
) -> (Option<Text>, Option<Text>) {
    let pictures = (!topic.emoji.is_empty()).then_some(state.picture_changes);
    let same = shown
        .as_ref()
        .filter(|made| Arc::ptr_eq(&made.topic, topic) && made.pictures == pictures);
    if let Some(made) = same {
        return (Some(made.text.clone()), made.line.clone());
    }
    let mentions: Vec<Mention> = topic.mentions.iter().map(rows::mention).collect();
    let emoji: Vec<CustomEmoji> = topic
        .emoji
        .iter()
        .map(|emoji| CustomEmoji {
            text: emoji.text.clone(),
            id: emoji.id.clone(),
            still: pictures::find(state, Some(&emoji.picture)),
        })
        .collect();
    requests.want(
        state,
        topic.emoji.iter().map(|emoji| emoji.picture.as_str()),
    );
    let made = |markdown: &str| {
        text::markdown(Source {
            mentions: &mentions,
            emoji: &emoji,
            cards: true,
            ..Source::new(markdown)
        })
    };
    let full = made(&topic.body);
    let line = one_line(&topic.body).map(|body| made(&body));
    *shown = Some(Shown {
        topic: topic.clone(),
        pictures,
        text: full.clone(),
        line: line.clone(),
    });
    (Some(full), line)
}

/// `body` with each line break a space, as Slint's line of the topic joins
/// its words; a byte for a byte, so the mentions stay where the core found
/// them. `None` where it has no line break.
fn one_line(body: &str) -> Option<String> {
    body.contains(['\n', '\r'])
        .then(|| body.replace(['\n', '\r'], " "))
}

#[cfg(test)]
mod tests {
    use super::one_line;

    #[test]
    fn a_topic_on_one_line_keeps_its_offsets() {
        assert_eq!(one_line("plans"), None);
        let body = "Co-op\r\nnights @Bob";
        let line = one_line(body).unwrap();
        assert_eq!(line, "Co-op  nights @Bob");
        assert_eq!(line.find("@Bob"), body.find("@Bob"));
    }
}
