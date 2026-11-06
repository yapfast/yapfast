//! Discord's embeds: the link previews Discord makes from a page's Open Graph
//! tags, and the cards bots post. Their pictures come through Discord's media
//! proxy, so showing them never contacts the linked site. See
//! <https://docs.discord.food/resources/message#embed-object>.

use serde_json::Value;

use crate::message::{Attachment, Shown, VIEW_HEIGHT, VIEW_WIDTH, date, iso_ms, scaled_url};

/// The size thumbnails beside an embed's text are downloaded at: twice what
/// they show at, for high-DPI displays.
const THUMBNAIL_SIZE: u64 = 160;

/// An embed as a card. Its pictures are among the message's attachments,
/// placed with [`Shown::Embed`] and [`Shown::Thumbnail`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Embed {
    /// The RGB colour of the bar down its side.
    pub color: Option<u32>,
    /// The site, such as "YouTube".
    pub provider: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    /// Where the title leads.
    pub url: Option<String>,
    /// Discord Markdown with mentions still as `<@id>`.
    pub description: Option<String>,
    pub fields: Vec<Field>,
    /// The footer, with the embed's date after it.
    pub footer: Option<String>,
    /// A video's: its large picture is the video's poster, which opens the
    /// video at `url` rather than in the image viewer.
    pub video: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Field {
    pub name: String,
    /// Discord Markdown.
    pub value: String,
    /// Shares a row with the inline fields beside it.
    pub inline: bool,
}

/// A text field that may be missing, null or blank.
fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

/// The most an embed's picture shows at; Discord keeps these smaller than an
/// attached picture.
const EMBED_WIDTH: u64 = 360;
const EMBED_HEIGHT: u64 = 270;

/// A picture of an embed as an attachment, when Discord's proxy has it.
fn picture(value: &Value, shown: Shown, thumbnail: bool) -> Option<Attachment> {
    let proxy = value["proxy_url"].as_str()?;
    let side = |name: &str| u32::try_from(value[name].as_u64()?).ok();
    let dimensions = side("width").zip(side("height"));
    let (width, height) = if thumbnail {
        (THUMBNAIL_SIZE, THUMBNAIL_SIZE)
    } else {
        (EMBED_WIDTH, EMBED_HEIGHT)
    };
    // A video's poster is its first frame alone.
    let animated = !matches!(shown, Shown::Poster(_));
    let preview_url = scaled_url(value, width, height, animated)?;
    let name = file_name(value["url"].as_str().unwrap_or(proxy)).unwrap_or("image");
    Some(Attachment {
        name: name.to_owned(),
        size: None,
        url: proxy.into(),
        image: !matches!(shown, Shown::Poster(_)),
        dimensions,
        preview_url: Some(preview_url),
        view_url: scaled_url(value, VIEW_WIDTH, VIEW_HEIGHT, animated),
        still_url: None,
        video_url: None,
        link_url: None,
        animated: crate::message::animated(value),
        shown,
        spoiler: false,
    })
}

/// A GIF link's picture, which plays from the GIF's video. Tenor, Giphy
/// and others send GIFs as MP4s, which Yapfast shows as GIFs, as Discord does:
/// the media proxy serves them whole. The picture, a still, stands in when
/// the video can't play. The file is the video, by its own name. Without a
/// video the proxy has, the picture plays, if it moves.
fn gif(picture: &mut Attachment, embed: &Value) {
    picture.link_url = embed["url"].as_str().map(Into::into);
    picture.animated = true;
    let video = &embed["video"];
    if let Some(proxy) = video["proxy_url"].as_str() {
        picture.still_url = picture.preview_url.replace(proxy.to_owned());
        picture.view_url = Some(proxy.to_owned());
        picture.url = proxy.into();
        if let Some(name) = file_name(video["url"].as_str().unwrap_or(proxy)) {
            picture.name = name.to_owned();
        }
    }
}

/// The last part of an address's path, such as `cat.png`.
pub(crate) fn file_name(url: &str) -> Option<&str> {
    url.split(['?', '#'])
        .next()
        .and_then(|path| path.rsplit('/').next())
        .filter(|name| !name.is_empty())
}

