//! The parts a field is built from: the input's own colours, the italic
//! placeholder laid over it and the trailing picture.

use iced::font::{self, Font};
use iced::widget::text::{Ellipsis, Style as TextStyle, Wrapping};
use iced::widget::text_input::{Status, Style};
use iced::widget::{container, text};
use iced::{Background, Border, Color, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;

use crate::icon::{Icon, icon};

use super::TRAILING_ICON;

/// The colours the well's style reads, apart from the theme's.
#[derive(Clone, Copy)]
pub(super) struct Looks {
    pub ink: Option<Color>,
}

impl Looks {
    /// The input's own look: text, placeholder and selection. Its ground and
    /// rim are the well's (`well.rs`), so they are none here.
    pub fn style(self, theme: &Theme, status: Status) -> Style {
        let palette = theme.palette();
        let disabled = status == Status::Disabled;
        Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            // The placeholder is the overlay's (`placeholder`), as iced's
            // cannot elide; the input's own is always empty.
            placeholder: Color::TRANSPARENT,
            value: if disabled {
                palette.muted
            } else {
                self.ink.unwrap_or(palette.text)
            },
            selection: palette.accent.scale_alpha(0.35),
        }
    }
}

/// The placeholder over an empty input, elided at the padding like Slint's
/// (`overflow: elide`), in italics when asked, not catching the pointer.
pub(super) fn placeholder<'a, Message: 'a>(
    placeholder: &'a str,
    italic: bool,
    size: f32,
    padding: Padding,
    enabled: bool,
    ink: Option<Color>,
) -> Element<'a, Message> {
    let face = Font {
        style: if italic {
            font::Style::Italic
        } else {
            font::Style::Normal
        },
        ..typography::UI
    };
    container(
        text(placeholder)
            .size(size)
            .font(face)
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                TextStyle {
                    color: Some(if enabled {
                        ink.unwrap_or(palette.muted)
                    } else {
                        palette.faint
                    }),
                    selection: None,
                }
            }),
    )
    .padding(padding)
    .center_y(Length::Fill)
    .width(Length::Fill)
    .boxed()
}

/// The trailing picture, `TRAILING_ICON` square, its right edge `pad - 4`
/// from the well's (`x: width - pad - 20px + 4px`, the icon's own margin).
pub(super) fn trailing<'a, Message: 'a>(
    glyph: Icon,
    tint: Option<Color>,
    pad: f32,
) -> Element<'a, Message> {
    // Room alone: tiny-skia drops a tint's alpha and would paint it black.
    if tint.is_some_and(|tint| tint.a == 0.0) {
        return iced::widget::space().boxed();
    }
    let face = icon(glyph, TRAILING_ICON).style(move |theme: &Theme, _| iced::widget::svg::Style {
        color: Some(tint.unwrap_or(theme.palette().secondary)),
    });
    container(face)
        .padding(Padding::default().right((pad - 4.0).max(0.0)))
        .align_right(Length::Fill)
        .center_y(Length::Fill)
        .boxed()
}
