//! What a message says: someone's Markdown with its mentions, custom emoji
//! and spoilers, in the blocks Discord's client lays out (`message_row`'s
//! `styled` and `body_blocks`, `src/ui/messages.rs:495`-`:524`, `:1105`);
//! text that isn't Markdown as typed, its addresses linked; a notice as
//! Discord words it (`notice_text`, `:326`); a blocked person's as a note
//! that shows it (`:885`).

use yapfast_core::blocks;
use yapfast_core::chat::{ChatMessage, NoticeKind};
use yapfast_core::discord;
use yapfast_lang::tr;

use anomi_vm::text::{
    self, Builder, CustomEmoji, Ink, Kind, Link, Mention, Section, Source, Spoilers, Style, Text,
};

use super::Context;
use crate::pictures;
use crate::project::rows;

/// Where a link to show a blocked person's message points; its item follows.
pub(crate) const BLOCKED_LINK: &str = "yapfast:blocked/";

pub(super) fn body(cx: &Context<'_>, message: &ChatMessage, blocked: bool, time: &str) -> Text {
    if blocked {
        return blocked_note(&message.item);
    }
    if let Some(notice) = &message.notice {
        return notice_text(message, &notice.kind, &notice.text, time);
    }
    if !message.formatted {
        return text::plain(&message.body);
    }
    markdown(cx, message, true)
}

/// Someone's Markdown with what the core knows of it; spoilers covered but
/// those revealed, as the timeline shows them.
fn markdown(cx: &Context<'_>, message: &ChatMessage, cards: bool) -> Text {
    let mentions: Vec<Mention> = message.mentions.iter().map(rows::mention).collect();
    let emoji: Vec<CustomEmoji> = message
        .emoji
        .iter()
        .map(|emoji| CustomEmoji {
            text: emoji.text.clone(),
            id: emoji.id.clone(),
            still: pictures::find(cx.state, Some(&emoji.picture)),
        })
        .collect();
    let sections = sections(&message.body);
    let revealed = |index: usize| cx.revealed.contains(&format!("{}/{index}", message.item));
    let spoilers = if message.spoilers {
        Spoilers::Covered(&revealed)
    } else {
        Spoilers::AsTyped
    };
    text::markdown(Source {
        markdown: &message.body,
        mentions: &mentions,
        emoji: &emoji,
        spoilers,
        sections: sections.as_deref(),
        cards,
    })
}

/// The blocks the core splits `markdown` into, as the text module reads
/// them; none for text alone.
pub(crate) fn sections(markdown: &str) -> Option<Vec<Section>> {
    let split = blocks::split(markdown)?;
    Some(
        split
            .into_iter()
            .map(|block| Section {
                kind: match block.kind {
                    blocks::Kind::Text => Kind::Text,
                    blocks::Kind::Heading(level) => Kind::Heading(level),
                    blocks::Kind::Subtext => Kind::Subtext,
                    blocks::Kind::Code => Kind::Code,
                },
                quoted: block.quoted,
                runs: block.runs,
            })
            .collect(),
    )
}

/// Discord shows nothing of a blocked message until asked: "*Blocked
/// message.* Show it", the link showing it.
fn blocked_note(item: &str) -> Text {
    let mut builder = Builder::new();
    let italic = Style {
        italic: true,
        ..Style::default()
    };
    builder.text(tr!("Blocked message."), italic);
    builder.text(" ", Style::default());
    builder.link(
        tr!("Show it"),
        Style::default(),
        Link::Url(format!("{BLOCKED_LINK}{item}")),
        None,
    );
    builder.finish()
}

/// A notice as Discord's client words it: its author's name in bold, in
/// their role colour, opening their card; what it tells of, a pin's with
/// links to the message and to the pinned messages; then when, muted.
fn notice_text(message: &ChatMessage, kind: &NoticeKind, said: &str, time: &str) -> Text {
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let mut builder = Builder::new();
    let sender = discord::sender_user(&message.sender).unwrap_or(&message.sender);
    let color = message
        .color
        .map_or(Ink::Text, |rgb| Ink::Own(rows::rgb(rgb)));
    if sender.is_empty() {
        builder.colored(&message.author, bold, color);
    } else {
        builder.link(
            &message.author,
            bold,
            Link::Author(sender.to_owned()),
            Some(color),
        );
    }
    match kind {
        NoticeKind::Pinned(item) => {
            let sentence = tr!(" pinned {} to this channel. See all {}.");
            builder.template(sentence, Style::default(), |builder, place| {
                let (words, link) = match place {
                    0 => (tr!("a message"), item.clone().map(Link::Jump)),
                    _ => (tr!("pinned messages"), Some(Link::Pins)),
                };
                match link {
                    Some(link) => builder.link(words, bold, link, Some(Ink::Text)),
                    None => builder.colored(words, bold, Ink::Text),
                };
            });
        }
        // A possessive follows the name without a space.
        _ if said.starts_with('\'') => {
            builder.text(said, Style::default());
        }
        _ => {
            builder.text(" ", Style::default());
            builder.text(said, Style::default());
        }
    }
    builder.colored(&format!("  {time}"), Style::default(), Ink::Muted);
    builder.finish()
}
