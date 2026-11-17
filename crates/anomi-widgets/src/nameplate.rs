//! A nameplate behind a row, as Discord's client draws it: a gradient in its
//! palette's colour, and its picture at the row's right end, faded out under
//! the row's text. The Slint side: `Nameplate` (`ui/controls.slint:627`),
//! which the member list, the DM list and the account card lay under their
//! rows (`ui/people.slint:268`, `ui/rail.slint:525`, `ui/yapfast.slint:3026`,
//! `ui/server.slint:837`).
//!
//! Here [`nameplate`] wraps the row's content and draws the plate under it,
//! filling the row. It knows where the pointer is itself; the row says
//! whether it's selected. Where the picture starts fading in comes from the
//! row's text: wrap the column of the name and its second line in
//! [`extent`], laid out as wide as its text (`Shrink`), and the plate reads
//! its place and width once laid out, as the Slint rows handed theirs in.
//!
//! The picture is the still a view-model carries; a plate plays its
//! animation, through [`picture`](crate::picture), only while the pointer
//! is on its row and the window plays, and shows the still again once the
//! pointer leaves, as Discord's client does, so a list of plates draws and
//! decodes only stills.

mod extent;
mod look;
pub mod sample;
#[cfg(test)]
mod tests;
mod widget;

pub use extent::{Extent, extent};

use iced::Color;
use iced::widget::image;

use anomi_theme::{Element, Motion, Palette, View};

/// Where a plate is laid: Discord's placements differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    /// A member list's row: .6 strong at rest and .9 pointed at or
    /// selected, its colour stronger under the pointer.
    #[default]
    Member,
    /// A DM row (`channel` in Slint): drawn only while the row is selected,
    /// pointed at or focused, so a long list holds none; its colour stays
    /// under the pointer and its picture starts fading in sooner.
    Dm,
    /// The account card's: stronger, .4 rising to .6 under the pointer,
    /// and not faded under the text.
    Account,
}

/// A row's content with its plate under it; see [`nameplate`].
pub struct Nameplate<'a, Message> {
    content: Element<'a, Message>,
    place: Place,
    picture: Option<image::Handle>,
    palette: Option<[Color; 2]>,
    selected: bool,
    focused: bool,
    held: bool,
    radius: f32,
    top_radius: Option<f32>,
    inset: f32,
    opacity: f32,
    ground: fn(&Palette) -> Color,
    motion: Motion,
}

/// `content` with a plate under it, in `place`, playing while the pointer
/// is on it and `motion` plays. It has neither picture nor colour until
/// given them, and draws nothing then.
pub fn nameplate<'a, Message>(
    content: impl View<Message> + 'a,
    place: Place,
    motion: Motion,
) -> Nameplate<'a, Message> {
    Nameplate {
        content: content.boxed(),
        place,
        picture: None,
        palette: None,
        selected: false,
        focused: false,
        held: false,
        radius: 0.0,
        top_radius: None,
        inset: 0.0,
        opacity: 1.0,
        ground: |palette| palette.sidebar,
        motion,
    }
}

impl<Message> Nameplate<'_, Message> {
    /// The plate's picture, as its still; none until it arrived.
    pub fn picture(mut self, picture: Option<image::Handle>) -> Self {
        self.picture = picture;
        self
    }

    /// Its palette's colours on a dark theme and on a light one; none for a
    /// palette Anomi doesn't know, which leaves the picture without a tint.
    pub fn palette(mut self, palette: Option<[Color; 2]>) -> Self {
        self.palette = palette;
        self
    }

    /// The row is the one chosen.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The row has the keyboard's focus, which shows a DM row's plate.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Drawn as under the pointer, still: the profile editor's preview of
    /// the account's plate, which shows one frame at the strength Discord's
    /// preview has.
    pub fn held(mut self) -> Self {
        self.held = true;
        self
    }

    /// The corners of the row it fills.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Its top corners, when they differ: square under a card it joins, as
    /// the account's under the voice dock.
    pub fn top_radius(mut self, radius: f32) -> Self {
        self.top_radius = Some(radius);
        self
    }

    /// How far inside the row it stops, as a DM row's stops short of its
    /// focus ring.
    pub fn inset(mut self, inset: f32) -> Self {
        self.inset = inset;
        self
    }

    /// The plate's opacity, not the content's: an offline member's is halved
    /// (`ui/people.slint:274`). The plate fades as a whole, as Slint's layer
    /// of it does: its picture, fade and tint together lie `opacity` as far
    /// from the ground as at 1, under the text as past it, at rest, pointed
    /// at and chosen alike.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// What lies under the row at rest, which the fade under the text lets
    /// through: the sidebar unless said. Pointed at it's `hover`, chosen
    /// `active`, as the Slint rows said.
    pub fn ground(mut self, ground: fn(&Palette) -> Color) -> Self {
        self.ground = ground;
        self
    }
}
