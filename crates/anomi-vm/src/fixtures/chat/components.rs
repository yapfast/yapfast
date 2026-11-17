//! Samples of a bot message's components for tests and shots, after the fake
//! Discord's `control components` messages: the "Discord" app's notice in a
//! container (`dm/discord_components_notice`), and the status bot's deploy
//! message, its controls outside one. Task C06's.

use iced::Color;

use crate::chat::attachments::{self, Attachments, Cell, Gallery};
use crate::chat::components::{Button, Card, Components, Picture, Row, Select, Style};
use crate::chat::message::{Author, Message};
use crate::chat::timeline::group::key;
use crate::chat::timeline::{Kind as RoomKind, Row as TimelineRow, Timeline};
use crate::fixtures::avatar;
use crate::fixtures::chat::message::bobs;
use crate::fixtures::chat::timeline as rows;
use crate::fixtures::pictures::{pictured, sunset};
use crate::kind::Expected;
use crate::text::{Kind, Section, Source, Text, markdown};

/// `markdown` split as the core splits it: each of `blocks` a section of
/// its kind.
fn text(source: &str, blocks: &[(Kind, &str)]) -> Text {
    let sections: Vec<Section> = blocks
        .iter()
        .map(|(kind, text)| {
            let start = source.find(text).unwrap_or_default();
            Section {
                kind: *kind,
                quoted: false,
                runs: std::iter::once(start..start + text.len()).collect(),
            }
        })
        .collect();
    markdown(Source {
        sections: Some(&sections),
        ..Source::new(source)
    })
}

/// Text alone.
fn plain(source: &str) -> Text {
    markdown(Source::new(source))
}

fn button(label: &str, style: Style, custom_id: &str) -> Button {
    Button {
        label: label.into(),
        style,
        custom_id: custom_id.into(),
        enabled: true,
        ..Button::default()
    }
}

fn link(label: &str, url: &str) -> Button {
    Button {
        url: url.into(),
        ..button(label, Style::Link, "")
    }
}

/// The notice's container: a heading, a banner, a divider, a section with
/// its thumbnail and one with its link button, a large gap, subtext and
/// buttons.
pub fn notice_card(item: u64) -> Card {
    let heading = "# A new sign-in to your account\nWe noticed a sign-in from a new device. If it was you, there's nothing to do.";
    let banner = Cell {
        index: 0,
        name: "notice-banner.png".into(),
        picture: Expected::Ready(sunset(550, 220)),
        width: 550,
        height: 220,
        ..Cell::default()
    };
    let subtext = "-# Sent by Discord. We'll never ask for your password.";
    Card {
        framed: true,
        color: Some(Color::from_rgb8(0x58, 0x65, 0xf2)),
        rows: vec![
            Row::Text {
                text: text(
                    heading,
                    &[
                        (Kind::Heading(1), "A new sign-in to your account"),
                        (
                            Kind::Text,
                            "We noticed a sign-in from a new device. If it was you, there's nothing to do.",
                        ),
                    ],
                ),
                thumbnail: None,
                button: None,
            },
            Row::Media(Attachments {
                item,
                gallery: Gallery::mosaic(vec![banner]),
                ..Attachments::default()
            }),
            Row::Separator {
                large: false,
                divider: true,
            },
            Row::Text {
                text: plain("**Linux · Yapfast**\nPrague, Czechia"),
                thumbnail: Some(Picture {
                    index: 1,
                    handle: Expected::Ready(sunset(160, 160)),
                    spoiler: false,
                    description: String::new(),
                }),
                button: None,
            },
            Row::Text {
                text: plain("Not you? Change your password and turn on two-factor authentication."),
                thumbnail: None,
                button: Some(link(
                    "Secure account",
                    "https://discord.com/settings/account",
                )),
            },
            Row::Separator {
                large: true,
                divider: false,
            },
            Row::Text {
                text: text(subtext, &[(Kind::Subtext, &subtext[3..])]),
                thumbnail: None,
                button: None,
            },
            Row::Buttons(vec![
                link("Learn more", "https://support.discord.com/"),
                button("👍 It was me", Style::Secondary, "dismiss"),
            ]),
        ],
    }
}

/// The deploy message's controls outside a container: buttons (one off, a
/// link), a menu, more buttons; then a file card, an offer and a part that
/// can't be shown.
pub fn controls(item: u64) -> Card {
    let mut retry = button("Retry", Style::Secondary, "retry");
    retry.enabled = false;
    let file = attachments::File {
        index: 0,
        name: "deploy.log".into(),
        detail: "1.2 MB".into(),
        voice: None,
    };
    Card {
        framed: false,
        color: None,
        rows: vec![
            Row::Buttons(vec![
                button("Approve", Style::Success, "ok"),
                button("Roll back", Style::Danger, "fail"),
                retry,
                link("Logs", "https://example.com/logs"),
            ]),
            Row::Select(Select {
                text: "Pick an environment".into(),
                id: "environment".into(),
                enabled: true,
                picked: false,
            }),
            Row::Buttons(vec![
                button("Write notes", Style::Primary, "form"),
                button("Check later", Style::Secondary, "slow"),
            ]),
            Row::Media(Attachments {
                item,
                files: vec![file],
                ..Attachments::default()
            }),
            Row::Buttons(vec![Button {
                label: "Premium".into(),
                style: Style::Premium,
                sku_id: "42".into(),
                enabled: true,
                ..Button::default()
            }]),
            Row::Unknown,
        ],
    }
}

/// The notice's components.
pub fn notice() -> Components {
    let item = key("notice");
    Components {
        key: item,
        cards: vec![notice_card(item)],
        ..Components::default()
    }
}

/// The deploy message's components, its app's id set.
pub fn components() -> Components {
    let item = key("deploy");
    Components {
        key: item,
        cards: vec![controls(item)],
        app_id: "7".into(),
        ..Components::default()
    }
}

fn app(name: &str, initial: &str, tint: u32) -> Author {
    Author {
        name: name.into(),
        avatar: pictured(avatar(initial, tint), name),
        bot: true,
        known: true,
        ..Author::default()
    }
}

/// A timeline of Bob's DM with `message` alone in it, by `key`.
fn alone(message: Message) -> Timeline {
    let mut timeline = rows::bob();
    timeline.rows = vec![TimelineRow {
        key: message.key,
        first: true,
        message,
        ..TimelineRow::default()
    }];
    timeline.unread = None;
    timeline
}

/// The "Discord" app's DM with its notice, as `dm/discord_components_notice`.
pub fn notice_timeline() -> Timeline {
    let mut timeline = alone(Message {
        key: key("notice"),
        author: app("Discord", "D", 0x5865f2),
        components: notice(),
        body: Text::default(),
        ..bobs(60, true)
    });
    if let Some(room) = &mut timeline.room {
        room.id = "discord:system".into();
        room.name = "Discord".into();
        room.kind = RoomKind::Direct(pictured(avatar("D", 0x5865f2), "Discord"));
    }
    timeline
}

/// The status bot's deploy message, its last control having failed.
pub fn deploy_timeline() -> Timeline {
    alone(Message {
        key: key("deploy"),
        author: app("Status", "S", 0x5865f2),
        body: plain("Deploy **v2.4** finished."),
        components: components(),
        failed: Some("The application did not respond".into()),
        ..bobs(61, true)
    })
}
