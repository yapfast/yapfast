//! The colours the preview's tabs wear: the interface's own, or what the
//! profile's theme sets (`ProfileTabs`' `ink`, `tile` and `line`,
//! `ui/server.slint:1065`-`:1069` and the call at `:2278`).

use iced::Color;
use iced::Font;
use iced::widget::text;

use anomi_theme::{Palette, Theme as Iced, rgba};
use anomi_vm::people::activity::card::Look as ActivityLook;
use anomi_vm::people::editor_preview::EditorPreview;
use anomi_vm::people::profile::Theme;

/// Which of the text's three strengths.
#[derive(Clone, Copy)]
pub enum Shade {
    Ink,
    Ink2,
    Ink3,
}

#[derive(Clone, Copy)]
pub struct Look {
    theme: Option<Theme>,
}

/// What an activity card wears with no theme: the palette's own.
const PLAIN: ActivityLook = ActivityLook {
    ink: None,
    ink2: None,
    ink3: None,
    ground: None,
    track: None,
};

impl Look {
    pub fn new(preview: &EditorPreview) -> Self {
        Self {
            theme: preview.theme,
        }
    }

    /// What an activity card is drawn in.
    pub fn activity(preview: &EditorPreview) -> &ActivityLook {
        preview
            .theme
            .as_ref()
            .map_or(&PLAIN, |theme| &theme.activity)
    }

    pub fn ink(&self, palette: &Palette) -> Color {
        self.theme.map_or(palette.text, |theme| theme.ink())
    }

    pub fn ink2(&self, palette: &Palette) -> Color {
        self.theme.map_or(palette.secondary, |theme| theme.ink2())
    }

    pub fn ink3(&self, palette: &Palette) -> Color {
        self.theme.map_or(palette.muted, |theme| theme.ink3())
    }

    /// A tile's ground: raised, or a white wash over a theme.
    pub fn tile(&self, palette: &Palette) -> Color {
        match self.theme {
            None => palette.raised,
            Some(theme) if theme.light => rgba(0xffffff3d),
            Some(_) => rgba(0xffffff14),
        }
    }

    /// The well a game's picture sits in: the field's, theme or not, as
    /// `ProfileTabs` leaves `BoardCard`'s `well` (`ui/server.slint:1124`).
    pub fn well(&self, palette: &Palette) -> Color {
        palette.field
    }

    /// The tab bar's rule.
    pub fn line(&self, palette: &Palette) -> Color {
        match self.theme {
            None => palette.line,
            Some(theme) => theme.ink().scale_alpha(0.12),
        }
    }

    pub fn shade(&self, shade: Shade, palette: &Palette) -> Color {
        match shade {
            Shade::Ink => self.ink(palette),
            Shade::Ink2 => self.ink2(palette),
            Shade::Ink3 => self.ink3(palette),
        }
    }

    /// Words in `shade`, `size` and `font`.
    pub fn words<'a>(
        self,
        words: impl text::IntoFragment<'a>,
        size: f32,
        font: Font,
        shade: Shade,
    ) -> text::Text<'a, Iced> {
        text::Text::new(words)
            .size(size)
            .font(font)
            .style(move |theme: &Iced| text::Style {
                color: Some(self.shade(shade, theme.palette())),
                ..text::Style::default()
            })
    }
}
