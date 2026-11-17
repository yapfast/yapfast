//! The passes before parsing, as the core and `src/ui/messages.rs` test them.

use super::super::links::{addresses, linkify, openable, shows};
use super::super::prepare::{keep_blank_lines, no_rules};
use super::super::spoiler::{plain, spans};

#[test]
fn finds_addresses_without_the_punctuation_after_them() {
    let text = "see (https://example.com/a_(b)), https://x.y/z?q=1. and xhttp://no";
    let found: Vec<&str> = addresses(text)
        .into_iter()
        .map(|range| &text[range])
        .collect();
    assert_eq!(found, ["https://example.com/a_(b)", "https://x.y/z?q=1"]);
    assert_eq!(linkify("a https://e.com b"), "a <https://e.com> b");
    assert_eq!(
        linkify("[t](https://e.com) `https://c.com`"),
        "[t](https://e.com) `https://c.com`"
    );
}

#[test]
fn opens_only_web_pages_that_show_where_they_go() {
    assert!(openable("https://example.com/x"));
    assert!(!openable("file:///etc/passwd"));
    assert!(!openable("https://discord.com@evil.example"));
    assert!(!openable("https://exa\u{200b}mple.com"));
    assert!(shows("example.com/", "https://Example.com"));
    assert!(!shows("discord.com", "https://evil.example"));
}

#[test]
fn rules_are_escaped_and_blank_lines_kept() {
    assert_eq!(no_rules("a\n---\n  ==="), "a\n\\---\n  \\===");
    assert_eq!(no_rules("a - b"), "a - b");
    assert_eq!(keep_blank_lines("a\n\nb\n"), "a\n\u{a0}\nb\n");
    assert_eq!(keep_blank_lines("```\na\n\nb\n```"), "```\na\n\nb\n```");
}

#[test]
fn spoilers_leave_code_empty_pairs_and_lone_bars_alone() {
    for text in [
        "`a || b || c`",
        "a |||| b",
        "|| a\n\nb ||",
        "one || alone",
        r"\||not\||",
    ] {
        assert!(spans(text).is_empty(), "{text}");
    }
    let found = spans("`||` ||x|| ``a`b``");
    assert_eq!((found.len(), found.first()), (1, Some(&(5..10))));
    assert_eq!(plain("**bold** and [link](https://e.com)"), "bold and link");
    assert_eq!(plain("  "), "spoiler");
}
