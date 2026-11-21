//! A message's Markdown as Discord's client lays it out in blocks: headings,
//! subtext, quotes and fenced code apart from the text between them. The
//! window reads the inline styles in each block's text, so the blocks come
//! apart before it does.
//!
//! The rules are Discord's, not CommonMark's. A heading is `#` to `###` and a
//! space, and subtext `-# `, each on a line of its own. A quote is a run of
//! `> ` lines, or `>>> ` and the rest of the message, and holds the others but
//! no quote. Code is fenced by three backticks anywhere, even mid-line, and
//! one left open is text. Nothing inside a spoiler comes apart, so it stays
//! hidden whole.

use std::borrow::Cow;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// `#` to `###`, 1 to 3.
    Heading(u8),
    Subtext,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    /// Inside a quote, behind its bar.
    pub quoted: bool,
    /// Where its text is in the message, without its markers: one run, or one
    /// a line in a quote of `> ` lines. The runs join with line breaks.
    pub runs: Vec<Range<usize>>,
}

impl Block {
    /// Its text: a heading's or subtext's without its marker, code's without
    /// its fences and language.
    pub fn text<'a>(&self, markdown: &'a str) -> Cow<'a, str> {
        match self.runs.as_slice() {
            [] => "".into(),
            [run] => markdown[run.clone()].into(),
            runs => runs
                .iter()
                .map(|run| &markdown[run.clone()])
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        }
    }

    /// Where `at`, a byte offset in the message, is in [`Block::text`], when
    /// the `len` bytes from it are all in one of its runs.
    pub fn offset(&self, at: usize, len: usize) -> Option<usize> {
        let mut start = 0;
        for run in &self.runs {
            if run.start <= at && at + len <= run.end {
                return Some(start + at - run.start);
            }
            start += run.len() + 1;
        }
        None
    }
}

/// `markdown`'s blocks in order, or `None` when it's text alone. Text
/// between blocks keeps its blank lines, less the line break that ends it,
/// so text of nothing but a line break is an empty line.
pub fn split(markdown: &str) -> Option<Vec<Block>> {
    let maybe = markdown.contains("```")
        || markdown.split('\n').any(|line| {
            let line = line.trim_start_matches(' ');
            line.starts_with('#') || line.starts_with("-#") || line.starts_with('>')
        });
    if !maybe {
        return None;
    }
    let mut blocks = Vec::new();
    for part in scan(markdown, true) {
        match part {
            Part::Block(kind, range) => blocks.push(Block {
                kind,
                quoted: false,
                runs: vec![range],
            }),
            Part::Quote(lines) => quote(markdown, &lines, &mut blocks),
        }
    }
    match blocks.as_slice() {
        [] => None,
        [only] if only.kind == Kind::Text && !only.quoted => None,
        _ => Some(blocks),
    }
}

/// A quote's blocks, from its `lines` of `markdown`. An empty quote still
/// shows, as an empty line.
fn quote(markdown: &str, lines: &[Range<usize>], blocks: &mut Vec<Block>) {
    // Its text as one, and where each of its lines starts in it.
    let mut text = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    for line in lines {
        if !starts.is_empty() {
            text.push('\n');
        }
        starts.push(text.len());
        text.push_str(&markdown[line.clone()]);
    }
    let first = blocks.len();
    for part in scan(&text, false) {
        if let Part::Block(kind, range) = part {
            let runs = lines
                .iter()
                .zip(&starts)
                .filter_map(|(line, &start)| {
                    let end = start + line.len();
                    let from = range.start.max(start);
                    let to = range.end.min(end);
                    let inside = from < to || (line.is_empty() && range.contains(&start));
                    inside.then(|| line.start + from - start..line.start + to - start)
                })
                .collect();
            blocks.push(Block {
                kind,
                quoted: true,
                runs,
            });
        }
    }
    if blocks.len() == first {
        blocks.push(Block {
            kind: Kind::Text,
            quoted: true,
            runs: Vec::new(),
        });
    }
}

