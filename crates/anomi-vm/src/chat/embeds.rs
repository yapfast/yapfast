//! A message's embeds: link previews and bot cards, each behind a bar of its
//! colour (`EmbedCard`, `ui/messages.slint:597`). Task C05.
//!
//! A card's texts are [`Text`]: the title already bold and linking to its
//! page when it has one (the port of `embed_title`, `src/ui/messages.rs`),
//! a description with headings, quotes or code as blocks. Its pictures are
//! the message's attachments by `index`, which the viewer opens by.

use iced::Color;

use crate::kind::Expected;
use crate::menu::Spot;
use crate::text::{Link, Text};

use super::video;

/// A message's embeds: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Embeds {
    /// The message's key, which its pictures open by.
    pub item: u64,
    pub cards: Vec<Card>,
}

impl Embeds {
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

/// One embed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    /// The bar's colour; the theme's edge when none.
    pub color: Option<Color>,
    pub provider: Option<Text>,
    pub author: Option<Text>,
    pub title: Option<Text>,
    /// Where the card leads: a video's poster opens it.
    pub url: String,
    pub description: Option<Text>,
    /// Fields in rows: the ones of a row share it equally.
    pub rows: Vec<Vec<Field>>,
    pub footer: Option<Text>,
    pub thumbnail: Option<Thumbnail>,
    pub gallery: Option<Gallery>,
}

/// A field: its name over its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: Text,
    pub value: Text,
}

/// The small picture beside the text, 80 pixels square.
#[derive(Clone, Debug, PartialEq)]
pub struct Thumbnail {
    pub picture: Expected,
    /// Its place among the message's attachments.
    pub index: usize,
}

/// The card's pictures, each placed in a box `width` by `height`, which
/// shrinks to what the card has.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gallery {
    pub width: f32,
    pub height: f32,
    pub cells: Vec<Cell>,
    /// A video's poster: a click opens the video's page.
    pub video: bool,
}

/// A picture of a gallery (`ImageCell`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Its place among the message's attachments.
    pub index: usize,
    /// `Waiting` shows a skeleton.
    pub picture: Expected,
    /// A spoiler shows blurred under its cover until a click reveals it.
    pub spoiler: bool,
    /// What it is called, for the screen reader.
    pub name: String,
    /// A video file the media proxy has, which plays in the cell.
    pub playing: Option<video::Video>,
}

/// A message's embeds: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A click on a link in a text.
    Link(Link),
    /// A video's poster: its page opens in the browser.
    Open(String),
    /// A picture opens in the viewer: the message's key and the
    /// attachment.
    View { item: u64, index: usize },
    /// A spoiler's cover is lifted.
    Reveal { item: u64, index: usize },
    /// A right click on a picture; `video` for one that plays.
    Menu {
        item: u64,
        index: usize,
        video: bool,
        at: Spot,
    },
    /// What the video in a cell asks for.
    Video(video::Intent),
}

impl From<video::Intent> for Intent {
    fn from(intent: video::Intent) -> Self {
        Self::Video(intent)
    }
}
