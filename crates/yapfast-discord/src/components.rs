//! A message's components: the buttons and menus apps put under a message,
//! and the cards of text, pictures and files that make up a message sent
//! with the `IS_COMPONENTS_V2` flag, as Discord's own notices are. See
//! <https://docs.discord.food/resources/components>.
//!
//! The tree flattens into [`Card`]s of [`Row`]s, nested no deeper than the
//! UI draws: a container is a card, and components outside one share a card
//! without a frame. Pictures and files go among the message's attachments,
//! placed by a slot their row names, as embeds' do.

use serde_json::Value;

use crate::message::{
    Attachment, IS_SPOILER, PREVIEW_HEIGHT, PREVIEW_WIDTH, Shown, VIEW_HEIGHT, VIEW_WIDTH,
    alt_text, animated, scaled_url,
};

/// The message flag of a message made of components alone, without content
/// or embeds.
pub const IS_COMPONENTS_V2: u64 = 1 << 15;

const ACTION_ROW: u64 = 1;
const BUTTON: u64 = 2;
const STRING_SELECT: u64 = 3;
const USER_SELECT: u64 = 5;
const ROLE_SELECT: u64 = 6;
const MENTIONABLE_SELECT: u64 = 7;
const CHANNEL_SELECT: u64 = 8;
const SECTION: u64 = 9;
const TEXT_DISPLAY: u64 = 10;
const THUMBNAIL: u64 = 11;
const MEDIA_GALLERY: u64 = 12;
const FILE: u64 = 13;
const SEPARATOR: u64 = 14;
const CONTAINER: u64 = 17;

/// The separator spacing that leaves a large gap.
const LARGE: u64 = 2;

/// The size a section's thumbnail is downloaded at: twice what it shows at,
/// for high-DPI displays.
const THUMBNAIL_SIZE: u64 = 170;

/// A container, or a run of components outside one.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Card {
    /// A container's, which shows framed, with its accent bar when it has
    /// a colour.
    pub framed: bool,
    pub color: Option<u32>,
    pub rows: Vec<Row>,
}

impl Card {
    /// The Markdown of its texts, in order.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().filter_map(|row| match row {
            Row::Text { text, .. } if !text.is_empty() => Some(text.as_str()),
            _ => None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Row {
    /// Discord Markdown with mentions still as `<@id>`, and what a section
    /// shows beside it.
    Text {
        text: String,
        beside: Option<Beside>,
    },
    /// The attachments placed [`Shown::Component`] with this slot: a
    /// gallery's pictures, or a file.
    Media(usize),
    /// A gap, and with `divider` a line across it.
    Separator {
        divider: bool,
        large: bool,
    },
    Buttons(Vec<Button>),
    /// A menu, by what it shows before it's opened: the options picked by
    /// default, or its placeholder.
    /// A select menu, by the options it picks by default or its
    /// placeholder; `kind` is its component type, 3 for its own options and
    /// 5 to 8 for users, roles, both or channels, of `channel_types`.
    Select {
        text: String,
        disabled: bool,
        #[serde(default)]
        kind: u64,
        #[serde(default)]
        custom_id: String,
        #[serde(default)]
        options: Vec<SelectOption>,
        #[serde(default)]
        defaults: Vec<String>,
        #[serde(default)]
        channel_types: Vec<u64>,
        #[serde(default)]
        min_values: u64,
        #[serde(default)]
        max_values: u64,
    },
    /// Components of a kind Yapfast can't show, one or more.
    Unknown,
}

/// What a section shows beside its text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Beside {
    /// The picture placed [`Shown::Component`] with this slot.
    Thumbnail(usize),
    Button(Button),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Button {
    pub label: String,
    /// In Discord's markup: the emoji itself, or `<:name:id>`.
    pub emoji: Option<String>,
    pub style: Style,
    /// Where a link button leads.
    pub url: Option<String>,
    /// What its app knows it by, when using it asks the app.
    pub custom_id: Option<String>,
    /// What a premium button offers in its app's store.
    pub sku_id: Option<String>,
    pub disabled: bool,
}

/// An option of a string select menu.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    pub default: bool,
    /// Shown before its label, in Discord's markup, as a button's.
    #[serde(default)]
    pub emoji: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Style {
    Primary,
    Secondary,
    Success,
    Danger,
    Link,
    Premium,
}

/// A message's components as cards, and their pictures and files as
/// attachments, placed by slot.
#[derive(Default)]
pub struct Parsed {
    pub cards: Vec<Card>,
    pub media: Vec<Attachment>,
}

pub fn parse(components: &Value) -> Parsed {
    let mut parsed = Parsed::default();
    let mut loose = Card::default();
    for component in components.as_array().into_iter().flatten() {
        if component["type"].as_u64() == Some(CONTAINER) {
            if !loose.rows.is_empty() {
                parsed.cards.push(std::mem::take(&mut loose));
            }
            let spoiler = component["spoiler"].as_bool().unwrap_or(false);
            let mut card = Card {
                framed: true,
                color: component["accent_color"]
                    .as_u64()
                    .and_then(|color| u32::try_from(color).ok()),
                rows: Vec::new(),
            };
            for inner in component["components"].as_array().into_iter().flatten() {
                parsed.row(inner, spoiler, &mut card.rows);
            }
            parsed.cards.push(card);
        } else {
            parsed.row(component, false, &mut loose.rows);
        }
    }
    if !loose.rows.is_empty() {
        parsed.cards.push(loose);
    }
    parsed
}

impl Parsed {
    /// The slot the next row's media takes.
    fn slot(&self) -> usize {
        self.media
            .iter()
            .filter_map(|media| match media.shown {
                Shown::Component(slot) => Some(slot + 1),
                _ => None,
            })
            .max()
            .unwrap_or_default()
    }

