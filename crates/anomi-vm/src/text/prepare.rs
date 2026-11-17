//! Markdown made ready for the inline parser, as `src/ui/messages.rs` made
//! it ready for Slint's: HTML escaped, mentions and hidden spoilers made
//! links to markers ([`Markers`]) the parser reads back, lines that would be
//! rules escaped, blank lines kept, bare web addresses linked. Ports
//! `mention::linked` and `spoiler::mark` of the core, and `no_rules` and
//! `keep_blank_lines` of `src/ui/messages.rs`.

use std::borrow::Cow;

use super::links::escape_html;
use super::source::{Mention, Spoilers};
use super::{Link, spoiler};

/// Link destinations only this parse writes: a prefix the Markdown doesn't
/// hold, so no one can type a mention's pill or a spoiler's cover. Markdown
/// unescapes a destination (`anomi0\:m0`, `anomi0&#58;m0`), so a marker
/// counts only as written, the tag itself between `(<` and `>)`.
pub struct Markers {
    tag: String,
}

/// What a marker stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    /// The source's mention at this index.
    Mention(usize),
    /// The message's spoiler at this index, hidden.
    Spoiler(usize),
}

impl Markers {
    pub fn for_markdown(markdown: &str) -> Self {
        let tag = (0u32..)
            .map(|n| format!("anomi{n}:"))
            .find(|tag| !markdown.contains(tag.as_str()))
            .unwrap_or_default();
        Self { tag }
    }

    fn write(&self, kind: char, index: usize) -> String {
        format!("{}{kind}{index}", self.tag)
    }

    /// What a link stands for, when it's a marker: `dest` is its destination
    /// as parsed, `source` its Markdown, `[text](<dest>)` when this wrote it.
    pub fn read(&self, dest: &str, source: &str) -> Option<Marker> {
        let written = source
            .strip_suffix(">)")
            .and_then(|before| before.strip_suffix(dest))
            .is_some_and(|before| before.ends_with("(<"));
        if !written {
            return None;
        }
        let rest = dest.strip_prefix(self.tag.as_str())?;
        let kind = rest.chars().next()?;
        let index = rest[kind.len_utf8()..].parse().ok()?;
        match kind {
            'm' => Some(Marker::Mention(index)),
            's' => Some(Marker::Spoiler(index)),
            _ => None,
        }
    }
}

/// `markdown` with its HTML escaped and each mention at `found` (offsets
/// into `markdown`, each with its index in `all`) outside code made a link
/// to its marker. A mention is only where the message's tag for it was:
/// `@Bobby` typed as text isn't Bob's.
pub fn mentions<'a>(
    markdown: &'a str,
    mut found: Vec<(usize, usize)>,
    all: &[Mention],
    markers: &Markers,
) -> Cow<'a, str> {
    let escaped = escape_html(markdown);
    found.retain(|&(_, index)| {
        all.get(index)
            .is_some_and(|mention| !mention.text.is_empty())
    });
    if found.is_empty() {
        return escaped;
    }
    found.sort_by_key(|&(at, _)| at);
    shift(markdown, &escaped, &mut found);
    let mut out = String::with_capacity(escaped.len() + 24 * found.len());
    let mut code = false;
    let mut last = 0;
    for (at, index) in found {
        let mention = &all[index];
        if at < last
            || !escaped
                .get(at..)
                .is_some_and(|rest| rest.starts_with(&mention.text))
        {
            continue;
        }
        let before = &escaped[last..at];
        code ^= before.matches('`').count() % 2 == 1;
        out.push_str(before);
        last = at;
        if code {
            continue;
        }
        // A name shows as typed: one such as `[x](https://…)` would otherwise
        // be a link of its own. A channel's comes escaped already.
        let text = match mention.target {
            Link::Channel(_) => Cow::Borrowed(mention.text.as_str()),
            _ => Cow::Owned(spoiler::escape(&mention.text)),
        };
        out.push('[');
        out.push_str(&text);
        out.push_str("](<");
        out.push_str(&markers.write('m', index));
        out.push_str(">)");
        last = at + mention.text.len();
    }
    out.push_str(&escaped[last..]);
    Cow::Owned(out)
}

