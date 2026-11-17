//! A button drawn as its words alone, for an action that starts a row: they
//! start on the column's edge, under the text above, where a ghost button's
//! padding would set them in, and brighten under the pointer. The Slint
//! side: `LinkButton` (`ui/settings.slint:259`).

use std::borrow::Cow;

use iced::Color;

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;

use super::leaf::{Ink, Leaf};
use super::press::mix;

/// The glyph and the line of a link, regular and small (`ui/settings.slint:
/// 263-281`).
const GLYPH: f32 = 18.0;
const GLYPH_SMALL: f32 = 16.0;
const LINE: f32 = 36.0;
const LINE_SMALL: f32 = 28.0;

/// The ring stands this far outside the words (`x: -6px`).
const REACH: f32 = 6.0;

/// A link button saying `text`; chain [`Link::icon`], [`Link::danger`],
/// [`Link::small`] and [`Link::enabled`], and end with [`Link::view`].
pub struct Link<'a, M> {
    text: Cow<'a, str>,
    icon: Option<Icon>,
    danger: bool,
    small: bool,
    enabled: bool,
    on_press: Option<M>,
    motion: Motion,
}

pub fn link<'a, M>(text: impl Into<Cow<'a, str>>, motion: Motion) -> Link<'a, M> {
    Link {
        text: text.into(),
        icon: None,
        danger: false,
        small: false,
        enabled: true,
        on_press: None,
        motion,
    }
}

impl<'a, M: Clone + 'a> Link<'a, M> {
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    /// 28 px tall with 14 px words, instead of 36 and 15.
    pub fn small(mut self, small: bool) -> Self {
        self.small = small;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn view(self) -> Element<'a, M> {
        let (danger, enabled) = (self.danger, self.enabled);
        let mut leaf = Leaf::new(self.text, self.motion, move |theme: &Theme, lit: f32| {
            let palette = theme.palette();
            let ink = if !enabled {
                palette.faint
            } else if danger {
                palette.danger_text()
            } else {
                mix(palette.secondary, palette.text, lit)
            };
            Ink {
                fill: Color::TRANSPARENT,
                text: ink,
                glyph: ink,
            }
        })
        .enabled(self.enabled);
        leaf.on_press = self.on_press;
        leaf.icon = self.icon;
        leaf.glyph = if self.small { GLYPH_SMALL } else { GLYPH };
        leaf.size = if self.small {
            typography::size::SMALL
        } else {
            typography::size::BODY
        };
        leaf.font = typography::BOLD;
        leaf.height = if self.small { LINE_SMALL } else { LINE };
        leaf.outset = REACH;
        leaf.boxed()
    }
}