enum Part {
    Block(Kind, Range<usize>),
    /// A quote's lines, each without its `> `; `>>> ` makes one of the rest.
    Quote(Vec<Range<usize>>),
}

/// `text`'s parts, with quotes among them where `quotes` allows.
fn scan(text: &str, quotes: bool) -> Vec<Part> {
    let bytes = text.as_bytes();
    let spoilers = crate::spoiler::spans(text);
    let mut spoilers = spoilers.iter().peekable();
    let mut parts = Vec::new();
    // Where the text not yet in a part starts.
    let mut start = 0;
    let mut at = 0;
    let mut line_start = true;
    // At the start, or just after a heading, subtext or quote, where a
    // heading or subtext may follow spaces, as Discord's client sees it.
    let mut fresh = true;
    let flush = |parts: &mut Vec<Part>, start: usize, end: usize| {
        if start < end {
            let end = if bytes[end - 1] == b'\n' {
                end - 1
            } else {
                end
            };
            parts.push(Part::Block(Kind::Text, start..end));
        }
    };
    while at < text.len() {
        while spoilers.next_if(|spoiler| spoiler.end <= at).is_some() {}
        if let Some(spoiler) = spoilers.peek()
            && spoiler.start <= at
        {
            at = spoiler.end;
            line_start = false;
            fresh = false;
            continue;
        }
        if line_start {
            let found = if quotes { quoted(text, at) } else { None };
            if let Some((lines, end)) = found {
                flush(&mut parts, start, at);
                parts.push(Part::Quote(lines));
                (start, at, fresh) = (end, end, true);
                continue;
            }
            if let Some((kind, content, end)) = heading(text, at, fresh) {
                flush(&mut parts, start, at);
                parts.push(Part::Block(kind, content));
                (start, at, fresh) = (end, end, true);
                continue;
            }
        }
        match bytes[at] {
            b'\n' => {
                at += 1;
                line_start = true;
                fresh = false;
                continue;
            }
            b'\\' => {
                at += 1;
                if let Some(next) = text[at..].chars().next()
                    && !next.is_alphanumeric()
                    && !next.is_whitespace()
                {
                    at += next.len_utf8();
                }
            }
            b'`' => {
                if let Some((content, end)) = code(text, at) {
                    flush(&mut parts, start, at);
                    parts.push(Part::Block(Kind::Code, content));
                    // The line break after the fence ends the block.
                    at = if bytes.get(end) == Some(&b'\n') {
                        end + 1
                    } else {
                        end
                    };
                    line_start = at > end;
                    (start, fresh) = (at, false);
                    continue;
                }
                at = inline_code(text, at);
            }
            _ => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
        line_start = false;
        fresh = false;
    }
    flush(&mut parts, start, text.len());
    parts
}

/// The heading or subtext on the line at `at`, its content's range without
/// marker or spaces, and where the next line starts. Spaces may come before
/// the marker only when `fresh`.
fn heading(text: &str, at: usize, fresh: bool) -> Option<(Kind, Range<usize>, usize)> {
    let line_end = text[at..].find('\n').map_or(text.len(), |end| at + end);
    let next = (line_end + 1).min(text.len());
    let line = &text[at..line_end];
    let marked = if fresh {
        line.trim_start_matches(' ')
    } else {
        line
    };
    let marker_at = at + line.len() - marked.len();
    let content = |content: &str| {
        let start = content.as_ptr() as usize - text.as_ptr() as usize;
        start..start + content.len()
    };
    if let Some(after) = marked.strip_prefix("-#") {
        // `-# ` and spaces, then the subtext, which isn't more subtext.
        let rest = after.trim_start_matches(' ');
        if rest.len() == after.len() || rest.starts_with("-#") || rest.trim().is_empty() {
            return None;
        }
        return Some((Kind::Subtext, content(rest.trim()), next));
    }
    let level = marked.bytes().take_while(|&byte| byte == b'#').count();
    if !(1..=3).contains(&level) {
        return None;
    }
    let after = &text[marker_at + level..line_end];
    let rest = after.trim_start();
    if rest.len() == after.len() || rest.trim().is_empty() {
        return None;
    }
    // Not a heading marker after a heading marker.
    let hashes = rest.bytes().take_while(|&byte| byte == b'#').count();
    if (1..=3).contains(&hashes) && rest[hashes..].starts_with(char::is_whitespace) {
        return None;
    }
    // Closing `#`s go, but a heading of nothing else keeps one.
    let rest = rest.trim_end();
    let stripped = rest.trim_end_matches('#');
    let shown = if stripped.trim().is_empty() {
        &rest[..1]
    } else {
        stripped.trim_end()
    };
    Some((Kind::Heading(level as u8), content(shown), next))
}

/// The quote on the line at `at`: its lines without their `> `, and where it
/// ends, past the line break after it.
fn quoted(text: &str, at: usize) -> Option<(Vec<Range<usize>>, usize)> {
    // Where a line's quote marker ends, past the one space taken with it,
    // and whether it's `>>>`.
    let marker = |at: usize| -> Option<(usize, bool)> {
        let line = &text[at..];
        let marked = line.trim_start_matches(' ');
        let after = at + line.len() - marked.len();
        if let Some(rest) = marked.strip_prefix(">>>") {
            return rest.starts_with(' ').then_some((after + 4, true));
        }
        let rest = marked.strip_prefix('>')?;
        (!rest.starts_with(">>") && rest.starts_with(' ')).then_some((after + 2, false))
    };
    let (content, all) = marker(at)?;
    let mut lines = Vec::new();
    if all {
        lines.push(content..text.len());
        return Some((lines, text.len()));
    }
    let mut content = content;
    loop {
        let end = text[content..]
            .find('\n')
            .map_or(text.len(), |end| content + end);
        lines.push(content..end);
        let next = (end + 1).min(text.len());
        match marker(next) {
            Some((after, false)) if end < text.len() => content = after,
            _ => return Some((lines, next)),
        }
    }
}

/// Fenced code at `at`: its content's range, without the language or the
/// line breaks at either end, and where the closing fence ends.
fn code(text: &str, at: usize) -> Option<(Range<usize>, usize)> {
    let open = at + 3;
    if !text[at..].starts_with("```") {
        return None;
    }
    let language = text[open..]
        .bytes()
        .take_while(|&byte| byte.is_ascii_alphanumeric() || b"_+-.#".contains(&byte))
        .count();
    let with_language = (language > 0 && text.as_bytes().get(open + language) == Some(&b'\n'))
        .then_some(open + language + 1);
    with_language
        .into_iter()
        .chain([open])
        .find_map(|start| fenced(text, start))
}

/// Code from `start` to the next fence, at least a character of it, less
/// the line breaks at either end.
fn fenced(text: &str, start: usize) -> Option<(Range<usize>, usize)> {
    let start = start + text[start..].len() - text[start..].trim_start_matches('\n').len();
    let first = text[start..].chars().next()?;
    let close = start + first.len_utf8() + text[start + first.len_utf8()..].find("```")?;
    let end = start + text[start..close].trim_end_matches('\n').len();
    Some((start..end, close + 3))
}

/// Past inline code at `at`, or past the backticks there when they don't
/// open any.
fn inline_code(text: &str, at: usize) -> usize {
    let run = text[at..].bytes().take_while(|&byte| byte == b'`').count();
    let after = at + run;
    // At least a character of code, then as many backticks.
    let Some(first) = text[after..].chars().next() else {
        return after;
    };
    let from = after + first.len_utf8();
    match crate::spoiler::find_run(&text[from..], &text[at..after]) {
        Some(end) => from + end + run,
        None => after,
    }
}

#[cfg(test)]
mod tests;
