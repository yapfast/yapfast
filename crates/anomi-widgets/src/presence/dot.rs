//! A presence dot on a ring cut from the background, as Discord draws it
//! (`StatusDot`, `ui/rail.slint:360`): green when online, a green phone
//! standing up from it when online only on a phone, a moon when idle, a bar
//! when they don't want to be disturbed, and a grey ring when offline. Every
//! dimension is a count of one sixteenth of the dot, so any size stays a
//! circle that fills its slot.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::{Border, Color, Length, Rectangle, Size};

use anomi_theme::{Palette, Renderer, Theme, rgb};
use anomi_vm::kind::Presence;

use crate::avatar::Ground;

/// Idle's yellow and do-not-disturb's red, the same in every theme.
const IDLE: u32 = 0xf0b232;
const DND: u32 = 0xf23f43;

/// What the dot's ring is cut out of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ring {
    Ground(Ground),
    /// A ground the palette has no name for, such as a profile's own colour.
    Color(Color),
}

impl Ring {
    pub(crate) fn color(self, palette: &Palette) -> Color {
        match self {
            Self::Ground(ground) => ground.color(palette),
            Self::Color(color) => color,
        }
    }
}

impl From<Ground> for Ring {
    fn from(ground: Ground) -> Self {
        Self::Ground(ground)
    }
}

/// A presence dot `width` pixels across (16 in a row of small avatars).
/// Mobile's phone stands five sixteenths above it.
pub fn status_dot(presence: Presence, width: f32, ring: impl Into<Ring>) -> StatusDot {
    StatusDot {
        presence,
        width,
        ring: ring.into(),
    }
}

pub struct StatusDot {
    presence: Presence,
    width: f32,
    ring: Ring,
}

/// A round quad centred at `centre`.
fn disc(renderer: &mut Renderer, centre: (f32, f32), across: f32, color: Color) {
    rect(
        renderer,
        (centre.0 - across / 2.0, centre.1 - across / 2.0),
        (across, across),
        across / 2.0,
        color,
    );
}

fn rect(renderer: &mut Renderer, at: (f32, f32), size: (f32, f32), radius: f32, color: Color) {
    renderer.fill_quad(
        Quad {
            bounds: Rectangle::new(iced::Point::new(at.0, at.1), Size::new(size.0, size.1)),
            border: Border::default().rounded(radius),
            ..Quad::default()
        },
        color,
    );
}

impl StatusDot {
    /// The phone of "online on a phone": the ring's tall plate, the body
    /// and its screen and button cut out of it.
    fn phone(renderer: &mut Renderer, (x, y): (f32, f32), unit: f32, ring: Color, live: Color) {
        let width = 16.0 * unit;
        let plate = 21.0 * unit;
        let top = y + width - plate;
        rect(renderer, (x, top), (width, plate), 4.0 * unit, ring);
        // The body is centred in the plate, as Slint centres a child.
        let body = (x + 3.0 * unit, top + 3.0 * unit);
        rect(renderer, body, (10.0 * unit, 15.0 * unit), 2.0 * unit, live);
        rect(
            renderer,
            (body.0 + 2.0 * unit, body.1 + 2.0 * unit),
            (6.0 * unit, 9.0 * unit),
            unit,
            ring,
        );
        rect(
            renderer,
            (body.0 + 4.0 * unit, body.1 + 12.0 * unit),
            (2.0 * unit, 2.0 * unit),
            unit,
            ring,
        );
    }
}

impl widget::Meta for StatusDot {}

impl<Message> widget::Widget<Message, Theme, Renderer> for StatusDot {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.width))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let side = Length::Fixed(self.width);
        tree.size = limits.resolve(side, side, Size::new(self.width, self.width));
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let palette = theme.palette();
        let ring = self.ring.color(palette);
        let bounds = layout.bounds();
        let unit = bounds.width / 16.0;
        let origin = (bounds.x, bounds.y);
        let centre = (
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        let mobile = self.presence == Presence::Mobile;
        if mobile {
            Self::phone(renderer, origin, unit, ring, palette.live);
            return;
        }
        disc(renderer, centre, bounds.width, ring);
        let (across, fill) = match self.presence {
            Presence::DoNotDisturb => (10.0, rgb(DND)),
            Presence::Idle => (12.0, rgb(IDLE)),
            Presence::Online => (10.0, palette.live),
            _ => (10.0, palette.faint),
        };
        disc(renderer, centre, across * unit, fill);
        match self.presence {
            // Offline: the grey disc's middle cut out, leaving a ring.
            Presence::Offline | Presence::None => disc(renderer, centre, 4.0 * unit, ring),
            // The moon: a bite out of the disc's top left.
            Presence::Idle => {
                let bite = (origin.0 + unit + 3.5 * unit, origin.1 + unit + 3.5 * unit);
                disc(renderer, bite, 7.0 * unit, ring);
            }
            Presence::DoNotDisturb => {
                let (w, h) = (6.0 * unit, 2.0 * unit);
                rect(
                    renderer,
                    (centre.0 - w / 2.0, centre.1 - h / 2.0),
                    (w, h),
                    h / 2.0,
                    ring,
                );
            }
            _ => {}
        }
    }
}
