//! A server's preview: what its card shows, such as the count of its members
//! who are online and whether Discover lists it. Discord's client asks for it
//! with `GET /guilds/{guild.id}/preview`; see
//! https://docs.discord.food/resources/guild. A server tag's card asks for the
//! server's profile instead, with `GET /guilds/{guild.id}/profile`; see
//! https://docs.discord.food/resources/discovery.

use serde_json::Value;

use crate::ready;

/// What a server's card shows of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    /// The server, by its id.
    pub guild: String,
    pub name: String,
    /// The address of its icon, if it has one.
    pub icon: Option<String>,
    /// How many members it has, and how many of them are online, when Discord
    /// counts them: it does for a server this account is in, and for one
    /// Discover lists.
    pub members: Option<u64>,
    pub online: Option<u64>,
    /// Discover lists it, as the features it lists say.
    pub discoverable: bool,
}

impl Preview {
    /// The preview of a partial guild object, as the endpoint answers it.
    pub fn of(preview: &Value) -> Option<Self> {
        let guild = preview["id"].as_str()?;
        let features = preview["features"].as_array();
        Some(Self {
            guild: guild.to_owned(),
            name: preview["name"].as_str().unwrap_or_default().to_owned(),
            icon: preview["icon"]
                .as_str()
                .map(|hash| ready::server_icon(guild, hash)),
            members: preview["approximate_member_count"].as_u64(),
            online: preview["approximate_presence_count"].as_u64(),
            discoverable: features.is_some_and(|features| {
                features
                    .iter()
                    .any(|feature| feature.as_str() == Some("DISCOVERABLE"))
            }),
        })
    }
}

/// What a server tag's card shows of the server it's from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// The address of its icon, if it has one.
    pub icon: Option<String>,
    /// What it says of itself; empty without it.
    pub description: String,
    /// How many members it has, and how many of them are online, when Discord
    /// counts them.
    pub members: Option<u64>,
    pub online: Option<u64>,
    /// The address of the picture across its card's top, its discovery
    /// splash, when it has one.
    pub banner: Option<String>,
    /// Its accent colour, as RGB, which fills the card's top without a banner.
    pub color: Option<u32>,
    /// The words it describes itself with, at most five, each with its
    /// emoji, if any, in order.
    pub traits: Vec<(String, String)>,
    /// Anyone may join it from its card: its `visibility` is public (1).
    pub joinable: bool,
}

/// A server profile's `visibility` for a server anyone may see and join;
/// 2 needs an invite, and 3 takes requests to join.
const PUBLIC: u64 = 1;

impl Profile {
    /// The profile of server `guild`, as `GET /guilds/{guild.id}/profile`
    /// answers it; none for an answer without a name.
    pub fn of(guild: &str, profile: &Value) -> Option<Self> {
        Some(Self {
            name: profile["name"].as_str()?.to_owned(),
            icon: profile["icon_hash"]
                .as_str()
                .map(|hash| ready::server_icon(guild, hash)),
            description: profile["description"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            members: profile["member_count"].as_u64(),
            online: profile["online_count"].as_u64(),
            banner: profile["custom_banner_hash"].as_str().map(|hash| {
                format!(
                    "{}/discovery-splashes/{guild}/{hash}.png?size=600",
                    ready::cdn()
                )
            }),
            color: profile["brand_color_primary"]
                .as_str()
                .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()),
            traits: {
                let mut traits: Vec<(i64, String, String)> = profile["traits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|word| {
                        let label = word["label"].as_str()?.trim();
                        (!label.is_empty()).then(|| {
                            (
                                word["position"].as_i64().unwrap_or(0),
                                word["emoji_name"].as_str().unwrap_or_default().to_owned(),
                                label.to_owned(),
                            )
                        })
                    })
                    .collect();
                traits.sort_by_key(|(position, ..)| *position);
                traits
                    .into_iter()
                    .take(5)
                    .map(|(_, emoji, label)| (emoji, label))
                    .collect()
            },
            joinable: profile["visibility"].as_u64() == Some(PUBLIC),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A server profile's banner, colour, words and whether anyone may
    /// join, as its tag's card shows them.
    #[test]
    fn a_profile_has_its_banner_words_and_visibility() {
        let profile = Profile::of(
            "10",
            &json!({"name": "Home", "custom_banner_hash": "ghi", "brand_color_primary": "#7cf895",
            "visibility": 1, "traits": [
                {"emoji_name": "video_game", "label": "games", "position": 1},
                {"emoji_name": null, "label": "friendly", "position": 0},
            ]}),
        )
        .unwrap();
        assert!(
            profile
                .banner
                .is_some_and(|banner| banner.contains("discovery-splashes/10/ghi"))
        );
        assert_eq!(profile.color, Some(0x7cf895));
        assert_eq!(
            profile.traits,
            [
                (String::new(), "friendly".to_owned()),
                ("video_game".to_owned(), "games".to_owned())
            ]
        );
        assert!(profile.joinable);
        let bare = Profile::of("10", &json!({"name": "Home", "visibility": 2})).unwrap();
        assert!(!bare.joinable, "a restricted one takes an invite");
    }

    #[test]
    fn reads_what_a_servers_card_shows() {
        let preview = Preview::of(&json!({
            "id": "10",
            "name": "Aliens",
            "icon": "abc",
            "features": ["COMMUNITY", "DISCOVERABLE"],
            "approximate_member_count": 4644,
            "approximate_presence_count": 1200,
        }))
        .expect("a server's preview");
        assert_eq!(preview.guild, "10");
        assert_eq!(preview.name, "Aliens");
        assert!(preview.icon.is_some_and(|icon| icon.contains("10/abc")));
        assert_eq!(preview.members, Some(4644));
        assert_eq!(preview.online, Some(1200));
        assert!(preview.discoverable);
    }

    #[test]
    fn a_private_server_has_no_counts_nor_discovery() {
        let preview = Preview::of(&json!({ "id": "10", "name": "Home", "features": [] }))
            .expect("a server's preview");
        assert_eq!(preview.members, None);
        assert_eq!(preview.online, None);
        assert!(!preview.discoverable);
        assert_eq!(Preview::of(&json!({ "name": "Nowhere" })), None);
    }

    #[test]
    fn reads_what_a_server_tags_card_shows() {
        let profile = Profile::of(
            "10",
            &json!({
                "id": "10",
                "name": "Aliens",
                "icon_hash": "abc",
                "description": "We come in peace",
                "member_count": 4644,
                "online_count": 1200,
                "tag": "UFO",
                "badge_hash": "def",
                "visibility": 1,
            }),
        )
        .expect("a server's profile");
        assert_eq!(profile.name, "Aliens");
        assert!(profile.icon.is_some_and(|icon| icon.contains("10/abc")));
        assert_eq!(profile.description, "We come in peace");
        assert_eq!((profile.members, profile.online), (Some(4644), Some(1200)));
        let bare = Profile::of("10", &json!({ "name": "Home", "icon_hash": null }))
            .expect("a server's profile");
        assert_eq!((bare.icon, bare.description.as_str()), (None, ""));
        assert_eq!(
            Profile::of("10", &json!({ "message": "Missing Access" })),
            None
        );
    }
}
