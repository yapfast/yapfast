//! Emoji as the window names and finds them: the `:name` word being typed
//! and what it suggests, a custom emoji's key and what it puts in a message,
//! a reaction's shortcode, and the names Discord's settings keep favourite
//! and used emoji by ([`discord_key`], [`discord_name`]), from Discord's own
//! table where it differs from GitHub's shortcodes ([`names`]).
//!
//! What draws an emoji is the client's: the functions that list emoji take
//! whether one can be drawn.

use std::collections::HashMap;

use crate::chat::{CustomEmoji, EmojiSet};

pub mod card;
pub mod names;

/// Windows 11 and the Noto Color Emoji builds Yapfast is tested with draw Unicode
/// 15.0; newer emoji would show as empty boxes.
pub const NEWEST: emojis::UnicodeVersion = emojis::UnicodeVersion::new(15, 0);

/// A custom emoji's reaction key, Discord's `name:id`.
pub fn custom_key(emoji: &CustomEmoji) -> String {
    format!("{}:{}", emoji.text.trim_matches(':'), emoji.id)
}

/// What an emoji puts in the composer: the emoji itself, or a custom emoji's
/// `:name:`, which goes out in Discord's form; see `crate::chat::wire_emoji`.
pub fn composer_text(key: &str) -> String {
    match key.split_once(':') {
        Some((name, _)) => format!(":{name}:"),
        None => key.to_owned(),
    }
}

/// An emoji's name as a picker shows it: "Thumbs up", or a custom one's
/// `:name:`.
pub fn title(key: &str) -> String {
    if let Some((name, id)) = key.split_once(':')
        && !name.is_empty()
        && id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return format!(":{name}:");
    }
    let name = emojis::get(key).map_or(key, emojis::Emoji::name);
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A reaction's emoji as Discord's client names it: `:name:` for a custom
/// one, whose key is already that, and the first shortcode of a Unicode one
/// that's a word, such as `:thumbsup:` rather than `:+1:`; the emoji itself
/// without one.
pub fn shortcode(key: &str) -> String {
    if key.starts_with(':') && key.ends_with(':') && key.len() > 2 {
        return key.to_owned();
    }
    let codes = || {
        emojis::get(key)
            .into_iter()
            .flat_map(emojis::Emoji::shortcodes)
    };
    codes()
        .find(|code| code.starts_with(|c: char| c.is_ascii_alphanumeric()))
        .or_else(|| codes().next())
        .map_or_else(|| key.to_owned(), |code| format!(":{code}:"))
}

/// Emoji names shorter than this aren't looked up while typing, as on Discord.
const MIN_QUERY: usize = 2;
const MAX_QUERY: usize = 32;

/// The `:name` word ending at `cursor`, a byte offset into `text`: where its
/// `:` is, and the name so far, of at least two letters.
pub fn query(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let before = text.get(..cursor)?;
    let colon = before.rfind(':')?;
    let query = &before[colon + 1..];
    let starts_word = before[..colon]
        .chars()
        .next_back()
        .is_none_or(|previous| previous.is_whitespace() || "([{\"'".contains(previous));
    let name = |c: char| c.is_ascii_alphanumeric() || "_+-".contains(c);
    (starts_word && (MIN_QUERY..=MAX_QUERY).contains(&query.len()) && query.chars().all(name))
        .then_some((colon, query))
}

/// An emoji a list offers: its key (the emoji, or a custom one's `name:id`)
/// and the name it's offered by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub key: String,
    pub name: String,
}

