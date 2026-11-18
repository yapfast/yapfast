//! A labelled button, optionally with a picture before and after its text.
//! The Slint side: `ActionButton` (`ui/controls.slint:8`). Its five looks are
//! the theme's `catalog::button::{secondary, primary, danger, ghost,
//! success}`; a disabled one fades whole, as `opacity: 0.5` there.

use iced::Background;
use iced::widget::button::{Status, Style};

use anomi_theme::catalog::button;
use anomi_theme::{Motion, Theme, metrics, typography};

use super::face::Face;
use super::look::Colours;
use crate::icon::Icon;

/// Which look an action takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The neutral face beside the main action.
    #[default]
    Secondary,
    /// The accent: the main action.
    Primary,
    /// Destructive.
    Danger,
    /// Nothing until the pointer lifts it.
    Ghost,
    /// Green, as a Discord app's button for a positive action.
    Success,
}

impl Kind {
    fn style(self) -> fn(&Theme, Status) -> Style {
        match self {
            Self::Secondary => button::secondary,
            Self::Primary => button::primary,
            Self::Danger => button::danger,
            Self::Ghost => button::ghost,
            Self::Success => button::success,
        }
    }
}

/// The height and smallest width of the normal and the small button
/// (`ui/controls.slint:21-22`).
const NORMAL: f32 = metrics::CONTROL_HEIGHT;
const SMALL: f32 = 28.0;
/// The picture before the text, normal and small (`:54`).
const GLYPH: f32 = 18.0;
const GLYPH_SMALL: f32 = 16.0;
/// The picture after it (`:62`). The face's default.
pub(super) const TRAILING: f32 = 16.0;
const TRAILING_SMALL: f32 = 14.0;
/// Between the edge and the content, and between the parts (`:51-52`).
const PADDING: f32 = 16.0;
const PADDING_SMALL: f32 = 10.0;
const SPACING: f32 = 8.0;

/// A button saying `text` (`text` may be empty for a picture alone). Chain
/// [`Face::on_press`], and [`leading`](Face::leading), [`trailing`](Face::trailing),
/// [`small`](Face::small).
pub fn action_button<'a, Message>(text: &'a str, kind: Kind, motion: Motion) -> Face<'a, Message> {
    let style = kind.style();
    let mut face = Face::new(motion, move |theme: &Theme, hovered, enabled, _open| {
        let status = match (enabled, hovered) {
            (false, _) => Status::Disabled,
            (true, true) => Status::Hovered,
            (true, false) => Status::Active,
        };
        let style = style(theme, status);
        let fill = match style.background {
            Some(Background::Color(color)) => color,
            _ => iced::Color::TRANSPARENT,
        };
        Colours::new(fill, style.text_color)
    });
    face.text = text;
    face.text_size = typography::size::BODY;
    face.glyph = GLYPH;
    face.trailing_glyph = TRAILING;
    face.padding = PADDING;
    face.spacing = SPACING;
    face.min = iced::Size::new(NORMAL, NORMAL);
    face
}

impl<'a, Message> Face<'a, Message> {
    /// A picture before its text, as a bot's button's server emoji.
    pub fn picture(mut self, picture: anomi_vm::kind::Expected) -> Self {
        self.picture = Some(picture);
        self
    }

    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// A picture after the text, such as the arrow of a button that opens a link.
    pub fn trailing(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }

    /// The smaller button: 28 px tall, 14 px text.
    pub fn small(mut self) -> Self {
        self.text_size = typography::size::SMALL;
        self.glyph = GLYPH_SMALL;
        self.trailing_glyph = TRAILING_SMALL;
        self.padding = PADDING_SMALL;
        self.min = iced::Size::new(SMALL, SMALL);
        self
    }
}
