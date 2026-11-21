//! Discovery: the servers anyone may join, as Discord's Discover page lists
//! them. `GET /discoverable-guilds` pages through them, optionally by a
//! discovery category (`categories`), `GET /discoverable-guilds/search`
//! looks for a name or a word (`query`, with `category_id`), and `GET
//! /discovery/categories` names the categories. Joining one is `PUT
//! /guilds/{id}/members/@me`.

use serde_json::Value;

use crate::ready;

/// How many servers one page of the list asks for. Discord allows 48.
pub const PAGE: u32 = 24;

/// One server of the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// The icon's address.
    pub icon: Option<String>,
    /// The picture that heads its card: the discovery splash, or the
    /// invite splash when there is none.
    pub splash: Option<String>,
    pub members: Option<u64>,
    pub online: Option<u64>,
    /// The name of its primary category, when Discord names it.
    pub category: Option<String>,
}

/// One of Discord's categories of servers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Category {
    pub id: u32,
    pub name: String,
}

/// A page of the list: the servers, and how many match in all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    pub servers: Vec<Server>,
    pub total: u64,
}

/// The servers in an answer, which is `{guilds, total}` for the list and
/// the search, or a plain array of them. A server without an id or a name
/// is left out.
pub fn parse(value: &Value) -> Batch {
    let list = value["guilds"]
        .as_array()
        .or_else(|| value["hits"].as_array())
        .or_else(|| value.as_array());
    let servers: Vec<Server> = list.into_iter().flatten().filter_map(server).collect();
    let total = value["total"]
        .as_u64()
        .or_else(|| value["total_hits"].as_u64())
        .unwrap_or(servers.len() as u64);
    Batch { servers, total }
}

fn server(value: &Value) -> Option<Server> {
    let id = value["id"].as_str().filter(|id| !id.is_empty())?;
    let name = value["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())?;
    let hash = |key: &str| value[key].as_str().filter(|hash| !hash.is_empty());
    let splash = hash("discovery_splash")
        .map(|hash| ready::discovery_splash(id, hash))
        .or_else(|| hash("splash").map(|hash| ready::splash(id, hash)));
    Some(Server {
        id: id.to_owned(),
        name: name.to_owned(),
        description: value["description"]
            .as_str()
            .map(str::trim)
            .filter(|words| !words.is_empty())
            .map(str::to_owned),
        icon: hash("icon").map(|hash| ready::server_icon(id, hash)),
        splash,
        members: value["approximate_member_count"].as_u64(),
        online: value["approximate_presence_count"].as_u64(),
        category: value["primary_category"]["name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
    })
}

/// The categories in an answer from the categories list, in Discord's
/// order. One without an id or a name is left out.
pub fn categories(value: &Value) -> Vec<Category> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|category| {
            Some(Category {
                id: u32::try_from(category["id"].as_u64()?).ok()?,
                name: category["name"]
                    .as_str()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())?
                    .to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_a_page_of_the_list() {
        let answer = json!({
            "guilds": [
                {
                    "id": "10", "name": " Rustaceans ", "description": "Rust, mostly.",
                    "icon": "abc", "discovery_splash": "def", "splash": "ghi",
                    "approximate_member_count": 1200, "approximate_presence_count": 300,
                    "primary_category": {"id": 5, "name": "Science & Tech"}
                },
                {"id": "20", "name": "Bare", "description": "  ", "icon": null, "splash": "ghi"},
                {"id": "30", "name": ""},
                {"name": "No id"},
            ],
            "total": 40, "limit": 24, "offset": 0,
        });
        let batch = parse(&answer);
        assert_eq!(batch.total, 40);
        assert_eq!(
            batch.servers.len(),
            2,
            "a server without a name or id is dropped"
        );
        let first = &batch.servers[0];
        assert_eq!(first.name, "Rustaceans");
        assert_eq!(first.description.as_deref(), Some("Rust, mostly."));
        assert!(first.icon.as_deref().unwrap().contains("/icons/10/abc."));
        assert!(
            first
                .splash
                .as_deref()
                .unwrap()
                .contains("/discovery-splashes/10/def."),
            "the discovery splash is preferred"
        );
        assert_eq!((first.members, first.online), (Some(1200), Some(300)));
        assert_eq!(first.category.as_deref(), Some("Science & Tech"));
        let second = &batch.servers[1];
        assert_eq!(second.description, None, "blank words are no description");
        assert_eq!(second.icon, None);
        assert!(
            second
                .splash
                .as_deref()
                .unwrap()
                .contains("/splashes/20/ghi.")
        );
        assert_eq!((second.members, second.online), (None, None));
    }

    #[test]
    fn a_plain_array_is_a_page_too() {
        let batch = parse(&json!([{"id": "1", "name": "One"}]));
        assert_eq!(batch.servers.len(), 1);
        assert_eq!(batch.total, 1);
        assert_eq!(parse(&json!(null)), Batch::default());
    }

    #[test]
    fn reads_the_categories_in_order() {
        let list = json!([
            {"id": 1, "name": "Gaming", "is_primary": true},
            {"id": 3, "name": " Music "},
            {"id": 4},
            {"name": "No id"},
        ]);
        let found = categories(&list);
        assert_eq!(
            found,
            [
                Category {
                    id: 1,
                    name: "Gaming".into()
                },
                Category {
                    id: 3,
                    name: "Music".into()
                },
            ]
        );
        assert!(categories(&json!({})).is_empty());
    }
}