    /// Adds `component`'s row to `rows`, unless Yapfast can't show it.
    fn row(&mut self, component: &Value, spoiler: bool, rows: &mut Vec<Row>) {
        let row = match component["type"].as_u64().unwrap_or_default() {
            TEXT_DISPLAY => text(component).map(|text| Row::Text { text, beside: None }),
            SECTION => {
                let text = component["components"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let accessory = &component["accessory"];
                let beside = match accessory["type"].as_u64() {
                    Some(THUMBNAIL) => {
                        let slot = self.slot();
                        let spoiler = spoiler || accessory["spoiler"].as_bool().unwrap_or(false);
                        let size = (THUMBNAIL_SIZE, THUMBNAIL_SIZE);
                        media(&accessory["media"], slot, size, spoiler).map(|mut picture| {
                            picture.description = alt_text(&accessory["description"]);
                            self.media.push(picture);
                            Beside::Thumbnail(slot)
                        })
                    }
                    Some(BUTTON) => button(accessory).map(Beside::Button),
                    _ => None,
                };
                (!text.is_empty() || beside.is_some()).then_some(Row::Text { text, beside })
            }
            MEDIA_GALLERY => {
                let slot = self.slot();
                let pictures: Vec<_> = component["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|item| {
                        let spoiler = spoiler || item["spoiler"].as_bool().unwrap_or(false);
                        let mut picture = media(
                            &item["media"],
                            slot,
                            (PREVIEW_WIDTH, PREVIEW_HEIGHT),
                            spoiler,
                        )?;
                        picture.description = alt_text(&item["description"]);
                        Some(picture)
                    })
                    .collect();
                (!pictures.is_empty()).then(|| {
                    self.media.extend(pictures);
                    Row::Media(slot)
                })
            }
            FILE => {
                let slot = self.slot();
                let spoiler = spoiler || component["spoiler"].as_bool().unwrap_or(false);
                file(component, slot, spoiler).map(|file| {
                    self.media.push(file);
                    Row::Media(slot)
                })
            }
            SEPARATOR => Some(Row::Separator {
                divider: component["divider"].as_bool().unwrap_or(true),
                large: component["spacing"].as_u64() == Some(LARGE),
            }),
            ACTION_ROW => {
                let inner = component["components"].as_array();
                let first = inner.and_then(|inner| inner.first());
                match first.and_then(|first| first["type"].as_u64()) {
                    Some(BUTTON) => {
                        let buttons: Vec<_> =
                            inner.into_iter().flatten().filter_map(button).collect();
                        (!buttons.is_empty()).then_some(Row::Buttons(buttons))
                    }
                    Some(
                        STRING_SELECT | USER_SELECT | ROLE_SELECT | MENTIONABLE_SELECT
                        | CHANNEL_SELECT,
                    ) => first.map(select),
                    _ => None,
                }
            }
            // Content inventory entries, checkpoint cards and kinds newer
            // than Yapfast, told of once for a run of them.
            _ => (rows.last() != Some(&Row::Unknown)).then_some(Row::Unknown),
        };
        rows.extend(row);
    }
}

/// A text display's Markdown, when it has any.
fn text(component: &Value) -> Option<String> {
    if component["type"].as_u64() != Some(TEXT_DISPLAY) {
        return None;
    }
    component["content"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

/// A button as it shows. Premium buttons name a purchase in Discord's store.
fn button(component: &Value) -> Option<Button> {
    let style = match component["style"].as_u64()? {
        1 => Style::Primary,
        2 => Style::Secondary,
        3 => Style::Success,
        4 => Style::Danger,
        5 => Style::Link,
        6 => Style::Premium,
        _ => return None,
    };
    let emoji = emoji(&component["emoji"]);
    let label = component["label"].as_str().unwrap_or_default().to_owned();
    if label.is_empty() && emoji.is_none() && style != Style::Premium {
        return None;
    }
    Some(Button {
        label,
        emoji,
        style,
        url: component["url"]
            .as_str()
            .filter(|_| style == Style::Link)
            .map(str::to_owned),
        custom_id: component["custom_id"].as_str().map(str::to_owned),
        sku_id: component["sku_id"].as_str().map(str::to_owned),
        disabled: component["disabled"].as_bool().unwrap_or(false),
    })
}

/// A partial emoji object in Discord's markup: the emoji itself, or
/// `<:name:id>`.
fn emoji(emoji: &Value) -> Option<String> {
    match (emoji["id"].as_str(), emoji["name"].as_str()) {
        (Some(id), name) => {
            let prefix = if emoji["animated"].as_bool().unwrap_or(false) {
                "a"
            } else {
                ""
            };
            Some(format!("<{prefix}:{}:{id}>", name.unwrap_or("emoji")))
        }
        (None, Some(name)) => Some(name.to_owned()),
        (None, None) => None,
    }
}

/// A select menu, by the options it picks by default or its placeholder.
fn select(component: &Value) -> Row {
    let options: Vec<SelectOption> = component["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| {
            Some(SelectOption {
                label: option["label"].as_str()?.to_owned(),
                value: option["value"].as_str()?.to_owned(),
                description: option["description"].as_str().map(str::to_owned),
                default: option["default"].as_bool().unwrap_or(false),
                emoji: emoji(&option["emoji"]),
            })
        })
        .collect();
    // As Discord shows the options picked: each with its emoji.
    let picked: Vec<String> = options
        .iter()
        .filter(|option| option.default)
        .map(|option| match &option.emoji {
            Some(emoji) => format!("{emoji} {}", option.label),
            None => option.label.clone(),
        })
        .collect();
    let text = if picked.is_empty() {
        component["placeholder"]
            .as_str()
            .unwrap_or("Make a selection")
            .to_owned()
    } else {
        picked.join(", ")
    };
    Row::Select {
        text,
        disabled: component["disabled"].as_bool().unwrap_or(false),
        kind: component["type"].as_u64().unwrap_or_default(),
        custom_id: component["custom_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        options,
        defaults: component["default_values"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value["id"].as_str().map(str::to_owned))
            .collect(),
        channel_types: component["channel_types"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect(),
        min_values: component["min_values"].as_u64().unwrap_or(1),
        max_values: component["max_values"].as_u64().unwrap_or(1),
    }
}

/// A picture or video of a gallery or thumbnail, from its unfurled media
/// item, fitted into `size`. Discord's proxy must have it.
fn media(item: &Value, slot: usize, size: (u64, u64), spoiler: bool) -> Option<Attachment> {
    let url = item["url"].as_str()?;
    item["proxy_url"].as_str()?;
    let video = item["content_type"]
        .as_str()
        .is_some_and(|kind| kind.starts_with("video/"));
    let side = |name: &str| u32::try_from(item[name].as_u64()?).ok();
    Some(Attachment {
        name: crate::embed::file_name(url).unwrap_or("image").to_owned(),
        size: None,
        url: url.into(),
        image: !video,
        dimensions: side("width").zip(side("height")),
        // A video's poster is its first frame alone.
        preview_url: scaled_url(item, size.0, size.1, !video),
        view_url: (!video)
            .then(|| scaled_url(item, VIEW_WIDTH, VIEW_HEIGHT, true))
            .flatten(),
        still_url: None,
        video_url: video.then(|| url.to_owned()),
        link_url: None,
        animated: !video && animated(item),
        shown: Shown::Component(slot),
        spoiler: spoiler || item["flags"].as_u64().unwrap_or_default() & IS_SPOILER != 0,
        description: None,
    })
}

/// A file component's attachment, which shows as a file card.
fn file(component: &Value, slot: usize, spoiler: bool) -> Option<Attachment> {
    let url = component["file"]["url"].as_str()?;
    let name = component["name"]
        .as_str()
        .or_else(|| crate::embed::file_name(url))
        .unwrap_or("file");
    Some(Attachment {
        name: name.to_owned(),
        size: component["size"].as_u64(),
        url: url.into(),
        image: false,
        dimensions: None,
        preview_url: None,
        view_url: None,
        still_url: None,
        video_url: None,
        link_url: None,
        animated: false,
        shown: Shown::Component(slot),
        spoiler,
        description: None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn picture(name: &str) -> Value {
        json!({
            "url": format!("https://cdn.discordapp.com/attachments/1/2/{name}"),
            "proxy_url": format!("https://media.discordapp.net/attachments/1/2/{name}"),
            "width": 800, "height": 600, "content_type": "image/png",
        })
    }

    #[test]
    fn channel_select_preserves_its_type_filter() {
        let row = select(
            &serde_json::json!({"type": 8, "custom_id": "channel", "channel_types": [0, 5]}),
        );
        assert!(matches!(row, Row::Select { channel_types, .. } if channel_types == [0, 5]));
    }

    #[test]
    fn containers_become_framed_cards_and_loose_components_share_one() {
        let parsed = parse(&json!([
            {"type": 10, "content": "Above"},
            {"type": 17, "accent_color": 0x5865f2, "components": [
                {"type": 10, "content": "# Hello"},
                {"type": 14, "divider": false, "spacing": 2},
                {"type": 9, "components": [{"type": 10, "content": "One"}, {"type": 10, "content": "Two"}],
                 "accessory": {"type": 11, "media": picture("thumb.png")}},
                {"type": 12, "items": [{"media": picture("a.png")}, {"media": picture("b.png"), "spoiler": true}]},
                {"type": 13, "file": {"url": "https://cdn.discordapp.com/attachments/1/2/notes.txt"},
                 "name": "notes.txt", "size": 12},
                {"type": 1, "components": [
                    {"type": 2, "style": 5, "label": "Read more", "url": "https://discord.com/blog"},
                    {"type": 2, "style": 1, "label": "Accept", "custom_id": "yes", "emoji": {"name": "👍"}},
                    {"type": 2, "style": 6, "sku_id": "1"},
                ]},
            ]},
            {"type": 20},
            {"type": 21},
            {"type": 1, "components": [{"type": 3, "custom_id": "pick", "placeholder": "Pick one",
                                         "options": [{"label": "A", "value": "a"}]}]},
        ]));
        assert_eq!(parsed.cards.len(), 3);
        let (above, card, below) = (&parsed.cards[0], &parsed.cards[1], &parsed.cards[2]);
        assert!(!above.framed && !below.framed && card.framed);
        assert_eq!(card.color, Some(0x5865f2));
        assert_eq!(
            card.rows[..3],
            [
                Row::Text {
                    text: "# Hello".into(),
                    beside: None
                },
                Row::Separator {
                    divider: false,
                    large: true
                },
                Row::Text {
                    text: "One\nTwo".into(),
                    beside: Some(Beside::Thumbnail(0))
                },
            ]
        );
        assert_eq!(card.rows[3..5], [Row::Media(1), Row::Media(2)]);
        let Row::Buttons(buttons) = &card.rows[5] else {
            panic!("a row of buttons");
        };
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[0].url.as_deref(), Some("https://discord.com/blog"));
        assert_eq!(buttons[1].emoji.as_deref(), Some("👍"));
        assert_eq!(buttons[1].url, None);
        assert_eq!(buttons[1].custom_id.as_deref(), Some("yes"));
        assert_eq!(buttons[2].style, Style::Premium);
        assert_eq!(buttons[2].sku_id.as_deref(), Some("1"));
        // Two kinds Yapfast doesn't know are told of once.
        assert_eq!(
            below.rows,
            [
                Row::Unknown,
                Row::Select {
                    text: "Pick one".into(),
                    disabled: false,
                    kind: 3,
                    custom_id: "pick".into(),
                    options: vec![SelectOption {
                        label: "A".into(),
                        value: "a".into(),
                        description: None,
                        default: false,
                        emoji: None,
                    }],
                    defaults: vec![],
                    channel_types: vec![],
                    min_values: 1,
                    max_values: 1,
                }
            ]
        );
        let media: Vec<_> = parsed
            .media
            .iter()
            .map(|media| (media.name.as_str(), media.shown, media.spoiler))
            .collect();
        assert_eq!(
            media,
            [
                ("thumb.png", Shown::Component(0), false),
                ("a.png", Shown::Component(1), false),
                ("b.png", Shown::Component(1), true),
                ("notes.txt", Shown::Component(2), false),
            ]
        );
        assert!(parsed.media[0].preview_url.is_some());
        assert_eq!(parsed.media[3].preview_url, None);
        assert_eq!(parsed.media[3].size, Some(12));
    }

    #[test]
    fn options_keep_their_emoji_and_pictures_their_alt_text() {
        let parsed = parse(&json!([
            {"type": 9, "components": [{"type": 10, "content": "Build"}],
             "accessory": {"type": 11, "media": picture("t.png"), "description": "A green tick"}},
            {"type": 12, "items": [{"media": picture("a.png"), "description": " Before "},
                                   {"media": picture("b.png"), "description": ""}]},
            {"type": 1, "components": [{"type": 3, "custom_id": "env", "options": [
                {"label": "Staging", "value": "s", "emoji": {"name": "🧪"}, "default": true},
                {"label": "Live", "value": "l", "emoji": {"id": "5", "name": "live", "animated": true}},
            ]}]},
        ]));
        let alt: Vec<_> = parsed
            .media
            .iter()
            .map(|media| media.description.as_deref())
            .collect();
        assert_eq!(alt, [Some("A green tick"), Some("Before"), None]);
        let Row::Select { text, options, .. } = &parsed.cards[0].rows[2] else {
            panic!("a select menu");
        };
        assert_eq!(text, "🧪 Staging");
        assert_eq!(options[0].emoji.as_deref(), Some("🧪"));
        assert_eq!(options[1].emoji.as_deref(), Some("<a:live:5>"));
    }

    #[test]
    fn a_spoiler_container_covers_its_pictures() {
        let parsed = parse(&json!([
            {"type": 17, "spoiler": true, "components": [
                {"type": 12, "items": [{"media": picture("a.png")}]},
            ]},
        ]));
        assert!(parsed.media[0].spoiler);
    }

    #[test]
    fn videos_play_from_their_address_with_a_poster() {
        let parsed = parse(&json!([
            {"type": 12, "items": [{"media": {
                "url": "https://cdn.discordapp.com/attachments/1/2/clip.mp4",
                "proxy_url": "https://media.discordapp.net/attachments/1/2/clip.mp4",
                "width": 1280, "height": 720, "content_type": "video/mp4"}}]},
        ]));
        let video = &parsed.media[0];
        assert!(!video.image);
        assert_eq!(
            video.video_url.as_deref(),
            Some("https://cdn.discordapp.com/attachments/1/2/clip.mp4")
        );
        assert!(
            video
                .preview_url
                .as_deref()
                .unwrap()
                .contains("format=webp")
        );
    }
}
