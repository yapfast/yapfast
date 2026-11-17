//! Emoji in text: those of Unicode's own, a `:shortcode:` sent without its
//! completion, and whether a message is emoji alone, which Discord shows
//! large. Ports `emoji_at` and `shortcode_at` of `src/emoji_text.rs` and
//! `only_emoji` of `src/custom_emoji.rs`.

use super::source::CustomEmoji;

/// The most code points an emoji has, as many starts as [`emoji_at`] tries.
const MOST_POINTS: usize = 12;

/// The most emoji a message of emoji alone shows large.
const MOST_LARGE: usize = 30;

/// The length in bytes of the longest emoji `text` starts with, or 0 when it
/// starts with none. Text that starts with a character no emoji does, as
/// any letter, is never looked up.
pub fn emoji_at(text: &str) -> usize {
    if !text.chars().next().is_some_and(could_be_emoji) {
        return 0;
    }
    let mut ends = [0usize; MOST_POINTS];
    let mut count = 0;
    for ((index, character), end) in text.char_indices().zip(&mut ends) {
        *end = index + character.len_utf8();
        count += 1;
    }
    ends[..count]
        .iter()
        .rev()
        .copied()
        .find(|&end| emojis::get(&text[..end]).is_some())
        .unwrap_or(0)
}

/// Whether an emoji could start at `character`: none starts below U+2000 but
/// the keycaps' 0 to 9, `#` and `*`, and © and ®.
fn could_be_emoji(character: char) -> bool {
    match character {
        '0'..='9' | '#' | '*' | '©' | '®' => true,
        _ => character >= '\u{2000}',
    }
}

/// The emoji `text` names at its start between colons, as `:sob:`, and how
/// many bytes that name takes.
pub fn shortcode_at(text: &str) -> Option<(usize, &'static str)> {
    let rest = text.strip_prefix(':')?;
    let end = rest.find(':')?;
    let name = &rest[..end];
    let shaped = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-+".contains(character));
    if !shaped {
        return None;
    }
    emojis::get_by_shortcode(name).map(|emoji| (end + 2, emoji.as_str()))
}

/// The server's emoji whose name `text` starts with, of those whose
/// pictures arrived.
pub fn custom_at<'a>(text: &str, emoji: &'a [CustomEmoji]) -> Option<&'a CustomEmoji> {
    if !text.starts_with(':') {
        return None;
    }
    emoji
        .iter()
        .find(|each| each.still.is_some() && !each.text.is_empty() && text.starts_with(&each.text))
}

/// Whether `text` is nothing but emoji, 1 to 30 of them, and space between:
/// the server's whose pictures arrived, and Unicode's.
pub fn only_emoji(text: &str, emoji: &[CustomEmoji]) -> bool {
    let mut count = 0;
    for word in text.split_whitespace() {
        let mut rest = word;
        while !rest.is_empty() {
            let length = match custom_at(rest, emoji) {
                Some(each) => each.text.len(),
                None => match emoji_at(rest) {
                    0 => return false,
                    length => length,
                },
            };
            rest = &rest[length..];
            count += 1;
            if count > MOST_LARGE {
                return false;
            }
        }
    }
    count > 0
}
