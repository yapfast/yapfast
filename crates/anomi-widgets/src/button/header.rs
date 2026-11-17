//! A conversation header's labelled toggle, such as the members button: an
//! icon and a word, 32 px tall, lit under the pointer and while its panel is
//! open, with its name in a tooltip below. The Slint side: `HeaderToggle`
//! (`ui/controls.slint:1674`).

use iced::Color;

use anomi_theme::{Motion, Theme, View, typography};

use super::face::Face;
use super::look::Colours;
use crate::icon::Icon;
use crate::tip::Side;

/// `ui/controls.slint:1680`.
const HEIGHT: f32 = 32.0;
/// The picture, the gap and the padding (`:1693-1694`).
const GLYPH: f32 = 18.0;
const PADDING: f32 = 8.0;
const SPACING: f32 = 6.0;

/// A header toggle: its face and the name its tooltip shows. End with
/// [`HeaderToggle::view`].
pub struct HeaderToggle<'a, Message> {
    face: Face<'a, Message>,
    label: &'a str,
}

impl<'a, Message: Clone + 'a> HeaderToggle<'a, Message> {
    pub fn on_press(mut self, message: Message) -> Self {
        self.face = self.face.on_press(message);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.face = self.face.enabled(enabled);
        self
    }

    /// The widget, with its tooltip below it.
    pub fn view(self) -> impl View<Message> + 'a {
        self.face.tipped(self.label, Side::Below)
    }
}

/// A toggle showing `icon` and `text`; `label` names it in the tooltip, and
/// `open` keeps its ink lit while what it opens is shown, easing as it
/// changes.
pub fn header_toggle<'a, Message>(
    icon: Icon,
    text: &'a str,
    label: &'a str,
    open: bool,
    motion: Motion,
) -> HeaderToggle<'a, Message> {
    let mut face = Face::new(motion, move |theme: &Theme, hovered, _, open| {
        let palette = theme.palette();
        let fill = if hovered {
            palette.hover
        } else {
            Color::TRANSPARENT
        };
        Colours::new(
            fill,
            if open || hovered {
                palette.text
            } else {
                palette.muted
            },
        )
    });
    face.leading = Some(icon);
    face.text = text;
    face.glyph = GLYPH;
    face.text_size = typography::size::SMALL;
    face.font = typography::MEDIUM;
    face.padding = PADDING;
    face.spacing = SPACING;
    face.min = iced::Size::new(0.0, HEIGHT);
    face.open = open;
    face.press = 1.0;
    HeaderToggle { face, label }
}
