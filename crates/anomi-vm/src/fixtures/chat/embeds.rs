//! Samples of a message's embeds for tests and shots, after the fake
//! Discord's data: task C05's.

use iced::Color;

use crate::chat::embeds::{Card, Cell, Embeds, Field, Gallery, Thumbnail};
use crate::chat::timeline::group::key;
use crate::fixtures::pictures;
use crate::fixtures::text as texts;
use crate::kind::Expected;
use crate::text::{Builder, Link, Source, Style, Text, markdown};

/// Plain text as an embed shows it: Markdown, parsed.
fn words(text: &str) -> Text {
    markdown(Source::new(text))
}

/// A title in bold, opening `url`.
fn title(text: &str, url: &str) -> Text {
    let mut builder = Builder::new();
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    builder.link(text, bold, Link::Url(url.into()), None);
    builder.finish()
}

fn field(name: &str, value: Text) -> Field {
    Field {
        name: words(&format!("**{name}**")),
        value,
    }
}

/// The cards of the message `id` (its key's source, as `showcase` has it).
fn embeds(id: &str, cards: Vec<Card>) -> Embeds {
    Embeds {
        item: key(id),
        cards,
    }
}

/// Bob's article, as the fake previews it (`articleEmbed`): a bar of the
/// site's blue, its name, the title and description, its wide picture
/// under them.
pub fn article() -> Embeds {
    embeds(
        "article",
        vec![Card {
            color: Some(Color::from_rgb8(0x2b, 0x6c, 0xb0)),
            provider: Some(words("Example News")),
            title: Some(texts::bridges_title()),
            url: "https://example.com/news/bridges".into(),
            description: Some(texts::bridges()),
            gallery: Some(Gallery {
                width: 360.0,
                height: 189.0,
                video: false,
                cells: vec![Cell {
                    width: 360.0,
                    height: 189.0,
                    picture: Expected::Ready(pictures::picture("/embeds/bridges.png")),
                    name: "bridge".into(),
                    ..Cell::default()
                }],
            }),
            ..Card::default()
        }],
    )
}

/// The status bot's card: green bar, a title that opens its page, a
/// heading, text and code, three fields in a row, then Notes alone, its
/// picture, its footer.
pub fn status() -> Embeds {
    let row = vec![
        field("Uptime", words("**99.98%**")),
        field("Players", words("42")),
        field("Region", words("Europe")),
    ];
    embeds(
        "status",
        vec![Card {
            color: Some(Color::from_rgb8(0x57, 0xf2, 0x87)),
            title: Some(title("Server status", "https://status.example.com")),
            url: "https://status.example.com".into(),
            description: Some(texts::status()),
            rows: vec![row, vec![field("Notes", texts::notes())]],
            footer: Some(words("Status Bot \u{2022} 17 Nov 2026")),
            thumbnail: Some(Thumbnail {
                picture: Expected::Ready(pictures::picture("/embeds/status.png")),
                index: 0,
            }),
            ..Card::default()
        }],
    )
}

/// A YouTube link's card: red bar, provider, author, the video's title and
/// description, its poster with a play button (`youtube_gif_embed`).
pub fn youtube() -> Embeds {
    let url = "https://www.youtube.com/watch?v=talk";
    embeds(
        "talk",
        vec![Card {
            color: Some(Color::from_rgb8(0xff, 0x00, 0x00)),
            provider: Some(words("YouTube")),
            author: Some(words("**Example City**")),
            title: Some(title("Building a bike bridge in 90 seconds", url)),
            url: url.into(),
            description: Some(words("A time-lapse of the whole build.")),
            gallery: Some(Gallery {
                width: 360.0,
                height: 202.0,
                video: true,
                cells: vec![Cell {
                    width: 360.0,
                    height: 202.0,
                    picture: Expected::Ready(pictures::sunset(360, 202)),
                    name: "poster".into(),
                    ..Cell::default()
                }],
            }),
            ..Card::default()
        }],
    )
}

/// A card of an image alone, one of its pictures a spoiler, and one still
/// on its way.
pub fn gallery() -> Embeds {
    let cell = |x: f32, index: usize, picture: Expected, spoiler: bool| Cell {
        x,
        width: 176.0,
        height: 132.0,
        index,
        picture,
        spoiler,
        name: format!("picture {index}"),
        ..Cell::default()
    };
    embeds(
        "photos",
        vec![Card {
            title: Some(title("Photos", "https://example.com/photos")),
            url: "https://example.com/photos".into(),
            gallery: Some(Gallery {
                width: 360.0,
                height: 132.0,
                video: false,
                cells: vec![
                    cell(0.0, 0, Expected::Ready(pictures::sunset(176, 132)), true),
                    cell(184.0, 1, Expected::Waiting, false),
                ],
            }),
            ..Card::default()
        }],
    )
}

/// #general with Bob's link and the status bot's card under it
/// (`chan/bot_embed_card_poll`), or a YouTube link and a card of pictures
/// (`chan/youtube_gif_embed`), one message each, for the shots.
pub fn showcase(video: bool) -> crate::chat::timeline::Timeline {
    use crate::chat::message::{Author, Message};
    use crate::chat::timeline::Row;
    use crate::fixtures::chat::message::{article as link, bob, command};
    let one = |id: &str, author: Author, body: &str, embeds: Embeds| {
        let message = Message {
            key: key(id),
            first: true,
            author,
            time: "01:10".into(),
            short_time: "01:10".into(),
            body: words(body),
            embeds,
            ..Message::default()
        };
        Row {
            key: message.key,
            first: true,
            message,
            ..Row::default()
        }
    };
    let bot = Author {
        name: "Status Bot".into(),
        ..command().author
    };
    let rows = if video {
        let said = "This one too https://www.youtube.com/watch?v=talk";
        vec![
            one("talk", bob(), said, youtube()),
            one("photos", bob(), "", gallery()),
        ]
    } else {
        let first = Message {
            embeds: article(),
            ..link()
        };
        vec![
            Row {
                key: first.key,
                first: true,
                message: first,
                ..Row::default()
            },
            one("status", bot, "", status()),
        ]
    };
    crate::chat::timeline::Timeline {
        rows,
        shown: None,
        ..super::timeline::general()
    }
}
