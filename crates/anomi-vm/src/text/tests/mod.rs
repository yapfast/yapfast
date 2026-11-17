//! The parser against what Slint's styled text showed of the same Markdown,
//! read back through [`shown`], which writes a text's spans as a terse
//! Markdown of their own.

mod blocks;
mod inline;
mod prepare;

use std::fmt::Write as _;

use super::*;

/// `text` as its spans read: `**bold**`, `*italic*`, `__underline__`,
/// `~~strike~~`, `` `code` ``, `[text](link)`, `@[mention]`, `#[channel]`,
/// `||spoiler||`, `{emoji}`, `{:picture:}`; blocks on lines of their own,
/// each after its level and quote, as `H1>` or `T`.
fn shown(text: &Text) -> String {
    let mut out = String::new();
    for (index, (block, spans)) in text.blocks().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if text.blocks().len() > 1 {
            let level = match block.level {
                Level::Text => "T".to_owned(),
                Level::Heading(level) => format!("H{level}"),
                Level::Subtext => "S".to_owned(),
                Level::Code => "C".to_owned(),
            };
            let quote = if block.quoted { ">" } else { "" };
            let _ = write!(out, "{level}{quote}|");
        }
        for span in spans {
            out.push_str(&span_shown(text, span));
        }
    }
    out
}

fn span_shown(text: &Text, span: &Span) -> String {
    let chars = text.chars(span);
    let mut shown = match span.look {
        Look::Text => chars.to_owned(),
        Look::Mention => format!("@[{chars}]"),
        Look::Channel => format!("#[{chars}]"),
        Look::Spoiler => format!("||{chars}||"),
        Look::Emoji => format!("{{{chars}}}"),
        Look::Picture(_) => format!("{{{chars}}}"),
    };
    let style = span.style;
    for (on, mark) in [
        (style.code, "`"),
        (style.bold, "**"),
        (style.italic, "*"),
        (style.underline, "__"),
        (style.strike, "~~"),
    ] {
        if on {
            shown = format!("{mark}{shown}{mark}");
        }
    }
    if span.look == Look::Text
        && let Some(link) = span.link.and_then(|link| text.link(link))
    {
        shown = format!("[{shown}]({link:?})");
    }
    shown
}

/// A message's Markdown as text alone, emoji opening their cards.
fn message(markdown: &str) -> Text {
    super::markdown(Source {
        cards: true,
        ..Source::new(markdown)
    })
}
