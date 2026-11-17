//! An emoji of the picker (`EmojiCell`, `ui/messages.slint:15`, as `cell`
//! in `src/emoji.rs:102` makes one): one of Unicode's, which Twemoji draws,
//! or a server's, with its picture.

use crate::kind::Expected;

/// An emoji of the picker. A Unicode one is the `emojis` crate's own, so
/// the thousands of the categories cost a pointer each.
#[derive(Clone, Debug, PartialEq)]
pub enum Emoji {
    Unicode(&'static emojis::Emoji),
    Custom(Box<Custom>),
}

/// A server's emoji.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Custom {
    /// What a reaction or a message carries: Discord's `name:id`.
    pub key: String,
    /// Its picture: `Waiting` while it comes, which shows a quiet square in
    /// its place.
    pub picture: Expected,
    /// This account may use it where it is looking; one it may not shows
    /// greyed with a lock, and pressing it offers Nitro.
    pub usable: bool,
}

impl Emoji {
    /// The Unicode emoji `glyph` is, skin tones included; none for text no
    /// emoji is.
    pub fn unicode(glyph: &str) -> Option<Self> {
        emojis::get(glyph).map(Self::Unicode)
    }

    /// A server's emoji named `name` (without colons), by its id.
    pub fn custom(name: &str, id: &str, picture: Expected, usable: bool) -> Self {
        Self::Custom(Box::new(Custom {
            key: format!("{name}:{id}"),
            picture,
            usable,
        }))
    }

    /// What picking it adds: the emoji itself, or a custom emoji's
    /// `name:id`.
    pub fn key(&self) -> &str {
        match self {
            Self::Unicode(emoji) => emoji.as_str(),
            Self::Custom(custom) => &custom.key,
        }
    }

    /// Its name as the foot shows it: Unicode's with a capital, as "Thumbs
    /// up", or a custom one's between colons, `:talk:`.
    pub fn name(&self) -> String {
        match self {
            Self::Unicode(emoji) => {
                let mut chars = emoji.name().chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
            Self::Custom(custom) => {
                let name = custom.key.rsplit_once(':').map_or("", |(name, _)| name);
                format!(":{name}:")
            }
        }
    }

    /// A custom emoji's id, which its picture is asked for by.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Unicode(_) => None,
            Self::Custom(custom) => custom.key.rsplit_once(':').map(|(_, id)| id),
        }
    }

    /// Whether this account may use it here: every Unicode one.
    pub fn usable(&self) -> bool {
        match self {
            Self::Unicode(_) => true,
            Self::Custom(custom) => custom.usable,
        }
    }

    /// A custom emoji's picture.
    pub fn picture(&self) -> Option<&Expected> {
        match self {
            Self::Unicode(_) => None,
            Self::Custom(custom) => Some(&custom.picture),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unicode_emoji_is_named_with_a_capital() {
        let thumbs = Emoji::unicode("👍").unwrap();
        assert_eq!(thumbs.key(), "👍");
        assert_eq!(thumbs.name(), "Thumbs up");
        assert!(thumbs.usable() && thumbs.id().is_none());
        assert!(Emoji::unicode("👍🏾").is_some(), "skin tones are emoji too");
        assert!(Emoji::unicode("talk").is_none());
    }

    #[test]
    fn a_custom_emoji_is_its_name_and_id() {
        let talk = Emoji::custom("talk", "1400000000000000300", Expected::Waiting, false);
        assert_eq!(talk.key(), "talk:1400000000000000300");
        assert_eq!(talk.name(), ":talk:");
        assert_eq!(talk.id(), Some("1400000000000000300"));
        assert!(!talk.usable());
        assert_eq!(talk.picture(), Some(&Expected::Waiting));
    }
}