/// Suggestions for a `:name` being typed, at most `limit`: emoji whose name
/// starts with `query`, then those whose name contains it, a server's custom
/// emoji first, of the Unicode ones those `drawn` says the client draws. Each
/// is named by its `:name:`.
pub fn suggestions(
    query: &str,
    custom: &[EmojiSet],
    drawn: impl Fn(&str) -> bool,
    limit: usize,
) -> Vec<Found> {
    let query = query.to_lowercase();
    // The emoji's rank and the name it matched by.
    let rank = |name: &str| {
        let lower = name.to_lowercase();
        if lower == query {
            Some(0)
        } else if lower.starts_with(&query) {
            Some(1)
        } else if lower.contains(&query) {
            Some(2)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, bool, String, String)> = Vec::new();
    for emoji in custom.iter().flat_map(|set| &set.emoji) {
        let name = emoji.text.trim_matches(':');
        if let Some(rank) = rank(name) {
            found.push((rank, false, name.to_owned(), custom_key(emoji)));
        }
    }
    for emoji in emojis::iter().filter(|emoji| offered(emoji, &drawn)) {
        if let Some((rank, code)) = emoji
            .shortcodes()
            .filter_map(|code| Some((rank(code)?, code)))
            .min_by_key(|(rank, _)| *rank)
        {
            found.push((rank, true, code.to_owned(), emoji.as_str().to_owned()));
        }
    }
    // Stable, so each rank keeps custom emoji, then Unicode's own order.
    found.sort_by_key(|(rank, unicode, ..)| (*rank, *unicode));
    found
        .into_iter()
        .take(limit)
        .map(|(_, _, name, key)| Found {
            key,
            name: format!(":{name}:"),
        })
        .collect()
}

/// Whether a list offers this Unicode emoji: one of the versions Discord's
/// client offers that the client draws.
pub fn offered(emoji: &emojis::Emoji, drawn: impl Fn(&str) -> bool) -> bool {
    emoji.unicode_version() <= NEWEST && drawn(emoji.as_str())
}

/// The picker's key for an emoji as Discord's settings name it, and whether it
/// may be used here: a custom emoji's id, found in `custom`, or a Unicode
/// emoji's Discord name, such as `slight_smile`, `thumbsup_tone2`,
/// `person_tone1_bald` or `flag_fr`, from Discord's own names where they differ
/// from GitHub's shortcodes. `None` for a custom emoji not there, or a name
/// Yapfast doesn't know. A number can be a Unicode emoji's name too: `100`.
pub fn discord_key(name: &str, custom: &HashMap<&str, &CustomEmoji>) -> Option<(String, bool)> {
    if let Some(emoji) = custom.get(name) {
        return Some((custom_key(emoji), emoji.usable));
    }
    // A skin tone comes last in most names, but in some within them.
    let (base, tone) = match name.split_once("_tone") {
        Some((start, rest)) if !rest.contains("_tone") => {
            let (tone, end) = rest.split_at_checked(1)?;
            (format!("{start}{end}"), tone)
        }
        _ => (name.to_owned(), ""),
    };
    let names = names::NAMES;
    let emoji = names
        .binary_search_by_key(&base.as_str(), |(name, _)| name)
        .ok()
        .and_then(|at| emojis::get(names[at].1))
        .or_else(|| country_flag(&base))
        .or_else(|| emojis::get_by_shortcode(&base))?;
    let emoji = match tone {
        "" => emoji,
        "1" => emoji.with_skin_tone(emojis::SkinTone::Light)?,
        "2" => emoji.with_skin_tone(emojis::SkinTone::MediumLight)?,
        "3" => emoji.with_skin_tone(emojis::SkinTone::Medium)?,
        "4" => emoji.with_skin_tone(emojis::SkinTone::MediumDark)?,
        "5" => emoji.with_skin_tone(emojis::SkinTone::Dark)?,
        _ => return None,
    };
    (emoji.unicode_version() <= NEWEST).then(|| (emoji.as_str().to_owned(), true))
}

/// The name Discord's settings keep the emoji of picker key `key` by, as
/// [`discord_key`] reads it back: a custom emoji's id, or a Unicode emoji's
/// Discord name without its skin tone, as Discord's client stars the emoji
/// itself.
pub fn discord_name(key: &str) -> Option<String> {
    if let Some((_, id)) = key.rsplit_once(':') {
        return Some(id.to_owned());
    }
    let emoji = emojis::get(key)?;
    let base = emoji
        .with_skin_tone(emojis::SkinTone::Default)
        .unwrap_or(emoji);
    let flag = || {
        let letters: Option<String> = base
            .as_str()
            .chars()
            .map(|point| {
                let letter = u32::from(point).checked_sub(0x1F1E6)?;
                (letter < 26).then(|| char::from(b'a' + letter as u8))
            })
            .collect();
        letters
            .filter(|letters| letters.len() == 2)
            .map(|letters| format!("flag_{letters}"))
    };
    // The emoji's first name, which Discord's client takes a favourite out
    // by, then any other it reads back as this emoji: some of GitHub's name
    // another one on Discord.
    let none = HashMap::new();
    let plain = |text: &str| text.replace('\u{FE0F}', "");
    let first = names::FIRST
        .iter()
        .find(|(glyph, _)| plain(glyph) == plain(base.as_str()))
        .map(|(_, name)| (*name).to_owned());
    first
        .into_iter()
        .chain(
            base.shortcodes()
                .filter(|code| code.starts_with(|c: char| c.is_ascii_alphanumeric()))
                .map(str::to_owned),
        )
        .chain(
            names::NAMES
                .iter()
                .filter(|(_, glyph)| {
                    emojis::get(glyph).map(emojis::Emoji::as_str) == Some(base.as_str())
                })
                .map(|(name, _)| (*name).to_owned()),
        )
        .chain(flag())
        .chain(base.shortcodes().map(str::to_owned))
        .find(|name| discord_key(name, &none).is_some_and(|(found, _)| found == base.as_str()))
}

/// The flag `flag_xx` names, from its country's two letters: GitHub's shortcode
/// `cl` is the CL button, not Chile's flag.
fn country_flag(name: &str) -> Option<&'static emojis::Emoji> {
    let code = name.strip_prefix("flag_").filter(|code| code.len() == 2)?;
    let flag: String = code
        .bytes()
        .map(|letter| char::from_u32(0x1F1E6 + u32::from(letter.checked_sub(b'a')?)))
        .collect::<Option<_>>()?;
    emojis::get(&flag)
}

