//! Discord's spoilers: text between a pair of `||`, hidden until clicked. Hidden,
//! a spoiler is a link to `yapfast:spoiler/{item}/{index}`, which the message text
//! covers; clicking it reveals that one.

use std::fmt::Write as _;
use std::ops::Range;

const LINK: &str = "yapfast:spoiler/";

/// What a quote or notification shows for a spoiler.
pub const HIDDEN: &str = "(spoiler)";

/// The address a message's spoiler at `index` links to while hidden.
pub fn link(item: &str, index: usize) -> String {
    format!("{LINK}{item}/{index}")
}

/// The message item and spoiler index a clicked link reveals.
pub fn target(link: &str) -> Option<(&str, usize)> {
    let (item, index) = link.strip_prefix(LINK)?.rsplit_once('/')?;
    Some((item, index.parse().ok()?))
}

/// Discord Markdown with its spoilers hidden as links, but those `revealed` says
/// were clicked, which show as written. They count from `first`, for a part of
/// a message after others with spoilers of their own.
pub fn mark(markdown: &str, item: &str, first: usize, revealed: impl Fn(usize) -> bool) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut last = 0;
    for (index, span) in (first..).zip(spans(markdown)) {
        out.push_str(&markdown[last..span.start]);
        let inner = &markdown[span.start + 2..span.end - 2];
        if revealed(index) {
            out.push_str(inner);
        } else {
            let _ = write!(out, "[{}](<{}>)", escape(&plain(inner)), link(item, index));
        }
        last = span.end;
    }
    out.push_str(&markdown[last..]);
    out
}

/// Discord Markdown with each spoiler replaced by [`HIDDEN`], for quotes and
/// notifications.
pub fn hide(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut last = 0;
    for span in spans(markdown) {
        out.push_str(&markdown[last..span.start]);
        out.push_str(HIDDEN);
        last = span.end;
    }
    out.push_str(&markdown[last..]);
    out
}

/// Whether `markdown` has a spoiler.
pub fn any(markdown: &str) -> bool {
    !spans(markdown).is_empty()
}

/// How many spoilers `markdown` has.
pub fn count(markdown: &str) -> usize {
    spans(markdown).len()
}

/// The byte ranges of the spoilers, each with its `||` pair. Pairs inside code
/// don't count, and neither does an empty pair or one across a paragraph break.
pub(crate) fn spans(markdown: &str) -> Vec<Range<usize>> {
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
pub(crate) fn find_run(text: &str, fence: &str) -> Option<usize> {
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

/// Markdown's text, without its formatting, links or HTML.
fn plain(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser};
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

/// `text` with Markdown's punctuation escaped, so it shows as typed.
fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escape = c.is_ascii_punctuation().then_some('\\');
            escape.into_iter().chain([c])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_spoilers_as_links_until_revealed() {
        let body = "The end: ||Bruce **dies**|| and ||<font color=\"#7cc4ff\">[@Ann](<yapfast:user/2>)</font> lives||.";
        assert_eq!(
            mark(body, "d:5", 0, |_| false),
            "The end: [Bruce dies](<yapfast:spoiler/d:5/0>) and [\\@Ann lives](<yapfast:spoiler/d:5/1>)."
        );
        assert_eq!(
            mark(body, "d:5", 0, |index| index == 0),
            "The end: Bruce **dies** and [\\@Ann lives](<yapfast:spoiler/d:5/1>)."
        );
        assert_eq!(target("yapfast:spoiler/d:5/1"), Some(("d:5", 1)));
        assert_eq!(target("yapfast:user/2"), None);
        assert_eq!(hide("Spoiler: ||it was him||!"), "Spoiler: (spoiler)!");
    }

    #[test]
    fn leaves_code_empty_pairs_and_lone_bars_alone() {
        for text in [
            "`a || b || c`",
            "```\n||x||\n```",
            "a |||| b",
            "|| a\n\nb ||",
            "one || alone",
            r"\||not\||",
        ] {
            assert!(!any(text), "{text}");
            assert_eq!(mark(text, "d:1", 0, |_| false), text);
        }
        assert_eq!(
            mark("`||` ||x|| ``a`b``", "d:1", 0, |_| false),
            "`||` [x](<yapfast:spoiler/d:1/0>) ``a`b``"
        );
    }
}
