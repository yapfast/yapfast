//! Web addresses in message text become links the reader can click.
use std::borrow::Cow;
use std::fmt::Write as _;
use std::ops::Range;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

/// What a masked link leads to instead of its address: a click asks before
/// opening the address that follows.
pub const LEAVE: &str = "yapfast:leave/";

/// Turns bare web addresses in Markdown into links, leaving code, existing
/// links and HTML alone.
pub fn linkify(markdown: &str) -> String {
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
    for url in addresses(markdown) {
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
    linked
}

/// Markdown with the HTML in it escaped, so it shows as typed rather than
/// styling the text around it.
pub fn escape_html(markdown: &str) -> Cow<'_, str> {
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

/// Markdown whose links show other text than where they go, such as
/// `[discord.com](https://evil.example)`, leading to [`LEAVE`] and the
/// address instead, so a click asks before going there.
pub fn mark_masked(markdown: &str) -> Cow<'_, str> {
    struct Link {
        start: usize,
        // Where the text between the brackets ends.
        text_end: usize,
        text: String,
        dest: String,
    }
    let mut marked = String::new();
    let mut copied = 0;
    let mut link: Option<Link> = None;
    for (event, range) in
        Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH).into_offset_iter()
    {
        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) if !matches!(link_type, LinkType::Autolink | LinkType::Email)
                && openable(&dest_url) =>
            {
                link = Some(Link {
                    start: range.start,
                    text_end: range.start + 1,
                    text: String::new(),
                    dest: dest_url.into_string(),
                });
            }
            Event::End(TagEnd::Link) => {
                let Some(link) = link.take() else { continue };
                if shows(&link.text, &link.dest) || !markdown[link.start..].starts_with('[') {
                    continue;
                }
                marked.push_str(&markdown[copied..link.start]);
                marked.push_str(&markdown[link.start..link.text_end]);
                marked.push_str("](<");
                marked.push_str(LEAVE);
                for character in link.dest.chars() {
                    match character {
                        '<' | '>' | '\\' | ' ' => {
                            let _ = write!(marked, "%{:02X}", character as u32);
                        }
                        character => marked.push(character),
                    }
                }
                marked.push_str(">)");
                copied = range.end;
            }
            event => {
                if let Some(link) = link.as_mut() {
                    if let Event::Text(text) | Event::Code(text) = event {
                        link.text.push_str(&text);
                    }
                    link.text_end = link.text_end.max(range.end);
                }
            }
        }
    }
    if copied == 0 {
        return Cow::Borrowed(markdown);
    }
    marked.push_str(&markdown[copied..]);
    Cow::Owned(marked)
}

