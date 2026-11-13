//! Forum channels, whose posts are threads, and what a list of their posts
//! shows of each: see <https://docs.discord.food/resources/channel> and
//! <https://docs.discord.food/topics/threads#forums>.

use serde_json::Value;

use crate::{message, ready};

/// A forum's channel type.
pub const GUILD_FORUM: u64 = 15;
/// A media channel's: a forum whose posts Discord's client shows as a
/// gallery, with the same fields.
pub const GUILD_MEDIA: u64 = 16;

/// A forum channel's flag that each post takes a tag.
const REQUIRE_TAG: u64 = 1 << 4;
/// A post's flag that it's pinned to the top of its forum.
const PINNED: u64 = 1 << 1;
/// Its `default_sort_order` that sorts posts by when they were made.
const CREATION_DATE: u64 = 1;

/// What a forum channel has that a text channel doesn't.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Forum {
    /// The tags its posts may take, in the forum's order.
    pub tags: Vec<Tag>,
    /// Each post takes a tag.
    pub require_tag: bool,
    /// Its posts sort by when they were made, rather than by their latest
    /// message, as the forum's default sort order says.
    pub by_creation: bool,
    /// The emoji a post without reactions offers to react with.
    pub reaction: Option<Emoji>,
}

/// A tag a forum's posts may take.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub emoji: Option<Emoji>,
    /// Only those who may manage threads give it.
    pub moderated: bool,
}

/// A Unicode emoji, or a custom one by id.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Emoji {
    Unicode(String),
    Custom { id: String, name: String },
}

impl Emoji {
    /// An emoji as a tag or a forum's default reaction carries it, by
    /// `emoji_id` and `emoji_name`.
    fn of_tag(value: &Value) -> Option<Self> {
        Self::parse(value["emoji_id"].as_str(), value["emoji_name"].as_str())
    }

    /// An emoji object, as a reaction carries it.
    fn of_reaction(emoji: &Value) -> Option<Self> {
        Self::parse(emoji["id"].as_str(), emoji["name"].as_str())
    }

    fn parse(id: Option<&str>, name: Option<&str>) -> Option<Self> {
        match (id, name) {
            (Some(id), name) => Some(Self::Custom {
                id: id.to_owned(),
                name: name.unwrap_or_default().to_owned(),
            }),
            (None, Some(name)) if !name.is_empty() => Some(Self::Unicode(name.to_owned())),
            _ => None,
        }
    }
}