/// Moves each offset into `original` to the same place in `escaped`, which
/// is `original` with backslashes put in before some of its `<`. The
/// offsets are in order.
fn shift(original: &str, escaped: &str, offsets: &mut [(usize, usize)]) {
    if original.len() == escaped.len() {
        return;
    }
    let (original, escaped) = (original.as_bytes(), escaped.as_bytes());
    let (mut from, mut to) = (0, 0);
    for (at, _) in offsets {
        while from < *at && from < original.len() {
            if escaped[to] == original[from] {
                from += 1;
            }
            to += 1;
        }
        *at = to;
    }
}

/// `markdown` with its spoilers as `spoilers` says, counted from `first`;
/// and how many it has.
pub fn mark_spoilers<'a>(
    markdown: Cow<'a, str>,
    spoilers: Spoilers<'_>,
    first: usize,
    markers: &Markers,
) -> (Cow<'a, str>, usize) {
    if matches!(spoilers, Spoilers::AsTyped) || !markdown.contains("||") {
        return (markdown, 0);
    }
    let found = spoiler::spans(&markdown);
    if found.is_empty() {
        return (markdown, 0);
    }
    let mut out = String::with_capacity(markdown.len() + 24 * found.len());
    let mut last = 0;
    for (index, span) in (first..).zip(&found) {
        out.push_str(&markdown[last..span.start]);
        let inner = &markdown[span.start + 2..span.end - 2];
        match spoilers {
            Spoilers::Covered(revealed) if revealed(index) => out.push_str(inner),
            Spoilers::Covered(_) => {
                out.push('[');
                out.push_str(&spoiler::escape(&spoiler::plain(inner)));
                out.push_str("](<");
                out.push_str(&markers.write('s', index));
                out.push_str(">)");
            }
            Spoilers::Hidden | Spoilers::AsTyped => out.push_str(spoiler::HIDDEN),
        }
        last = span.end;
    }
    out.push_str(&markdown[last..]);
    (Cow::Owned(out), found.len())
}

/// `text` with its lines of nothing but `-`, `=`, `*` or `_` escaped, so they
/// show as typed, as in Discord's client: Markdown would make them rules, or
/// headings of the line above.
pub fn no_rules(text: &str) -> Cow<'_, str> {
    let rule = |line: &str| {
        let marks = line.trim();
        let indent = line.len() - line.trim_start().len();
        indent < 4
            && marks.chars().next().is_some_and(|first| {
                "-=*_".contains(first) && marks.chars().all(|c| c == first || c == ' ')
            })
    };
    if !text.split('\n').any(rule) {
        return text.into();
    }
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| {
            if rule(line) {
                let indent = line.len() - line.trim_start().len();
                format!("{}\\{}", &line[..indent], &line[indent..])
            } else {
                line.to_owned()
            }
        })
        .collect();
    lines.join("\n").into()
}

/// `text` with each blank line between its lines holding a no-break space.
/// Discord shows every line break as typed, where Markdown runs blank lines
/// together into one paragraph break; a line with a no-break space isn't
/// blank to Markdown, so it shows as an empty line. Fenced code keeps its
/// lines as they are.
pub fn keep_blank_lines(text: &str) -> Cow<'_, str> {
    let blank = |line: &str| line.trim().is_empty();
    let lines: Vec<&str> = text.split('\n').collect();
    let (Some(first), Some(last)) = (
        lines.iter().position(|line| !blank(line)),
        lines.iter().rposition(|line| !blank(line)),
    ) else {
        return text.into();
    };
    if !lines[first..last].iter().any(|line| blank(line)) {
        return text.into();
    }
    let mut fenced = false;
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
            }
            if !fenced && index > first && index < last && blank(line) {
                "\u{a0}"
            } else {
                line
            }
        })
        .collect();
    kept.join("\n").into()
}

/// `text` with every blank line holding a no-break space: a text block of
/// Discord's shows every line, blank or not.
pub fn every_line(text: &str) -> Cow<'_, str> {
    if !text.split('\n').any(|line| line.trim().is_empty()) {
        return text.into();
    }
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                "\u{a0}"
            } else {
                line
            }
        })
        .collect();
    lines.join("\n").into()
}
