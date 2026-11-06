//! A server's preview: what its card shows, such as the count of its members
//! who are online and whether Discover lists it. Discord's client asks for it
//! with `GET /guilds/{guild.id}/preview`; see
//! https://docs.discord.food/resources/guild.

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
}