/// What a forum or media channel object says of its posts; none for
/// another channel.
pub fn parse(channel: &Value) -> Option<Box<Forum>> {
    if !matches!(channel["type"].as_u64(), Some(GUILD_FORUM | GUILD_MEDIA)) {
        return None;
    }
    let tags = channel["available_tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| {
            Some(Tag {
                id: tag["id"].as_str()?.to_owned(),
                name: tag["name"].as_str().unwrap_or_default().to_owned(),
                emoji: Emoji::of_tag(tag),
                moderated: tag["moderated"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    Some(Box::new(Forum {
        tags,
        require_tag: channel["flags"].as_u64().unwrap_or_default() & REQUIRE_TAG != 0,
        by_creation: channel["default_sort_order"].as_u64() == Some(CREATION_DATE),
        reaction: Emoji::of_tag(&channel["default_reaction_emoji"]),
    }))
}

/// The tags a post's channel object gives it, by id.
pub fn applied_tags(thread: &Value) -> Vec<String> {
    thread["applied_tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// Whether a post's channel object says it's pinned.
pub fn pinned(thread: &Value) -> bool {
    thread["flags"].as_u64().unwrap_or_default() & PINNED != 0
}

/// A post's first message, as its card in a forum shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Starter {
    /// Its author, by user id.
    pub author: String,
    /// Its text in Discord's markup, cut as a thread's newest message is.
    pub content: String,
    /// The reaction most given, and how many gave it, super reactions
    /// among them.
    pub reaction: Option<(Emoji, u64)>,
    /// Its first picture, from a file or an embed, at the size the
    /// card's thumbnail needs, and whether it's a spoiler.
    pub picture: Option<String>,
    pub spoiler: bool,
}

/// How large a thumbnail Yapfast asks for: its card shows it at 72px.
const THUMBNAIL: u64 = 144;

impl Starter {
    /// A post's first message object; none without an author.
    pub fn of(message: &Value) -> Option<Self> {
        let recent = ready::Recent::of(message)?;
        let reaction = message["reactions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|reaction| {
                let count = reaction["count"].as_u64().unwrap_or(1);
                let burst = reaction["burst_count"].as_u64().unwrap_or_default();
                Some((Emoji::of_reaction(&reaction["emoji"])?, count, burst))
            })
            // As Discord's client picks it: the most given, then the most
            // super reactions; the first of equals.
            .rev()
            .max_by_key(|(_, count, burst)| (count + burst, *burst))
            .map(|(emoji, count, burst)| (emoji, count + burst));
        let file = message["attachments"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|file| {
                file["content_type"]
                    .as_str()
                    .is_some_and(|kind| kind.starts_with("image/"))
            })
            .map(|file| {
                let spoiler = file["filename"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("SPOILER_"));
                (file, spoiler)
            });
        let embed = || {
            message["embeds"].as_array()?.iter().find_map(|embed| {
                let picture = &embed["image"];
                let picture = if picture.is_object() {
                    picture
                } else {
                    &embed["thumbnail"]
                };
                picture.is_object().then_some((picture, false))
            })
        };
        let (picture, spoiler) = file
            .or_else(embed)
            .and_then(|(picture, spoiler)| {
                Some((
                    message::scaled_url(picture, THUMBNAIL, THUMBNAIL, false)?,
                    spoiler,
                ))
            })
            .unzip();
        Some(Self {
            author: recent.author,
            content: recent.content,
            reaction,
            picture,
            spoiler: spoiler.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_a_forums_tags_and_defaults() {
        let forum = parse(&json!({
            "id": "1", "type": 15, "flags": 16, "default_sort_order": 1,
            "default_reaction_emoji": {"emoji_id": null, "emoji_name": "👍"},
            "available_tags": [
                {"id": "7", "name": "Bug", "moderated": false, "emoji_id": null, "emoji_name": "🐛"},
                {"id": "8", "name": "Staff", "moderated": true, "emoji_id": "9", "emoji_name": null},
                {"id": "10", "name": "Idea", "moderated": false, "emoji_id": null, "emoji_name": null},
            ],
        }))
        .unwrap();
        assert!(forum.require_tag && forum.by_creation);
        assert_eq!(forum.reaction, Some(Emoji::Unicode("👍".into())));
        let tags: Vec<_> = forum
            .tags
            .iter()
            .map(|tag| (tag.name.as_str(), tag.emoji.clone(), tag.moderated))
            .collect();
        assert_eq!(
            tags,
            [
                ("Bug", Some(Emoji::Unicode("🐛".into())), false),
                (
                    "Staff",
                    Some(Emoji::Custom {
                        id: "9".into(),
                        name: String::new()
                    }),
                    true
                ),
                ("Idea", None, false),
            ]
        );
        assert!(parse(&json!({"id": "2", "type": 0})).is_none());
        let plain = parse(&json!({"id": "3", "type": 15})).unwrap();
        assert!(!plain.require_tag && !plain.by_creation && plain.reaction.is_none());
        let media = parse(&json!({"id": "4", "type": 16, "flags": 16})).unwrap();
        assert!(media.require_tag);
    }

    #[test]
    fn a_starter_shows_its_top_reaction_and_first_picture() {
        let starter = Starter::of(&json!({
            "id": "5", "channel_id": "5", "content": "Look at this",
            "author": {"id": "2", "username": "bob"},
            "reactions": [
                {"emoji": {"id": null, "name": "👀"}, "count": 2, "burst_count": 0},
                {"emoji": {"id": "6", "name": "party"}, "count": 1, "burst_count": 1},
                {"emoji": {"id": null, "name": "🎉"}, "count": 2, "burst_count": 0},
            ],
            "attachments": [
                {"filename": "notes.txt", "content_type": "text/plain", "proxy_url": "https://media/notes.txt"},
                {"filename": "SPOILER_cat.png", "content_type": "image/png",
                 "proxy_url": "https://media/cat.png", "width": 600, "height": 300},
            ],
        }))
        .unwrap();
        assert_eq!(starter.author, "2");
        assert_eq!(starter.content, "Look at this");
        // Two each; of equals, the one with more super reactions.
        assert_eq!(
            starter.reaction,
            Some((
                Emoji::Custom {
                    id: "6".into(),
                    name: "party".into()
                },
                2
            ))
        );
        assert_eq!(
            starter.picture.as_deref(),
            Some("https://media/cat.png?format=webp&width=144&height=72")
        );
        assert!(starter.spoiler);
        let embedded = Starter::of(&json!({
            "id": "5", "content": "", "author": {"id": "2"},
            "reactions": [
                {"emoji": {"id": null, "name": "👀"}, "count": 1},
                {"emoji": {"id": null, "name": "🎉"}, "count": 1},
            ],
            "embeds": [{"thumbnail": {"proxy_url": "https://media/t.jpg", "width": 100, "height": 100}}],
        }))
        .unwrap();
        // The first of equals.
        assert_eq!(embedded.reaction, Some((Emoji::Unicode("👀".into()), 1)));
        assert_eq!(
            embedded.picture.as_deref(),
            Some("https://media/t.jpg?format=webp&width=100&height=100")
        );
        assert!(Starter::of(&json!({"id": "5", "content": "?"})).is_none());
    }

    #[test]
    fn reads_a_posts_tags_and_pin() {
        let post = json!({"id": "4", "flags": 2, "applied_tags": ["7", "8"]});
        assert_eq!(applied_tags(&post), ["7", "8"]);
        assert!(pinned(&post));
        assert!(!pinned(&json!({"flags": 0})));
        assert!(applied_tags(&json!({})).is_empty());
    }
}
