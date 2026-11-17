//! Discord's spoilers, `||like this||`, found in Markdown: ported from the
//! core's `spoiler.rs` (`spans`, `find_run`, `plain`, `escape`), which keeps
//! them to itself.

use std::ops::Range;

use pulldown_cmark::{Event, Parser};

/// What a pinned or found message shows for a spoiler.
pub const HIDDEN: &str = "(spoiler)";

/// The byte ranges of the spoilers, each with its `||` pair. Pairs inside
/// code don't count, and neither does an empty pair or one across a
/// paragraph break.
pub fn spans(markdown: &str) -> Vec<Range<usize>> {
    let bytes = markdown.as_bytes();
    let mut bars = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'`' => {
                let run = bytes[index..].iter().take_while(|&&b| b == b'`').count();
                let fence = &markdown[index..index + run];
                // Code runs to the next run of as many backticks; one left open is text.
                let after = index + run;
                match find_run(&markdown[after..], fence) {
                    Some(end) => index = after + end + run,
                    None => index = after,
                }
            }
            b'\\' => index += 2,
            b'|' if bytes.get(index + 1) == Some(&b'|') => {
                bars.push(index);
                index += 2;
            }
            _ => index += 1,
        }
    }
    let mut spans = Vec::new();
    let mut open: Option<usize> = None;
    for bar in bars {
        match open {
            None => open = Some(bar),
            Some(start) => {
                let inner = &markdown[start + 2..bar];
                if inner.trim().is_empty() || inner.contains("\n\n") {
                    open = Some(bar);
                } else {
                    spans.push(start..bar + 2);
                    open = None;
                }
            }
        }
    }
    spans
}

/// Where a run of exactly `fence` backticks starts in `text`.
pub fn find_run(text: &str, fence: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(found) = text[index..].find(fence) {
        let start = index + found;
        let end = start + fence.len();
        let longer = (start > 0 && bytes[start - 1] == b'`') || bytes.get(end) == Some(&b'`');
        if !longer {
            return Some(start);
        }
        index = end;
        while bytes.get(index) == Some(&b'`') {
            index += 1;
        }
    }
    None
}

/// Markdown's text, without its formatting, links or HTML: what a hidden
/// spoiler's cover covers.
pub fn plain(markdown: &str) -> String {
    let mut text = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Text(part) | Event::Code(part) => text.push_str(&part),
            Event::SoftBreak | Event::HardBreak => text.push(' '),
            _ => {}
        }
    }
    if text.trim().is_empty() {
        "spoiler".into()
    } else {
        text
    }
}

/// `text` with Markdown's punctuation escaped by backslashes, so it shows
/// as typed.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