/// A message's embeds as cards, and their pictures as attachments to add
/// after the message's own. An image or GIF linked on its own shows like an
/// attached one, without a card; `hides` is that link, which Discord leaves
/// out of the message's text when it's all the text there is.
pub struct Parsed {
    pub embeds: Vec<Embed>,
    pub pictures: Vec<Attachment>,
    pub hides: Vec<String>,
}

pub fn parse(embeds: &Value) -> Parsed {
    let mut parsed = Parsed {
        embeds: Vec::new(),
        pictures: Vec::new(),
        hides: Vec::new(),
    };
    for embed in embeds.as_array().into_iter().flatten() {
        let kind = embed["type"].as_str().unwrap_or("rich");
        let url = text(&embed["url"]);
        match kind {
            // A linked image or GIF: Discord puts the picture in the thumbnail.
            "image" | "gifv" => {
                let source = if embed["thumbnail"].is_object() {
                    &embed["thumbnail"]
                } else {
                    &embed["image"]
                };
                if let Some(mut picture) = picture(source, Shown::Message, false) {
                    if kind == "gifv" {
                        gif(&mut picture, embed);
                    } else if picture.animated {
                        // A moving picture is kept among the favourites by
                        // its own address, as Discord's client keeps it.
                        picture.link_url = source["url"]
                            .as_str()
                            .or(embed["url"].as_str())
                            .map(Into::into);
                    }
                    parsed.pictures.push(picture);
                    parsed.hides.extend(url);
                }
                continue;
            }
            "rich" | "article" | "link" | "video" => {}
            // Automod notices, poll results and the like.
            _ => continue,
        }
        // A post with several pictures comes as one embed per picture, the
        // later ones only a picture under the same address; they share a card.
        let joins = parsed.embeds.last().filter(|last| {
            url.is_some()
                && last.url == url
                && text(&embed["title"]).is_none()
                && text(&embed["description"]).is_none()
        });
        if joins.is_some() {
            let index = parsed.embeds.len() - 1;
            parsed
                .pictures
                .extend(picture(&embed["image"], Shown::Embed(index), false));
            continue;
        }
        let index = parsed.embeds.len();
        // A video file the media proxy has plays in Yapfast, whatever the
        // embed's kind: vxtwitter's posts come as rich cards with one. Other
        // videos, such as YouTube's, open in the browser.
        let playable = embed["video"]["proxy_url"].as_str();
        let video = embed["video"].is_object() && (kind == "video" || playable.is_some());
        // Articles and videos show their thumbnail large, as Discord does.
        let large_thumbnail = video || kind == "article";
        let image = embed["image"].is_object();
        if video {
            // A video's poster is its thumbnail, or its image without one.
            let source = if embed["thumbnail"].is_object() || !image {
                &embed["thumbnail"]
            } else {
                &embed["image"]
            };
            let mut poster = picture(source, Shown::Poster(index), false);
            if let Some(poster) = poster.as_mut() {
                poster.video_url = playable.map(str::to_owned);
            }
            parsed.pictures.extend(poster);
        } else if image {
            parsed
                .pictures
                .extend(picture(&embed["image"], Shown::Embed(index), false));
        } else if large_thumbnail {
            parsed
                .pictures
                .extend(picture(&embed["thumbnail"], Shown::Embed(index), false));
        }
        if !large_thumbnail || (image && !video) {
            parsed
                .pictures
                .extend(picture(&embed["thumbnail"], Shown::Thumbnail(index), true));
        }
        let footer = text(&embed["footer"]["text"]);
        let when = embed["timestamp"].as_str().and_then(iso_ms).map(date);
        parsed.embeds.push(Embed {
            color: embed["color"]
                .as_u64()
                .and_then(|color| u32::try_from(color).ok()),
            provider: text(&embed["provider"]["name"]),
            author: text(&embed["author"]["name"]),
            title: text(&embed["title"]),
            url,
            description: text(&embed["description"]),
            fields: embed["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|field| {
                    Some(Field {
                        name: field["name"].as_str()?.to_owned(),
                        value: field["value"].as_str()?.to_owned(),
                        inline: field["inline"].as_bool().unwrap_or(false),
                    })
                })
                .collect(),
            footer: match (footer, when) {
                (Some(footer), Some(when)) => Some(format!("{footer} • {when}")),
                (footer, when) => footer.or(when),
            },
            video,
        });
    }
    parsed
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn proxied(name: &str, width: u64, height: u64) -> Value {
        json!({
            "url": format!("https://example.com/{name}"),
            "proxy_url": format!("https://images-ext-1.discordapp.net/external/x/https/example.com/{name}"),
            "width": width,
            "height": height,
        })
    }

    #[test]
    fn link_previews_become_cards_with_their_pictures() {
        let parsed = parse(&json!([
            {"type": "article", "url": "https://example.com/news", "title": "News",
             "description": "What happened", "color": 0x123456,
             "provider": {"name": "Example"}, "thumbnail": proxied("news.jpg", 1200, 630)},
            {"type": "link", "url": "https://example.com/about", "title": "About",
             "thumbnail": proxied("logo.png", 64, 64)},
            {"type": "video", "url": "https://youtube.com/watch?v=1", "title": "A video",
             "provider": {"name": "YouTube"}, "author": {"name": "A channel"},
             "thumbnail": proxied("poster.jpg", 1280, 720),
             "video": {"url": "https://youtube.com/embed/1", "width": 1280, "height": 720}},
            {"type": "gifv", "url": "https://tenor.com/view/cat", "thumbnail": proxied("cat.png", 498, 280),
             "video": proxied("cat.mp4", 498, 280)},
            {"type": "poll_result", "fields": []},
        ]));
        assert_eq!(parsed.embeds.len(), 3);
        assert_eq!(parsed.embeds[0].provider.as_deref(), Some("Example"));
        assert_eq!(parsed.embeds[0].color, Some(0x123456));
        assert!(parsed.embeds[2].video);
        let shown: Vec<_> = parsed
            .pictures
            .iter()
            .map(|picture| (picture.name.as_str(), picture.shown, picture.image))
            .collect();
        assert_eq!(
            shown,
            [
                ("news.jpg", Shown::Embed(0), true),
                ("logo.png", Shown::Thumbnail(1), true),
                ("poster.jpg", Shown::Poster(2), false),
                ("cat.mp4", Shown::Message, true),
            ]
        );
        assert!(
            parsed.pictures[0]
                .preview_url
                .as_deref()
                .unwrap()
                .starts_with("https://images-ext-1.discordapp.net/")
        );
        assert_eq!(
            parsed.pictures[2].video_url, None,
            "YouTube's opens in the browser"
        );
        // The GIF plays from its MP4, its picture standing in if it can't,
        // and saves as the MP4.
        let gif = &parsed.pictures[3];
        let mp4 = "https://images-ext-1.discordapp.net/external/x/https/example.com/cat.mp4";
        assert_eq!(gif.preview_url.as_deref(), Some(mp4));
        assert_eq!(gif.view_url.as_deref(), Some(mp4));
        assert!(gif.still_url.as_deref().unwrap().contains("cat.png?"));
        assert_eq!((gif.name.as_str(), &*gif.url), ("cat.mp4", mp4));
        assert_eq!(gif.link_url.as_deref(), Some("https://tenor.com/view/cat"));
        assert!(gif.animated);
        assert_eq!(parsed.hides, ["https://tenor.com/view/cat"]);
    }

    #[test]
    fn linked_pictures_keep_their_link_when_they_move() {
        let animated = |name: &str, flags: u64| {
            let mut picture = proxied(name, 220, 165);
            picture["flags"] = json!(flags);
            json!({"type": "image", "url": format!("https://example.com/{name}"), "thumbnail": picture})
        };
        let parsed = parse(&json!([
            animated("wave.webp", 32),
            animated("still.png", 0)
        ]));
        let shown: Vec<_> = parsed
            .pictures
            .iter()
            .map(|picture| (picture.animated, picture.link_url.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [(true, Some("https://example.com/wave.webp")), (false, None)]
        );
        assert_eq!(
            parsed.hides,
            [
                "https://example.com/wave.webp",
                "https://example.com/still.png"
            ]
        );
    }

    #[test]
    fn rich_cards_with_a_proxied_video_play_it() {
        // vxtwitter's video posts.
        let parsed = parse(&json!([
            {"type": "rich", "url": "https://vxtwitter.com/a/status/1", "color": 1942002,
             "author": {"name": "A (@a)"}, "description": "?",
             "footer": {"text": "vxTwitter / fixvx"},
             "thumbnail": proxied("thumb.jpg", 1200, 675),
             "video": proxied("tvid/1280x720/clip", 1280, 720)},
            {"type": "rich", "url": "https://example.com/b", "title": "Elsewhere",
             "thumbnail": proxied("logo.png", 64, 64),
             "video": {"url": "https://example.com/player", "width": 640, "height": 360}},
        ]));
        assert!(parsed.embeds[0].video && !parsed.embeds[1].video);
        let poster = &parsed.pictures[0];
        assert_eq!(
            (poster.name.as_str(), poster.shown, poster.image),
            ("thumb.jpg", Shown::Poster(0), false)
        );
        assert_eq!(
            poster.video_url.as_deref(),
            Some(
                "https://images-ext-1.discordapp.net/external/x/https/example.com/tvid/1280x720/clip"
            )
        );
        // A video the proxy hasn't leaves the card as it was.
        assert_eq!(parsed.pictures[1].shown, Shown::Thumbnail(1));
    }

    #[test]
    fn moving_pictures_come_animated() {
        // vxtwitter's GIFs: an animated AVIF, which Discord's proxy sends as
        // an animated WebP when asked, as its client asks.
        let proxy = "https://images-ext-1.discordapp.net/external/x/https/gifconvert.vxtwitter.com/convert.avif";
        let parsed = parse(&json!([
            {"type": "image", "url": "https://gifconvert.vxtwitter.com/convert.avif?url=1",
             "thumbnail": {"url": "https://gifconvert.vxtwitter.com/convert.avif?url=1",
                           "proxy_url": proxy, "width": 400, "height": 346}},
        ]));
        let picture = &parsed.pictures[0];
        assert_eq!(
            picture.preview_url.as_deref(),
            Some(format!("{proxy}?animated=true&format=webp&width=312&height=270").as_str())
        );
        assert!(
            picture
                .view_url
                .as_deref()
                .unwrap()
                .contains("animated=true")
        );
        // A video's poster is one frame.
        let parsed = parse(&json!([
            {"type": "video", "url": "https://example.com/v",
             "thumbnail": {"url": "https://example.com/p.jpg", "proxy_url": proxy, "width": 400, "height": 346},
             "video": {"url": "https://example.com/v.mp4"}},
        ]));
        assert!(
            !parsed.pictures[0]
                .preview_url
                .as_deref()
                .unwrap()
                .contains("animated")
        );
    }

    #[test]
    fn bot_cards_keep_their_fields_footer_and_shared_pictures() {
        let parsed = parse(&json!([
            {"type": "rich", "url": "https://example.com/post", "title": "Post",
             "fields": [{"name": "Score", "value": "**3**", "inline": true},
                        {"name": "Notes", "value": "none"}],
             "footer": {"text": "Bot"}, "timestamp": "2026-10-06T12:00:00+00:00",
             "image": proxied("one.png", 800, 600)},
            {"type": "rich", "url": "https://example.com/post", "image": proxied("two.png", 800, 600)},
        ]));
        assert_eq!(
            parsed.embeds.len(),
            1,
            "the second picture joins the first card"
        );
        let card = &parsed.embeds[0];
        assert_eq!(card.fields.len(), 2);
        assert!(card.fields[0].inline && !card.fields[1].inline);
        assert_eq!(card.footer.as_deref(), Some("Bot • 6 Oct 2026"));
        assert!(
            parsed
                .pictures
                .iter()
                .all(|picture| picture.shown == Shown::Embed(0))
        );
        assert_eq!(parsed.pictures.len(), 2);
    }
}
