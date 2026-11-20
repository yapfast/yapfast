//! Text a plugin puts in Yapfast's own interface: its name, its labels, its
//! badges and the words in its drawings.
//!
//! `char::is_control` is only Unicode's `Cc`, so it lets through the format
//! characters that change how a line reads without showing themselves: a
//! right-to-left override that turns `moc.elgoog` into `google.com` on
//! screen, isolates, a zero-width space inside `Yapfast` that makes a
//! different name look the same, and a byte-order mark. Every check of plugin text goes through
//! here so none of them can forget one.

/// Whether `c` may not appear in plugin text that is one line: a control
/// character, a line or paragraph separator, or a format character that hides
/// or reorders text (see [`invisible`]).
pub fn refused(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') || invisible(c)
}

/// Whether `c` is a format character that shows nothing or changes the
/// direction of what follows: the soft hyphen, the Arabic letter mark, the
/// Mongolian vowel separator, the zero-width space, non-joiner, joiner and the
/// two direction marks, the bidi embeddings, overrides and isolates, the
/// word joiner and the invisible operators, the Hangul fillers that draw as a
/// blank, the byte-order mark, the interlinear annotation marks and the tag
/// characters.
///
/// The zero-width joiner is how an emoji is built from others (a family, a
/// flag with a rainbow), so [`check`] allows it between two characters that
/// aren't ASCII; this function, which sees one character, refuses it.
pub fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Why a line of plugin text can't be shown, or `Ok` when it can: no control
/// character or line break, and no invisible or direction-changing character
/// apart from a zero-width joiner inside an emoji.
pub fn check(text: &str) -> Result<(), &'static str> {
    let chars: Vec<char> = text.chars().collect();
    for (at, &c) in chars.iter().enumerate() {
        if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
            return Err("it has a line break or control character");
        }
        if c == '\u{200D}' && joins_emoji(&chars, at) {
            continue;
        }
        if invisible(c) {
            return Err("it has an invisible or direction-changing character");
        }
    }
    Ok(())
}

/// `text` without the characters [`check`] refuses, apart from line breaks:
/// for the longer words of a drawing, which are kept rather than refused.
pub fn strip(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter(|&(at, &c)| {
            c == '\n'
                || (c == '\u{200D}' && joins_emoji(&chars, at))
                || !(c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') || invisible(c))
        })
        .map(|(_, &c)| c)
        .collect()
}

/// Whether the zero-width joiner at `at` sits between two characters that
/// aren't ASCII, as it does inside an emoji sequence.
fn joins_emoji(chars: &[char], at: usize) -> bool {
    let before = at.checked_sub(1).and_then(|before| chars.get(before));
    let after = chars.get(at + 1);
    matches!((before, after), (Some(before), Some(after)) if !before.is_ascii() && !after.is_ascii())
}

/// The longest a plugin's name may be, in characters: it is shown in the
/// install review, the consent question and beside everything it draws.
pub const NAME_CHARS: usize = 48;

/// Whether `name` is one a plugin may not take, because it would read as
/// Yapfast's own: any name that spells "yapfast", ignoring case, spaces and
/// punctuation, such as `Yapfast`, `Yapfast Security` or `Yap-fast Update`.
pub fn reserved(name: &str) -> bool {
    let letters: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    letters.contains("yapfast")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_and_invisible_characters_are_refused() {
        for text in [
            "moc.elgoog\u{202E}",
            "a\u{2066}b\u{2069}",
            "Yap\u{200B}fast",
            "\u{FEFF}name",
            "left\u{200E}mark",
            "tag\u{E0041}",
            "line\u{2028}break",
            "tab\there",
        ] {
            assert!(check(text).is_err(), "{text:?} passes");
        }
    }

    #[test]
    fn plain_text_and_emoji_pass() {
        for text in [
            "Word Filter",
            "Ünïcödé ok",
            "日本語",
            // A family emoji is people joined with zero-width joiners.
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            "\u{2764}\u{FE0F}",
        ] {
            assert_eq!(check(text), Ok(()), "{text:?} refused");
        }
    }

    #[test]
    fn strip_keeps_line_breaks_and_drops_the_rest() {
        assert_eq!(strip("a\u{202E}b\nc\u{200B}d"), "ab\ncd");
        assert_eq!(
            strip("\u{1F468}\u{200D}\u{1F469}"),
            "\u{1F468}\u{200D}\u{1F469}"
        );
        assert_eq!(strip("x\u{200D}y"), "xy");
    }

    #[test]
    fn a_name_that_spells_yapfast_is_reserved() {
        for name in ["Yapfast", "yapfast security", "Yap-Fast Update", "YAP FAST"] {
            assert!(reserved(name), "{name} isn't reserved");
        }
        for name in ["Word Filter", "Fast Yap", "Yap"] {
            assert!(!reserved(name), "{name} is reserved");
        }
    }
}
