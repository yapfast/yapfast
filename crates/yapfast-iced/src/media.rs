//! Pictures in messages: one piece ([`Shot`]) that puts a core picture in a
//! box (at its own size, cut to cover a tile, or fitted within), on its
//! ground while it comes, playing while it moves, blurred under a spoiler's
//! cover until revealed; and the gallery that lays a message's pictures out
//! (one at its own size, several as square tiles two to a row).
//! Attachments, galleries, embeds' pictures, cards' thumbnails and stickers
//! all show through it.

use std::sync::Arc;

use iced::{
    Alignment, Color, Element, Length, Rectangle, Size,
    widget::{Space, container, stack, text},
};
use yapfast_core::chat::{self, Attachment, Picture, Placement};

use crate::{app::Message, fonts::BOLD, style::rounded, widgets::Pic};

/// A gallery's square tiles and the gap between them, as Discord's client
/// lays out several pictures.
pub const TILE: f32 = 273.0;
pub const TILE_GAP: f32 = 4.0;
/// A spoiler's blur: the picture averaged down to this many pixels across,
/// drawn large.
const BLUR_SIDE: u32 = 10;
/// The spoiler cover's dimming.
const COVER_DIM: Color = Color::from_rgba8(0, 0, 0, 0x59 as f32 / 255.0);

/// How a picture fills its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Scaled to fill the box and cut to it about its middle.
    Cover,
    /// Scaled to fit inside the box, keeping its shape, in the middle.
    Contain,
}

/// A picture in a box.
pub struct Shot<'a> {
    /// The core's picture; none while it comes.
    pub picture: Option<&'a Arc<Picture>>,
    pub size: Size,
    pub fit: Fit,
    pub radius: f32,
    /// Under a spoiler's cover.
    pub covered: bool,
    /// Its ground shows behind it while it comes, and behind a picture
    /// with see-through parts; a sticker has none.
    pub ground: bool,
}

impl<'a> Shot<'a> {
    pub fn new(picture: Option<&'a Arc<Picture>>, size: Size) -> Self {
        Self {
            picture,
            size,
            fit: Fit::Cover,
            radius: 0.0,
            covered: false,
            ground: true,
        }
    }

    pub fn fit(mut self, fit: Fit) -> Self {
        self.fit = fit;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn covered(mut self, covered: bool) -> Self {
        self.covered = covered;
        self
    }

    pub fn bare(mut self) -> Self {
        self.ground = false;
        self
    }
}

/// Where each of a message's pictures goes in its gallery, and its size.
#[derive(Clone, Debug, PartialEq)]
pub struct Gallery {
    /// Each picture's attachment index and box, from the gallery's corner.
    pub cells: Vec<(usize, Rectangle)>,
    pub size: Size,
}

impl Gallery {
    /// The pictures of `attachments` placed at `placement` (those with a
    /// preview, coming, or a video): one at its own size, several as square
    /// tiles two to a row; scaled to be at most `room` wide.
    pub fn of(attachments: &[Attachment], placement: Placement, room: f32) -> Self {
        let pictures: Vec<(usize, &Attachment)> = attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| attachment.placement == placement && pictured(attachment))
            .collect();
        let (cells, size) = match pictures.as_slice() {
            [] => (Vec::new(), Size::ZERO),
            [(index, attachment)] => {
                let size = coming_size(attachment);
                (vec![(*index, Rectangle::with_size(size))], size)
            }
            _ => {
                let cells = pictures
                    .iter()
                    .enumerate()
                    .map(|(place, (index, _))| {
                        let at = iced::Point::new(
                            (place % 2) as f32 * (TILE + TILE_GAP),
                            (place / 2) as f32 * (TILE + TILE_GAP),
                        );
                        (*index, Rectangle::new(at, Size::new(TILE, TILE)))
                    })
                    .collect();
                let rows = pictures.len().div_ceil(2) as f32;
                let size = Size::new(2.0 * TILE + TILE_GAP, rows * TILE + (rows - 1.0) * TILE_GAP);
                (cells, size)
            }
        };
        let gallery = Self { cells, size };
        if room > 0.0 && gallery.size.width > room {
            let scale = room / gallery.size.width;
            gallery.scaled(scale)
        } else {
            gallery
        }
    }