/// Whether a link's text is its address, with or without the scheme or a
/// trailing slash.
fn shows(text: &str, dest: &str) -> bool {
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

/// The address a masked link from [`mark_masked`] goes to, when it's one
/// Yapfast opens.
pub fn masked(link: &str) -> Option<&str> {
    link.strip_prefix(LEAVE).filter(|link| openable(link))
}

/// A web address's host, as `example.com` in `https://user@example.com:8/a`.
pub fn host(link: &str) -> &str {
    let rest = link.split_once("://").map_or(link, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // Without its port; an IPv6 address keeps its brackets and colons.
    match host.rsplit_once(':') {
        Some((name, port))
            if port.bytes().all(|byte| byte.is_ascii_digit())
                && (!name.starts_with('[') || name.ends_with(']')) =>
        {
            name
        }
        _ => host,
    }
}

/// Plain text as Markdown with its web addresses as links, or `None`
/// without any.
pub fn plain_with_links(text: &str) -> Option<String> {
    let urls = addresses(text);
    if urls.is_empty() {
        return None;
    }
    let mut linked = String::with_capacity(text.len() + 16);
    let mut copied = 0;
    for url in urls {
        escape(&text[copied..url.start], &mut linked);
        linked.push('<');
        linked.push_str(&text[url.clone()]);
        linked.push('>');
        copied = url.end;
    }
    escape(&text[copied..], &mut linked);
    Some(linked)
}

/// Whether Yapfast opens a clicked link: only web pages, never local files or
/// other programs. A name and password before the host, as in
/// `https://discord.com@evil.example`, and invisible or direction-changing
/// characters only serve to make a link look like it goes somewhere else.
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

/// Unicode's format characters that matter in a link: zero-width ones,
/// and those that change the direction text reads in.
pub fn hidden(c: char) -> bool {
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

/// `text` with Markdown's punctuation escaped by backslashes, so it shows
/// as typed.
pub fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    escape(text, &mut escaped);
    escaped
}

fn escape(text: &str, out: &mut String) {
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(character);
    }
}

/// `text` with the tracking parameters taken out of its web addresses:
/// those that only tell a site or an advertiser who shared a link and
/// where it was clicked. The rest of each address stays as typed.
pub fn without_tracking(text: &str) -> Cow<'_, str> {
    let mut clean = String::new();
    let mut copied = 0;
    for range in addresses(text) {
        let Some(link) = untracked(&text[range.clone()]) else {
            continue;
        };
        clean.push_str(&text[copied..range.start]);
        clean.push_str(&link);
        copied = range.end;
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    clean.push_str(&text[copied..]);
    Cow::Owned(clean)
}

/// `link` without its tracking parameters, or `None` when it has none.
fn untracked(link: &str) -> Option<String> {
    let (address, fragment) = match link.split_once('#') {
        Some((address, fragment)) => (address, Some(fragment)),
        None => (link, None),
    };
    let (base, query) = address.split_once('?')?;
    let host = host(link).to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let parameters = query.split('&');
    let kept: Vec<&str> = parameters
        .clone()
        .filter(|parameter| {
            let name = parameter.split('=').next().unwrap_or_default();
            !tracks(host, &name.to_ascii_lowercase())
        })
        .collect();
    if kept.len() == parameters.count() {
        return None;
    }
    let mut clean = base.to_owned();
    if !kept.is_empty() {
        clean.push('?');
        clean.push_str(&kept.join("&"));
    }
    if let Some(fragment) = fragment {
        clean.push('#');
        clean.push_str(fragment);
    }
    Some(clean)
}

/// Whether a query parameter named `name` on `host` only tracks: the
/// campaign tags any site gets, the click ids ad networks add, and the
/// share ids a few big sites add to links copied from them.
fn tracks(host: &str, name: &str) -> bool {
    const ANYWHERE: &[&str] = &[
        "fbclid",
        "gclid",
        "dclid",
        "gbraid",
        "wbraid",
        "msclkid",
        "yclid",
        "twclid",
        "ttclid",
        "li_fat_id",
        "mc_cid",
        "mc_eid",
        "_hsenc",
        "_hsmi",
        "mkt_tok",
        "igshid",
    ];
    let shared = match host {
        "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "open.spotify.com" => {
            &["si"][..]
        }
        "instagram.com" => &["igsh"][..],
        "twitter.com" | "x.com" => &["s", "t", "ref_src"][..],
        "reddit.com" | "old.reddit.com" => &["share_id", "utm_name"][..],
        _ => &[][..],
    };
    name.starts_with("utm_") || ANYWHERE.contains(&name) || shared.contains(&name)
}

