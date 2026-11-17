//! A message's gallery, files and stickers: what it shows and what it asks
//! for. Task C04 (docs/iced/tasks/chat.md).
//!
//! The app lays the gallery out (`gallery_cells`, `src/ui/pictures.rs`) and
//! hands every cell its place; [`Gallery::mosaic`] makes the same layout for
//! samples and for a message built here.

use super::{video::Video, voice_message::VoiceMessage};
use crate::kind::Expected;
use crate::menu::Spot;

/// A message's gallery, files and stickers: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachments {
    /// The message's key, which every intent names it by.
    pub item: u64,
    /// The message the pointer is on (`active-item`): its pictures show Save.
    pub lit: bool,
    /// The local preview of the interface: nothing saves or plays.
    pub preview: bool,
    /// Still going out, or failed to: nothing saves or plays yet.
    pub undelivered: bool,
    pub gallery: Gallery,
    pub files: Vec<File>,
    pub stickers: Vec<Sticker>,
}

impl Attachments {
    /// Whether it draws nothing.
    pub fn is_empty(&self) -> bool {
        self.gallery.cells.is_empty() && self.files.is_empty() && self.stickers.is_empty()
    }

    /// Whether it takes actions: saving, playing, keeping a GIF.
    pub fn acts(&self) -> bool {
        !self.preview && !self.undelivered
    }
}

/// A message's pictures and videos laid out as one block `width` by `height`
/// (`gallery-width`, `gallery-height`), in logical pixels as it fits when
/// there is room; narrower, the view shrinks it, keeping its shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gallery {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Cell>,
}

/// A tile of the mosaic and the gap between tiles.
const TILE: u32 = 273;
const GAP: u32 = 4;

impl Gallery {
    /// `cells` as the app lays them out: one at its own size (its `width` and
    /// `height`), several as square tiles two to a row, together as wide as
    /// one preview, an odd count's first picture filling what a lone last
    /// tile would leave: the left column beside the other two of three, or a
    /// row of its own above the pairs.
    pub fn mosaic(mut cells: Vec<Cell>) -> Self {
        let count = cells.len() as u32;
        match count {
            0 => return Self::default(),
            1 => {
                let (width, height) = (cells[0].width, cells[0].height);
                return Self {
                    width,
                    height,
                    cells,
                };
            }
            _ => {}
        }
        let wide = 2 * TILE + GAP;
        let lone = count % 2;
        let rows = lone + (count - lone) / 2;
        for (place, cell) in cells.iter_mut().enumerate() {
            let place = place as u32;
            (cell.x, cell.y, cell.width, cell.height) = match (count, place) {
                (3, 0) => (0, 0, TILE, wide),
                (3, _) => (TILE + GAP, (place - 1) * (TILE + GAP), TILE, TILE),
                (_, 0) if lone == 1 => (0, 0, wide, TILE),
                _ => {
                    let paired = place - lone;
                    (
                        (paired % 2) * (TILE + GAP),
                        (lone + paired / 2) * (TILE + GAP),
                        TILE,
                        TILE,
                    )
                }
            };
        }
        Self {
            width: wide,
            height: rows * TILE + (rows - 1) * GAP,
            cells,
        }
    }
}

/// A picture or video of the gallery.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    /// Its index among the message's attachments.
    pub index: usize,
    pub name: String,
    /// Its alt text.
    pub description: String,
    /// `Waiting` draws a skeleton at the size it will take.
    pub picture: Expected,
    /// Where it lies in the gallery, and its size.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Covered by its spoiler: the picture shows blurred under the cover
    /// until a click reveals it.
    pub spoiler: bool,
    /// A video, whose poster the picture is: it plays in place (C09).
    pub video: Option<Video>,
    /// A GIF, which can be kept among the favourites.
    pub gif: Option<Gif>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gif {
    pub favorite: bool,
}

/// A file, or a picture without a preview, as a card; a voice message as a
/// player (C11).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct File {
    pub index: usize,
    pub name: String,
    /// Under the name: its size.
    pub detail: String,
    pub voice: Option<VoiceMessage>,
}

/// A Discord sticker, drawn on its own at its size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sticker {
    pub index: usize,
    pub name: String,
    pub picture: Expected,
    pub width: u32,
    pub height: u32,
}

/// A message's gallery, files and stickers: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A click on a covered spoiler: show it (`reveal-attachment`).
    Reveal { item: u64, index: usize },
    /// A click on a picture: open it large (`view-image`).
    Open { item: u64, index: usize },
    /// Save the attachment (`save-attachment`).
    Save { item: u64, index: usize },
    /// A right click on a picture or video (`media-menu`), at the pointer.
    Menu {
        item: u64,
        index: usize,
        video: bool,
        spot: Spot,
    },
    /// The star on a GIF: keep it or let it go (`GifFavorites.toggle`).
    Favorite { item: u64, index: usize },
    /// A click on a sticker: its card opens beside it.
    Sticker { item: u64, index: usize, spot: Spot },
    /// What a video asks for (C09).
    Video(super::video::Intent),
    /// What a voice message asks for (C11).
    Voice(super::voice_message::Intent),
}