    fn scaled(self, scale: f32) -> Self {
        let whole = |side: f32| (side * scale).floor().max(1.0);
        Self {
            cells: self
                .cells
                .into_iter()
                .map(|(index, cell)| {
                    (
                        index,
                        Rectangle::new(
                            iced::Point::new(whole(cell.x), whole(cell.y)),
                            Size::new(whole(cell.width), whole(cell.height)),
                        ),
                    )
                })
                .collect(),
            size: Size::new(whole(self.size.width), whole(self.size.height)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

/// Whether an attachment shows as a picture: a preview, one on its way, or
/// a video (which plays even without its poster).
pub fn pictured(attachment: &Attachment) -> bool {
    attachment.preview.is_some() || loading(attachment) || attachment.video
}

/// A picture whose preview is on its way.
pub fn loading(attachment: &Attachment) -> bool {
    (attachment.image || attachment.video)
        && attachment.preview.is_none()
        && !attachment.preview_failed
}

/// The size a preview shows at: its picture's, else the size the sender
/// gave fitted to the preview box as the core decodes to it, else the
/// preview box's width by 200.
pub fn coming_size(attachment: &Attachment) -> Size {
    if let Some(picture) = &attachment.preview {
        return Size::new(picture.width as f32, picture.height as f32);
    }
    let Some((width, height)) = attachment.dimensions else {
        return Size::new(chat::PREVIEW_WIDTH as f32, 200.0);
    };
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(chat::PREVIEW_WIDTH) / f64::from(width.max(1)),
            f64::from(chat::PREVIEW_HEIGHT) / f64::from(height.max(1)),
        ),
    );
    let side = |side: u32| ((f64::from(side) * scale).round() as f32).max(1.0);
    Size::new(side(width), side(height))
}

impl<'a> crate::ctx::Ctx<'a> {
    /// A picture in its box: the core's own pixels when it fills the box
    /// at its own size, else a copy at the box's size (made off the window's
    /// thread, its ground showing until then); a moving one plays.
    pub fn shot(self, shot: Shot<'_>) -> Element<'a, Message> {
        let Shot {
            picture,
            size,
            fit,
            radius,
            covered,
            ground,
        } = shot;
        let shown = picture.filter(|picture| picture.width > 0 && picture.height > 0);
        // Where the picture goes in its box: the whole box, or for Contain
        // the middle at its shape.
        let at = match (shown, fit) {
            (Some(picture), Fit::Contain) => {
                let scale = f32::min(
                    size.width / picture.width as f32,
                    size.height / picture.height as f32,
                );
                let drawn = Size::new(
                    (picture.width as f32 * scale).round().max(1.0),
                    (picture.height as f32 * scale).round().max(1.0),
                );
                Rectangle::new(
                    iced::Point::new(
                        ((size.width - drawn.width) / 2.0).round(),
                        ((size.height - drawn.height) / 2.0).round(),
                    ),
                    drawn,
                )
            }
            _ => Rectangle::with_size(size),
        };
        let (width, height) = (at.width as u32, at.height as u32);
        // The window's pixels it covers, which a copy is made at.
        let pixels = (self.pixels(width), self.pixels(height));
        // A still that fills the whole box and is opaque hides its ground.
        let mut hidden = false;
        let inner: Element<'_, Message> = match shown {
            Some(picture) if covered => {
                // Averaged down to a few pixels and drawn large: a blur.
                let side = BLUR_SIDE.min(picture.width.max(picture.height));
                let scale = picture.width.max(picture.height) as f32 / side as f32;
                let small = (
                    ((picture.width as f32 / scale).ceil() as u32).max(1),
                    ((picture.height as f32 / scale).ceil() as u32).max(1),
                );
                match self.pictures.fitter.borrow_mut().get(
                    picture,
                    small.0,
                    small.1,
                    false,
                    self.media_look(),
                ) {
                    Some(blur) => Pic::new(blur, size)
                        .at(Rectangle::with_size(size))
                        .radius(radius)
                        .into(),
                    None => Space::new().width(size.width).height(size.height).into(),
                }
            }
            Some(picture) if picture.animation.is_some() && self.plays => self
                .pictures
                .animated(picture, size, at, fit == Fit::Cover, self.media_look())
                .radius(radius)
                .into(),
            // At its own size in the view or in the window's pixels, the
            // core's own pixels: one made at the view's size is left for the
            // renderer to stretch to the zoom, rather than copied here on the
            // window's thread (or blank until the fitter's copy comes).
            Some(picture)
                if [(width, height), (pixels.0, pixels.1)]
                    .contains(&(picture.width, picture.height)) =>
            {
                let handle = self.own_size(picture);
                hidden = at.size() == size && self.pictures.opaque(picture);
                Pic::new(handle, size).at(at).radius(radius).into()
            }
            Some(picture) => {
                match self.pictures.fitter.borrow_mut().get(
                    picture,
                    pixels.0,
                    pixels.1,
                    fit == Fit::Cover,
                    self.media_look(),
                ) {
                    Some(handle) => {
                        hidden = at.size() == size && self.pictures.opaque(picture);
                        Pic::new(handle, size).at(at).radius(radius).into()
                    }
                    None => Space::new().width(size.width).height(size.height).into(),
                }
            }
            None => Space::new().width(size.width).height(size.height).into(),
        };
        let mut layers = stack![inner].width(size.width).height(size.height);
        if covered {
            layers = layers.push(self.spoiler_cover(radius));
        }
        let boxed = container(layers).width(size.width).height(size.height);
        if !ground || hidden {
            return boxed.into();
        }
        let raised = self.look.theme.raised;
        boxed.style(move |_| rounded(raised, radius)).into()
    }

    /// A spoiler's cover over a picture: the picture dimmed, crate::i18n::tr!(self.i18n, "SPOILER") on a
    /// pill.
    pub fn spoiler_cover(self, radius: f32) -> Element<'a, Message> {
        container(
            container(
                text(crate::i18n::tr!(self.i18n, "SPOILER"))
                    .size(13)
                    .font(BOLD)
                    .color(Color::WHITE),
            )
            .padding([0, 12])
            .height(28)
            .align_y(Alignment::Center)
            .style(|_| rounded(crate::style::SMOKE, 14)),
        )
        .center(Length::Fill)
        .style(move |_| rounded(COVER_DIM, radius))
        .into()
    }
}

impl<'a> crate::ctx::Ctx<'a> {}
