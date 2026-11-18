//! A bot message's components (`card_row`, `card_button`,
//! `src/ui/messages.rs:951`, `:1055`): Discord's containers and the
//! components outside one, their text with the message's custom emoji,
//! thumbnails, galleries and files found among the message's attachments by
//! slot, buttons and menus. While one of its controls waits for its bot,
//! the core holds the message's controls back (`busy`); a menu's choice
//! sent shows in its box until the bot answers.

use iced::Color;
use yapfast_core::chat::{Beside, ButtonStyle, CardButton, CardRow, ChatMessage, Placement};

use anomi_vm::chat::attachments::Attachments;
use anomi_vm::chat::components::{Button, Card, Components, Picture, Row, Select, Style};
use anomi_vm::text::{self, CustomEmoji, Source, Text};

use super::attachments;
use super::message::{Context, body_sections};
use crate::pictures;

/// The message's components; none for a message without.
pub(crate) fn components(cx: &Context<'_>, message: &ChatMessage, key: u64) -> Components {
    if message.cards.is_empty() {
        return Components::default();
    }
    let emoji: Vec<CustomEmoji> = message
        .emoji
        .iter()
        .map(|emoji| CustomEmoji {
            text: emoji.text.clone(),
            id: emoji.id.clone(),
            still: pictures::find(cx.state, Some(&emoji.picture)),
        })
        .collect();
    let parts = Parts {
        cx,
        message,
        key,
        emoji: &emoji,
    };
    Components {
        key,
        cards: message
            .cards
            .iter()
            .map(|card| Card {
                framed: card.framed,
                color: card.color.map(|rgb| {
                    let [_, red, green, blue] = rgb.to_be_bytes();
                    Color::from_rgb8(red, green, blue)
                }),
                rows: card.rows.iter().map(|row| parts.row(row)).collect(),
            })
            .collect(),
        app_id: app_id(message),
        busy: cx.state.interactions.busy(&message.item),
        size: cx.look.size,
    }
}

/// The app a store page opens by: the message's, or a bot's own, its user
/// being its app's.
fn app_id(message: &ChatMessage) -> String {
    message
        .application_id
        .as_deref()
        .or_else(|| {
            message
                .bot
                .then(|| yapfast_core::discord::sender_user(&message.sender))
                .flatten()
        })
        .unwrap_or_default()
        .to_owned()
}

/// What each row is made with.
struct Parts<'a> {
    cx: &'a Context<'a>,
    message: &'a ChatMessage,
    key: u64,
    emoji: &'a [CustomEmoji],
}

impl Parts<'_> {
    fn row(&self, row: &CardRow) -> Row {
        match row {
            CardRow::Text { text, beside } => Row::Text {
                text: self.text(text),
                thumbnail: match beside {
                    Some(Beside::Thumbnail(slot)) => self.thumbnail(*slot),
                    _ => None,
                },
                button: match beside {
                    Some(Beside::Button(button)) => Some(card_button(button)),
                    _ => None,
                },
            },
            CardRow::Media(slot) => Row::Media(self.media(Placement::Component(*slot))),
            CardRow::Separator { divider, large } => Row::Separator {
                large: *large,
                divider: *divider,
            },
            CardRow::Buttons(buttons) => Row::Buttons(buttons.iter().map(card_button).collect()),
            CardRow::Select {
                text,
                disabled,
                custom_id,
                options,
                ..
            } => {
                let chosen = self.cx.kept.chosen(&self.message.item, custom_id);
                Row::Select(Select {
                    text: chosen.unwrap_or(text).to_owned(),
                    id: custom_id.clone(),
                    enabled: !disabled && !custom_id.is_empty(),
                    picked: chosen.is_some() || options.iter().any(|option| option.default),
                })
            }
            CardRow::Unknown => Row::Unknown,
        }
    }

    /// A text's Markdown with the message's custom emoji, its spoilers as
    /// typed, as a card's text is in the Slint app (`text_blocks`).
    fn text(&self, markdown: &str) -> Text {
        let sections = body_sections(markdown);
        text::markdown(Source {
            emoji: self.emoji,
            sections: sections.as_deref(),
            ..Source::new(markdown)
        })
    }

    /// A section's thumbnail: the first picture placed with its slot.
    fn thumbnail(&self, slot: usize) -> Option<Picture> {
        let (index, attachment) =
            attachments::shown(self.message, Placement::Component(slot)).next()?;
        Some(Picture {
            index,
            handle: attachments::picture(attachment),
            spoiler: attachment.spoiler && self.covered(index),
            description: attachment.description.clone().unwrap_or_default(),
        })
    }

    /// A gallery's pictures and files, as the message's own show.
    fn media(&self, placement: Placement) -> Attachments {
        let (cx, message, key) = (self.cx, self.message, self.key);
        Attachments {
            item: key,
            lit: false,
            preview: cx.look.preview,
            undelivered: message.delivery != yapfast_core::chat::Delivery::Sent,
            gallery: attachments::gallery(cx, message, key, placement, &|index| {
                self.covered(index)
            }),
            files: attachments::files(message, key, placement),
            stickers: Vec::new(),
        }
    }

    fn covered(&self, index: usize) -> bool {
        let revealed = format!("{}/a{index}", self.message.item);
        !self.cx.revealed.contains(&revealed)
    }
}

/// An app's button: a link's leads somewhere, an offer's to its store, any
/// other asks its app; one that does none of these is off.
fn card_button(button: &CardButton) -> Button {
    Button {
        label: button.label.clone(),
        style: match button.style {
            ButtonStyle::Primary => Style::Primary,
            ButtonStyle::Secondary => Style::Secondary,
            ButtonStyle::Success => Style::Success,
            ButtonStyle::Danger => Style::Danger,
            ButtonStyle::Link => Style::Link,
            ButtonStyle::Premium => Style::Premium,
        },
        url: button.url.clone().unwrap_or_default(),
        custom_id: button.custom_id.clone().unwrap_or_default(),
        sku_id: button.sku_id.clone().unwrap_or_default(),
        enabled: !button.disabled
            && (button.url.is_some() || button.custom_id.is_some() || button.sku_id.is_some()),
    }
}
