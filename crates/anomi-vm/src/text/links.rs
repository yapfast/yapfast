//! Web addresses in text, ported from the core's `links.rs`: finding bare
//! ones (`addresses`, private there), linking them in Markdown (`linkify`),
//! escaping Markdown's HTML (`escape_html`), and telling a link whose text
//! shows another address than where it goes (`shows`, behind `mark_masked`).

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};

/// Turns bare web addresses in Markdown into links, leaving code, existing
/// links and HTML alone.
pub fn linkify(markdown: &str) -> Cow<'_, str> {
    let urls = addresses(markdown);
    if urls.is_empty() {
        return Cow::Borrowed(markdown);
    }
    let mut skipped: Vec<Range<usize>> = Vec::new();
    for (event, range) in
        Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH).into_offset_iter()
    {
        match event {
            Event::Start(
                Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_) | Tag::HtmlBlock,
            )
            | Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_) => skipped.push(range),
            _ => {}
        }
    }
    let mut linked = String::with_capacity(markdown.len() + 16);
    let mut copied = 0;
    for url in urls {
        if skipped
            .iter()
            .any(|skip| skip.start < url.end && url.start < skip.end)
        {
            continue;
        }
        linked.push_str(&markdown[copied..url.start]);
        linked.push('<');
        linked.push_str(&markdown[url.clone()]);
        linked.push('>');
        copied = url.end;
    }
    linked.push_str(&markdown[copied..]);
    Cow::Owned(linked)
}

/// Markdown with the HTML in it escaped, so it shows as typed rather than
/// styling the text around it.
pub fn escape_html(markdown: &str) -> Cow<'_, str> {
    if !markdown.contains('<') {
        return Cow::Borrowed(markdown);
    }
    let mut html = Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH)
        .into_offset_iter()
        .filter(|(event, _)| matches!(event, Event::Html(_) | Event::InlineHtml(_)))
        .peekable();
    if html.peek().is_none() {
        return Cow::Borrowed(markdown);
    }
    let mut escaped = String::with_capacity(markdown.len() + 16);
    let mut copied = 0;
    for (_, range) in html {
        for (at, _) in markdown[range.clone()].match_indices('<') {
            let at = range.start + at;
            if at >= copied {
                escaped.push_str(&markdown[copied..at]);
                escaped.push('\\');
                copied = at;
            }
        }
    }
    escaped.push_str(&markdown[copied..]);
    Cow::Owned(escaped)
}

/// Byte ranges of `http://` and `https://` addresses, without the
/// punctuation that usually follows one in a sentence.
pub fn addresses(text: &str) -> Vec<Range<usize>> {
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = lower[from..].find("http") {
        let start = from + offset;
        let scheme = ["https://", "http://"]
            .into_iter()
            .find(|scheme| lower[start..].starts_with(scheme));
        // Inside a word, such as "xhttp://", it's not an address.
        let in_word = text[..start]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric());
        let Some(scheme) = scheme.filter(|_| !in_word) else {
            from = start + 4;
            continue;
        };
        let length = text[start..]
            .find(|c: char| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"'))
            .unwrap_or(text.len() - start);
        let mut end = start + length;
        let count = |wanted: u8, end: usize| {
            text.as_bytes()[start..end]
                .iter()
                .filter(|&&byte| byte == wanted)
                .count()
        };
        let (mut parentheses, mut brackets) = (
            (count(b'(', end), count(b')', end)),
            (count(b'[', end), count(b']', end)),
        );
        while let Some(last) = text[start..end].chars().next_back() {
            let trailing = matches!(
                last,
                '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '*' | '~' | '|'
            ) || (last == ')' && parentheses.0 < parentheses.1)
                || (last == ']' && brackets.0 < brackets.1);
            if !trailing {
                break;
            }
            match last {
                ')' => parentheses.1 -= 1,
                ']' => brackets.1 -= 1,
                _ => {}
            }
            end -= last.len_utf8();
        }
        if end > start + scheme.len() {
            found.push(start..end);
        }
        from = end.max(start + scheme.len());
    }
    found
}

/// Whether the app opens a clicked link: only web pages, never local files
/// or other programs. A name and password before the host, and invisible or
/// direction-changing characters, only serve to make a link look like it
/// goes somewhere else (`links::openable` in the core).
pub fn openable(link: &str) -> bool {
    let lower = link.to_ascii_lowercase();
    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty()
        && !authority.contains('@')
        && !link.chars().any(|c| c.is_control() || hidden(c))
}

/// Unicode's format characters that matter in a link: zero-width ones, and
/// those that change the direction text reads in.
fn hidden(c: char) -> bool {
    matches!(
        c,
        '\u{00ad}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// Whether a link's text is its address, with or without the scheme or a
/// trailing slash.
pub fn shows(text: &str, dest: &str) -> bool {
    let bare = |link: &str| {
        let link = link.trim().trim_end_matches('/');
        let lower = link.to_ascii_lowercase();
        let scheme = ["https://", "http://"]
            .into_iter()
            .find(|scheme| lower.starts_with(scheme))
            .map_or(0, str::len);
        link[scheme..].to_ascii_lowercase()
    };
    bare(text) == bare(dest)
}