/// The picker's keys of the emoji `names`, those Discord's settings name, and
/// whether each may be used here, leaving out those there's no showing.
pub fn discord_keys<'a>(
    names: impl IntoIterator<Item = &'a String>,
    custom: &HashMap<&str, &CustomEmoji>,
) -> Vec<(String, bool)> {
    names
        .into_iter()
        .filter_map(|name| discord_key(name, custom))
        .collect()
}

/// Unicode's categories as the pickers head them, with the emoji each is
/// shown by on the picker's rail.
pub fn category(group: emojis::Group) -> (&'static str, &'static str) {
    match group {
        emojis::Group::SmileysAndEmotion => ("😀", "Smileys & emotion"),
        emojis::Group::PeopleAndBody => ("👋", "People & body"),
        emojis::Group::AnimalsAndNature => ("🐻", "Animals & nature"),
        emojis::Group::FoodAndDrink => ("🍔", "Food & drink"),
        emojis::Group::TravelAndPlaces => ("✈️", "Travel & places"),
        emojis::Group::Activities => ("⚽", "Activities"),
        emojis::Group::Objects => ("💡", "Objects"),
        emojis::Group::Symbols => ("🔣", "Symbols"),
        emojis::Group::Flags => ("🏁", "Flags"),
    }
}

/// The Unicode emoji whose name or a shortcode holds `query`, lowercase.
pub fn matches(emoji: &emojis::Emoji, query: &str) -> bool {
    emoji.name().contains(query) || emoji.shortcodes().any(|code| code.contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(text: &str, id: &str, usable: bool) -> CustomEmoji {
        CustomEmoji {
            text: text.into(),
            id: id.into(),
            animated: false,
            usable,
            picture: String::new(),
        }
    }

    #[test]
    fn finds_emoji_by_discord_names() {
        assert!(
            names::NAMES.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "sorted"
        );
        let talk = custom(":talk:", "5", false);
        let custom = HashMap::from([("5", &talk)]);
        let key = |name| discord_key(name, &custom).map(|(key, _)| key);
        // Discord's own names, then GitHub's that it shares.
        assert_eq!(key("skull_crossbones").as_deref(), Some("☠️"));
        assert_eq!(key("slight_smile").as_deref(), Some("🙂"));
        assert_eq!(key("joy").as_deref(), Some("😂"));
        assert_eq!(key("thumbsup_tone4").as_deref(), Some("👍🏾"));
        assert_eq!(key("person_tone1_bald").as_deref(), Some("🧑🏻‍🦲"));
        assert_eq!(key("6"), None, "a custom emoji not usable here");
        assert_eq!(key("no_such_emoji"), None);
        // Whether it may be used comes with a custom emoji.
        assert_eq!(discord_key("5", &custom), Some(("talk:5".into(), false)));
        // A number that's no custom emoji can be a Unicode emoji's name.
        assert_eq!(key("100").as_deref(), Some("💯"));
        assert_eq!(key("1234").as_deref(), Some("🔢"));
        // Flags are their country's, though GitHub's shortcode is something else.
        assert_eq!(key("flag_fr").as_deref(), Some("🇫🇷"));
        assert_eq!(key("flag_cl").as_deref(), Some("🇨🇱"));
        assert_eq!(key("flag_tv").as_deref(), Some("🇹🇻"));
        // Where JoyPixels and Discord mean different emoji by a name.
        assert_eq!(key("beach_umbrella").as_deref(), Some("⛱️"));
        assert_eq!(key("city_sunset").as_deref(), Some("🌇"));
    }

    #[test]
    fn names_an_emoji_as_discord_keeps_its_favourites() {
        // As Discord's settings name them, read back to the same emoji.
        for (key, name) in [
            ("☠️", "skull_crossbones"),
            ("🙂", "slight_smile"),
            ("😂", "joy"),
            ("🇫🇷", "flag_fr"),
            ("🇨🇱", "flag_cl"),
            ("💯", "100"),
            ("⛱️", "beach_umbrella"),
            ("talk:5", "5"),
        ] {
            assert_eq!(discord_name(key).as_deref(), Some(name), "{key}");
        }
        // Discord's first name, not GitHub's `+1` or the `thumbs_up` it reads
        // too, and a hyphen as Discord writes it.
        assert_eq!(discord_name("👍").as_deref(), Some("thumbsup"));
        assert_eq!(discord_name("📧").as_deref(), Some("e_mail"));
        // A skin tone is the emoji itself, which Discord's client stars.
        assert_eq!(discord_name("👍🏾").as_deref(), Some("thumbsup"));
        assert_eq!(discord_name("not an emoji"), None);
    }

    #[test]
    fn suggests_emoji_for_the_name_being_typed() {
        assert_eq!(query("look :thu", 9), Some((5, "thu")));
        assert_eq!(query(":+1", 3), Some((0, "+1")));
        assert_eq!(query("(:pizza", 7), Some((1, "pizza")));
        assert_eq!(query("look :t", 7), None, "two letters at least");
        assert_eq!(query("at 10:30", 8), None, "not after a letter or digit");
        assert_eq!(query(":thumbs up", 10), None, "a space ends it");
        assert_eq!(query(":pog:", 5), None, "a closed name");

        let custom = [EmojiSet {
            id: "90".into(),
            server: "Games".into(),
            icon: None,
            emoji: vec![custom(":thunk:", "5", true)],
        }];
        let all = |_: &str| true;
        let found = suggestions("thu", &custom, all, 10);
        assert_eq!(found[0].key, "thunk:5", "a server's emoji first");
        assert_eq!(found[0].name, ":thunk:");
        assert!(
            found[1..]
                .iter()
                .any(|found| found.key == "👍" && found.name == ":thumbsup:")
        );
        assert!(found.len() <= 10);
        let exact = suggestions("pizza", &[], all, 10);
        assert_eq!(
            (exact[0].key.as_str(), exact[0].name.as_str()),
            ("🍕", ":pizza:")
        );
        assert!(suggestions("zzzzzz", &[], all, 10).is_empty());
        assert!(
            suggestions("pizza", &[], |_| false, 10).is_empty(),
            "only what the client draws"
        );
    }

    #[test]
    fn names_emoji_as_the_pickers_show_them() {
        assert_eq!(title("👍"), "Thumbs up");
        assert_eq!(title("talk:5"), ":talk:");
        assert_eq!(composer_text("talk:5"), ":talk:");
        assert_eq!(shortcode("👍"), ":thumbsup:");
    }
}
