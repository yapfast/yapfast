//! What the grid is built from that isn't the account's: Unicode's
//! categories, and a search's matches (`rows`, `src/emoji.rs:450`). The
//! account's own sections (its favourites, what it uses most, each server's
//! emoji) are the projection's, from the core, which also names the
//! categories and says which emoji a search matches
//! (`yapfast_core::emoji::{category, matches}`): this crate keeps no copy.

use emojis::Group;

use super::{Emoji, Section, SectionIcon};

/// Unicode's categories in the `emojis` crate's order, which is Discord's,
/// each with the emoji `drawn` keeps (those Twemoji draws, of the versions
/// Discord's client offers: `yapfast_core::emoji::offered`). `category`
/// gives each one's emoji in the rail and its name in the chosen language
/// (`yapfast_core::emoji::category`). Groups `drawn` leaves empty aren't
/// listed.
pub fn categories(
    category: impl Fn(Group) -> (&'static str, &'static str),
    drawn: impl Fn(&emojis::Emoji) -> bool,
) -> Vec<Section> {
    Group::iter()
        .filter_map(|group| {
            let emoji: Vec<Emoji> = group
                .emojis()
                .filter(|emoji| drawn(emoji))
                .map(Emoji::Unicode)
                .collect();
            let (glyph, name) = category(group);
            (!emoji.is_empty()).then(|| Section {
                name: name.to_owned(),
                icon: emojis::get(glyph).map_or(SectionIcon::Frequent, SectionIcon::Category),
                emoji,
            })
        })
        .collect()
}

/// A search's matches for `query`, colons and case aside: the servers'
/// emoji among `sections` whose name holds it, then the Unicode ones
/// `unicode` takes for the lowercase query (offered and matching:
/// `yapfast_core::emoji::{offered, matches}`). None for an empty query.
pub fn found(
    sections: &[Section],
    query: &str,
    unicode: impl Fn(&emojis::Emoji, &str) -> bool,
) -> Vec<Emoji> {
    let query = query.trim().trim_matches(':').to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let custom = sections
        .iter()
        .filter(|section| matches!(section.icon, SectionIcon::Server { .. }))
        .flat_map(|section| &section.emoji)
        .filter(|emoji| {
            emoji
                .key()
                .rsplit_once(':')
                .is_some_and(|(name, _)| name.to_lowercase().contains(&query))
        })
        .cloned();
    let unicode = emojis::iter()
        .filter(|emoji| unicode(emoji, &query))
        .map(Emoji::Unicode);
    custom.chain(unicode).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::Expected;

    fn every(_: &emojis::Emoji) -> bool {
        true
    }

    /// Names each category by its group, shown by its first emoji.
    fn named(group: Group) -> (&'static str, &'static str) {
        let first = group.emojis().next().map_or("", emojis::Emoji::as_str);
        let name = match group {
            Group::SmileysAndEmotion => "Smileys",
            Group::TravelAndPlaces => "Travel",
            _ => "Other",
        };
        (first, name)
    }

    fn by_name(emoji: &emojis::Emoji, query: &str) -> bool {
        emoji.name().contains(query) || emoji.shortcodes().any(|code| code.contains(query))
    }

    #[test]
    fn the_categories_go_in_discords_order() {
        let sections = categories(named, every);
        assert_eq!(sections.len(), 9);
        assert_eq!(sections[0].name, "Smileys");
        assert_eq!(sections[4].name, "Travel");
        assert_eq!(
            sections[4].icon,
            SectionIcon::Category(Group::TravelAndPlaces.emojis().next().unwrap())
        );
        assert_eq!(sections[0].emoji[0].key(), "😀");
        assert_eq!(sections[8].emoji[0].key(), "🏁", "flags last");
        let none = categories(named, |emoji| emoji.group() == Group::Flags);
        assert_eq!(none.len(), 1, "a category with nothing drawn isn't listed");
    }

    #[test]
    fn a_search_finds_the_servers_emoji_then_unicodes() {
        let sections = vec![
            Section {
                name: "Server".into(),
                icon: SectionIcon::Server {
                    id: "1".into(),
                    picture: Expected::None,
                },
                emoji: vec![
                    Emoji::custom("party", "301", Expected::Waiting, true),
                    Emoji::custom("talk", "300", Expected::Waiting, true),
                ],
            },
            Section {
                name: "Favorites".into(),
                icon: SectionIcon::Favorites,
                emoji: vec![Emoji::custom("party", "301", Expected::Waiting, true)],
            },
        ];
        let matches = found(&sections, " :PART", by_name);
        assert_eq!(matches[0].key(), "party:301", "once, from its server");
        assert_ne!(matches[1].key(), "party:301");
        assert!(
            matches.iter().any(|emoji| emoji.key() == "🥳"),
            "partying face"
        );
        assert!(found(&sections, "::", by_name).is_empty());
        assert!(
            found(&sections, "thumbsup", by_name)
                .iter()
                .any(|e| e.key() == "👍"),
            "by shortcode"
        );
    }
}
