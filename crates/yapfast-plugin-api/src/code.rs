//! Coloured spans for one code block on screen.
//!
//! §17.2 of the plan asks for one answer per block, cached by the block's own
//! content hash, and only for the blocks the person can see: a channel of
//! fifty code blocks is fifty answers the first time and none afterwards, and
//! two messages quoting the same snippet are one answer.
//!
//! The plugin gives back the text it was handed, cut into spans with a tone
//! each. It cannot rewrite the block: a span set that doesn't spell the block
//! exactly is refused, so a plugin can colour a code block and never change
//! what it says.

use serde::{Deserialize, Serialize};

/// The most spans one block may be cut into.
pub const MAX_SPANS: usize = 512;

/// The longest block a plugin is asked about: a message's own cap, and the
/// most a row can draw before it scrolls.
pub const MAX_BLOCK: usize = 8192;

/// What a span is for, as a closed set. A tone and not a colour, so the
/// interface's theme decides how it looks, the way a widget cell's does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Plain,
    Comment,
    Keyword,
    String,
    Number,
    Function,
    Type,
    Variable,
    Operator,
    Punctuation,
    Error,
}

/// One run of a block, in one tone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default = "plain")]
    pub tone: Tone,
}

fn plain() -> Tone {
    Tone::Plain
}

impl Span {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Span {
            text: text.into(),
            tone,
        }
    }
}

/// What `yf_code_block` is given: one block, named by its content hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBlockRequest {
    /// The block, by the content hash the parent files the answer under.
    pub block: String,
    /// Its language, as the fence named it, when the fence named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// What it says. Never longer than [`MAX_BLOCK`]; a longer block isn't
    /// asked about at all.
    pub text: String,
}

/// Whether `spans` spells `text`, and is a shape the interface can draw.
///
/// It is the whole check, and it is deliberately strict: the spans must add up
/// to the block character for character, there must be no more than
/// [`MAX_SPANS`] of them, and none may be empty. A plugin colours a block; it
/// never gets to change it.
pub fn fits(text: &str, spans: &[Span]) -> bool {
    if spans.is_empty() || spans.len() > MAX_SPANS {
        return false;
    }
    if spans.iter().any(|span| span.text.is_empty()) {
        return false;
    }
    let length: usize = spans.iter().map(|span| span.text.len()).sum();
    if length != text.len() {
        return false;
    }
    let mut joined = String::with_capacity(length);
    for span in spans {
        joined.push_str(&span.text);
    }
    joined == text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_set_must_spell_the_block() {
        let text = "let x = 1;";
        let good = [
            Span::new("let", Tone::Keyword),
            Span::new(" x = ", Tone::Plain),
            Span::new("1", Tone::Number),
            Span::new(";", Tone::Punctuation),
        ];
        assert!(fits(text, &good));

        // The same length, different text: refused.
        let wrong = [Span::new("let y = 1;", Tone::Plain)];
        assert!(!fits(text, &wrong));
        // A span that spells the block plus a word: refused, and not cut.
        let long = [
            Span::new("let x = 1;", Tone::Plain),
            Span::new("// hi", Tone::Comment),
        ];
        assert!(!fits(text, &long));
        // Nothing, or an empty span, is nothing to draw.
        assert!(!fits(text, &[]));
        assert!(!fits(text, &[Span::new("", Tone::Plain)]));
    }

    #[test]
    fn a_span_carries_a_tone_and_defaults_to_plain() {
        let span: Span = serde_json::from_str(r#"{"text":"hi"}"#).unwrap();
        assert_eq!(span.tone, Tone::Plain);
        let json = serde_json::to_string(&Span::new("hi", Tone::Keyword)).unwrap();
        assert_eq!(json, r#"{"text":"hi","tone":"keyword"}"#);
    }
}
