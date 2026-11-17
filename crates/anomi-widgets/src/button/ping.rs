//! The reply bar's switch for whether a reply mentions the person it replies
//! to, as Discord's "@ ON". The Slint side: `PingToggle`
//! (`ui/controls.slint:84`): 28 px tall, its label's width and 16 more, the
//! hover fill under the pointer, the label in the accent when on and muted
//! when off. It gives no more under a press than that, and has the focus ring
//! the others have.

use iced::Color;
use yapfast_lang::tr;

use anomi_theme::{Motion, Theme, typography};

use super::face::Face;
use super::look::Colours;

/// `ui/controls.slint:87`: the height, and the 8 px either side of the label.
const HEIGHT: f32 = 28.0;
const PADDING: f32 = 8.0;

/// The toggle for `on`; press it to send the opposite.
pub fn ping_toggle<'a, Message>(on: bool, motion: Motion) -> Face<'a, Message> {
    let mut face = Face::new(motion, move |theme: &Theme, hovered, _, on| {
        let palette = theme.palette();
        let fill = if hovered {
            palette.hover
        } else {
            Color::TRANSPARENT
        };
        Colours::new(fill, if on { palette.accent } else { palette.muted })
    });
    face.text = if on { tr!("@ On") } else { tr!("@ Off") };
    face.text_size = typography::size::LABEL;
    face.padding = PADDING;
    face.min = iced::Size::new(0.0, HEIGHT);
    face.open = on;
    face.press = 1.0;
    face
}
