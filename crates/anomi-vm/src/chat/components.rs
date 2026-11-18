//! A bot message's components: what it shows and what it asks for. Ports
//! `ComponentCard`, `CardRow` and `CardButton` (`ui/messages.slint:609`-
//! `:633`) as the projection (`src/ui/messages.rs`, `card_row`) fills them.
//! Task C06.
//!
//! A message's [`Card`]s are Discord's containers (framed, with a bar of
//! their colour) or the components outside one; each is a list of [`Row`]s.
//! Which control of the message waits for its bot is [`Components::busy`]:
//! the core holds back the others, as Discord's client queues one
//! interaction a message. Why the last one failed is the message's
//! `failed`, under all of this.
//!
//! A gallery and its file cards are a message's ([`Attachments`], C04's):
//! the app lays the gallery out within a card's width, and what they ask
//! for, with a section's thumbnail, is [`Intent::Media`].

use iced::Color;

use crate::kind::Expected;
use crate::menu::Spot;
use crate::text::{Link, Text};

use super::attachments::{self, Attachments};
use super::message::Size;

/// A bot message's components: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Components {
    /// The message's key: every intent names it.
    pub key: u64,
    pub cards: Vec<Card>,
    /// The app's id, which a store page opens by.
    pub app_id: String,
    /// One of the message's controls waits for its bot.
    pub busy: bool,
    /// The size of messages' text.
    pub size: Size,
}

/// A container of components, framed with a bar of its colour, or those
/// outside one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub framed: bool,
    pub color: Option<Color>,
    pub rows: Vec<Row>,
}

/// One row of a card.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Text, with what a section shows beside it.
    Text {
        text: Text,
        thumbnail: Option<Picture>,
        button: Option<Button>,
    },
    /// A gallery and file cards, as a message's: its `item` is the
    /// message's key, its `lit`, `preview` and `undelivered` the message's.
    Media(Attachments),
    /// A gap, large or small, with or without its line.
    Separator {
        large: bool,
        divider: bool,
    },
    Buttons(Vec<Button>),
    Select(Select),
    /// A part Discord's client tells of but this one can't show.
    Unknown,
}

/// A section's thumbnail. `index` names it among the message's files.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub index: usize,
    pub handle: Expected,
    pub spoiler: bool,
    /// Its alt text.
    pub description: String,
}

/// What a button does when pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
    /// Opens an address, its arrow after the label.
    Link,
    /// A purchase, made in Discord.
    Premium,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Button {
    /// Its label after its emoji, if a server's: that one drawn before it.
    pub label: String,
    pub emoji: Option<crate::kind::Expected>,
    pub style: Style,
    /// A link's address.
    pub url: String,
    /// What the app is told when it is pressed; empty for a link.
    pub custom_id: String,
    /// A premium button's offer.
    pub sku_id: String,
    pub enabled: bool,
}

/// A menu by what it shows, which opens its dropdown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Select {
    /// What it picked by default if `picked`, its placeholder otherwise.
    pub text: String,
    pub id: String,
    pub enabled: bool,
    pub picked: bool,
}

/// A bot message's components: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant. Each names the message by its key.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A text's link.
    Link(u64, Link),
    /// A link button, or a store page: open this address.
    Open(u64, String),
    /// A button with this custom id was pressed: the app is told.
    Use(u64, String),
    /// A menu, by its custom id, opens its dropdown under `Spot`.
    OpenSelect(u64, String, Spot),
    /// A thumbnail's, a gallery's or a file card's, as a message's
    /// attachments ask (`reveal-attachment`, `view-image`,
    /// `save-attachment`, `media-menu`).
    Media(attachments::Intent),
}

impl Button {
    /// The address a press opens, when it is no interaction: a link's, or
    /// the app's store page for an offer (none without the app's id).
    pub fn address(&self, app_id: &str) -> Option<String> {
        if !self.custom_id.is_empty() {
            return None;
        }
        if !self.sku_id.is_empty() && !app_id.is_empty() {
            return Some(format!(
                "https://discord.com/application-directory/{app_id}/store/{}",
                self.sku_id
            ));
        }
        Some(self.url.clone())
    }
}
