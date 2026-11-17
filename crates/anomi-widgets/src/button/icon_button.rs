//! An icon-only action with a tooltip naming it. The Slint side:
//! `IconButton` (`ui/controls.slint:105`): a square face (32 px, its picture
//! 20), whose ink is `tint` until the pointer is over it, then the text
//! colour; `alert` draws it red (a muted microphone), `danger` reddens it
//! under the pointer, `emphasis` fills it with the accent. Chain the setters
//! and end with [`IconButton::view`].

use iced::Color;

use anomi_theme::View;

use anomi_theme::{Motion, Palette, Theme, metrics};

use super::face::Face;
use super::look::Colours;
use crate::icon::Icon;
use crate::tip::Side;

/// The button's side (`ui/controls.slint:113`).
const SIZE: f32 = 32.0;
/// The danger hover fill's alpha (`:136`).
const DANGER_HOVER: f32 = 0.16;
/// The hover lift, a tenth of the overlay: `fill.mix(Theme.overlay, 0.9)` (`:137`).
const HOVER_LIFT: f32 = 0.1;

/// A colour read from the theme at draw time.
pub type Pick = fn(&Palette) -> Color;

#[derive(Clone, Copy)]
struct Look {
    tint: Pick,
    fill: Pick,
    danger: bool,
    emphasis: bool,
}

impl Look {
    /// `ui/controls.slint:131-141` and `:145-147`.
    fn colours(&self, theme: &Theme, hovered: bool, enabled: bool, alert: bool) -> Colours {
        let palette = theme.palette();
        let fill = (self.fill)(palette);
        let hovered = hovered && enabled;
        let ground = if !enabled {
            fill
        } else if self.emphasis {
            if hovered {
                palette.accent_hover
            } else {
                palette.accent
            }
        } else if self.danger && hovered {
            palette.danger.scale_alpha(DANGER_HOVER)
        } else if hovered {
            // `fill.mix(overlay, 0.9)`: a tenth of the overlay over the fill.
            if fill.a == 0.0 {
                palette.lift(HOVER_LIFT)
            } else {
                super::paint::blend(fill, palette.overlay, HOVER_LIFT)
            }
        } else {
            fill
        };
        let ink = if !enabled {
            palette.faint
        } else if self.emphasis {
            palette.on_accent
        } else if alert || (self.danger && hovered) {
            palette.danger
        } else if hovered {
            palette.text
        } else {
            (self.tint)(palette)
        };
        Colours::new(ground, ink)
    }
}

pub struct IconButton<'a, Message> {
    icon: Icon,
    label: &'a str,
    motion: Motion,
    look: Look,
    alert: bool,
    enabled: bool,
    size: f32,
    glyph: f32,
    radius: f32,
    side: Side,
    on_press: Option<Message>,
}

/// An icon button; `label` names it in its tooltip.
pub fn icon_button<'a, Message>(
    icon: Icon,
    label: &'a str,
    motion: Motion,
) -> IconButton<'a, Message> {
    IconButton {
        icon,
        label,
        motion,
        look: Look {
            tint: |palette| palette.secondary,
            fill: |_| Color::TRANSPARENT,
            danger: false,
            emphasis: false,
        },
        alert: false,
        enabled: true,
        size: SIZE,
        glyph: metrics::ICON_ROW,
        radius: metrics::RADIUS_ROW,
        side: Side::Above,
        on_press: None,
    }
}

impl<'a, Message> IconButton<'a, Message> {
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A state worth warning about, drawn in red.
    pub fn alert(mut self, alert: bool) -> Self {
        self.alert = alert;
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.look.danger = danger;
        self
    }

    /// Filled with the accent.
    pub fn emphasis(mut self, emphasis: bool) -> Self {
        self.look.emphasis = emphasis;
        self
    }

    /// The side length of the face, 32 by default.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The picture's side, 20 by default.
    pub fn glyph(mut self, glyph: f32) -> Self {
        self.glyph = glyph;
        self
    }

    /// The ink at rest, `secondary` by default.
    pub fn tint(mut self, tint: Pick) -> Self {
        self.look.tint = tint;
        self
    }

    /// The fill at rest, none by default.
    pub fn fill(mut self, fill: Pick) -> Self {
        self.look.fill = fill;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Where the tooltip sits: above by default.
    pub fn tip_side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// The widget, with its tooltip.
    pub fn view(self) -> impl View<Message> + 'a
    where
        Message: Clone + 'a,
    {
        let (label, side) = (self.label, self.side);
        self.face().tipped(label, side)
    }

    /// The face alone, without its tooltip.
    pub fn face(self) -> Face<'a, Message> {
        let look = self.look;
        let mut face = Face::new(
            self.motion,
            move |theme: &Theme, hovered, enabled, alert| {
                look.colours(theme, hovered, enabled, alert)
            },
        );
        face.open = self.alert;
        face.leading = Some(self.icon);
        face.glyph = self.glyph;
        face.min = iced::Size::new(self.size, self.size);
        face.fixed = true;
        face.radius = self.radius;
        face.enabled = self.enabled;
        face.on_press = self.on_press;
        face
    }
}
