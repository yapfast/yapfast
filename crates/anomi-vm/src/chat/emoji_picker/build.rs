//! What the grid is built from that isn't the account's: Unicode's
//! categories, and a search's matches (`rows`, `src/emoji.rs:450`). The
//! account's own sections (its favourites, what it uses most, each server's
//! emoji) are the projection's, from the core.

use emojis::Group;

use super::{Emoji, Section, SectionIcon};

/// Unicode's categories in Discord's order: each one's emoji in the rail,
/// and its name in English, which the projection translates
/// (`src/emoji.rs:551`).
pub const CATEGORIES: [(Group, &str, &str); 9] = [
    (Group::SmileysAndEmotion, "😀", "Smileys & emotion"),
    (Group::PeopleAndBody, "👋", "People & body"),
    (Group::AnimalsAndNature, "🐻", "Animals & nature"),
    (Group::FoodAndDrink, "🍔", "Food & drink"),
    (Group::TravelAndPlaces, "✈️", "Travel & places"),
    (Group::Activities, "⚽", "Activities"),
    (Group::Objects, "💡", "Objects"),
    (Group::Symbols, "🔣", "Symbols"),
    (Group::Flags, "🏁", "Flags"),
];

/// Unicode's categories, each named by `name` from its English name, with
/// the emoji `drawn` keeps: those Twemoji draws, of the versions Discord's
/// client offers (`drawn`, `src/emoji.rs:95`). Groups `drawn` leaves empty
/// aren't listed.
pub fn categories(
    name: impl Fn(&'static str) -> String,
    drawn: impl Fn(&emojis::Emoji) -> bool,
) -> Vec<Section> {
    CATEGORIES
        .iter()
        .filter_map(|&(group, glyph, english)| {
            let emoji: Vec<Emoji> = group
                .emojis()
                .filter(|emoji| drawn(emoji))
                .map(Emoji::Unicode)
                .collect();
            (!emoji.is_empty()).then(|| Section {
                name: name(english),
                icon: emojis::get(glyph).map_or(SectionIcon::Frequent, SectionIcon::Category),
                emoji,
            })
        })
        .collect()
}

/// A search's matches for `query`, colons and case aside: the servers'
/// emoji among `sections` whose name holds it, then Unicode's whose name or
/// a shortcode does, of those `drawn` keeps. None for an empty query.
pub fn found(
    sections: &[Section],
    query: &str,
    drawn: impl Fn(&emojis::Emoji) -> bool,
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
        .filter(|emoji| drawn(emoji))
        .filter(|emoji| {
            emoji.name().contains(&query) || emoji.shortcodes().any(|code| code.contains(&query))
        })
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

    #[test]
    fn the_categories_go_in_discords_order() {
        let sections = categories(str::to_owned, every);
        assert_eq!(sections.len(), 9);
        assert_eq!(sections[0].name, "Smileys & emotion");
        assert_eq!(
            sections[4].icon,
            SectionIcon::Category(emojis::get("✈️").unwrap())
        );
        assert_eq!(sections[0].emoji[0].key(), "😀");
        let none = categories(str::to_owned, |emoji| emoji.group() == Group::Flags);
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
        let matches = found(&sections, " :PART", every);
        assert_eq!(matches[0].key(), "party:301", "once, from its server");
        assert_ne!(matches[1].key(), "party:301");
        assert!(
            matches.iter().any(|emoji| emoji.key() == "🥳"),
            "partying face"
        );
        assert!(found(&sections, "::", every).is_empty());
        assert!(
            found(&sections, "thumbsup", every)
                .iter()
                .any(|e| e.key() == "👍"),
            "by shortcode"
        );
    }
}