/// Byte ranges of `http://` and `https://` addresses, without the
/// punctuation that usually follows one in a sentence.
fn addresses(text: &str) -> Vec<Range<usize>> {
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
        // Counted once and kept as the end moves back: counting again for
        // each character let a run of brackets take seconds.
        let count = |wanted: u8| {
            text.as_bytes()[start..end]
                .iter()
                .filter(|&&byte| byte == wanted)
                .count()
        };
        let (mut parentheses, mut brackets) =
            ((count(b'('), count(b')')), (count(b'['), count(b']')));
        while let Some(last) = text[start..end].chars().next_back() {
            if matches!(
                last,
                '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '*' | '~' | '|'
            ) || (last == ')' && parentheses.0 < parentheses.1)
                || (last == ']' && brackets.0 < brackets.1)
            {
                match last {
                    ')' => parentheses.1 -= 1,
                    ']' => brackets.1 -= 1,
                    _ => {}
                }
                end -= last.len_utf8();
            } else {
                break;
            }
        }
        if end > start + scheme.len() {
            found.push(start..end);
        }
        from = end.max(start + scheme.len());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_tracking_out_of_links_and_leaves_the_rest() {
        assert_eq!(
            without_tracking(
                "look https://example.com/a?id=4&utm_source=x&UTM_medium=y#top and https://youtu.be/abc?si=Q1&t=30."
            ),
            "look https://example.com/a?id=4#top and https://youtu.be/abc?t=30."
        );
        assert_eq!(
            without_tracking("(https://x.com/a/status/1?s=20&t=abc)"),
            "(https://x.com/a/status/1)"
        );
        // Another site's `si` or `t` may mean something.
        let kept = "https://example.org/?si=1&t=2 and https://example.org/plain";
        assert!(matches!(without_tracking(kept), Cow::Borrowed(text) if text == kept));
    }

    #[test]
    fn links_bare_addresses_only() {
        assert_eq!(
            linkify("see https://example.com/a_b?c=1. and (http://x.org/wiki/A_(b))!"),
            "see <https://example.com/a_b?c=1>. and (<http://x.org/wiki/A_(b)>)!"
        );
        assert_eq!(
            linkify("**https://bold.example** `https://code.example` [site](https://a.example)"),
            "**<https://bold.example>** `https://code.example` [site](https://a.example)"
        );
        assert_eq!(
            linkify("<https://already.example>"),
            "<https://already.example>"
        );
        assert_eq!(
            linkify("https:// xhttps://no.example"),
            "https:// xhttps://no.example"
        );
        assert_eq!(
            plain_with_links("a*b https://e.example/x_y"),
            Some("a\\*b <https://e.example/x_y>".into())
        );
        assert_eq!(plain_with_links("no links"), None);
    }

    #[test]
    fn marks_links_that_show_another_address() {
        assert_eq!(
            mark_masked("see [**discord.com**](<https://evil.example/a b>) now"),
            "see [**discord.com**](<yapfast:leave/https://evil.example/a%20b>) now"
        );
        for kept in [
            "<https://a.example>",
            "[https://a.example/](https://a.example)",
            "[a.example](https://A.example/)",
            "[@Bob](<yapfast:user/2>)",
            "[x](file:///etc/passwd)",
            "no links",
        ] {
            assert!(matches!(mark_masked(kept), Cow::Borrowed(_)), "{kept}");
        }
        assert_eq!(
            masked("yapfast:leave/https://evil.example/a%20b"),
            Some("https://evil.example/a%20b")
        );
        assert_eq!(masked("yapfast:leave/javascript:alert(1)"), None);
        assert_eq!(host("https://evil.example:8080/x?y#z"), "evil.example");
        assert_eq!(host("https://[::1]:8/x"), "[::1]");
    }

    #[test]
    fn escapes_html_only() {
        assert_eq!(
            escape_html("<font color=\"#5865f2\">@Admin</font> <u>x</u>"),
            "\\<font color=\"#5865f2\">@Admin\\</font> \\<u>x\\</u>"
        );
        assert!(matches!(
            escape_html("<https://a.example> a < b <3 `<u>` **b**"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn opens_only_web_pages() {
        assert!(openable("https://example.com"));
        assert!(openable("HTTP://example.com"));
        assert!(!openable("file:///etc/passwd"));
        assert!(!openable("javascript:alert(1)"));
        assert!(!openable("https://"));
        assert!(!openable("https://discord.com@evil.example/"));
        assert!(!openable("https://evil.example/\u{202e}moc.drocsid"));
        assert!(!openable("https://disc\u{200b}ord.com"));
        assert!(openable("https://example.com/a@b?c=@d#@e"));
    }

    #[test]
    fn trims_long_bracket_runs_quickly() {
        let text = format!("http://a{}", ")".repeat(100_000));
        let started = std::time::Instant::now();
        assert_eq!(plain_with_links(&text).unwrap().len(), text.len() * 2 - 6);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
