//! The iced theme of the whole interface: a palette with its id and name.

use std::sync::Arc;

use iced::theme::{self, Mode, palette::Seed};

use crate::{Palette, builtin};

/// The theme every widget draws with. Cloning it is cheap: the program's
/// `theme` function hands iced a clone on each redraw.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme(Arc<Inner>);

#[derive(Debug, PartialEq)]
struct Inner {
    id: String,
    name: String,
    palette: Palette,
}

impl Theme {
    pub fn new(id: impl Into<String>, name: impl Into<String>, palette: Palette) -> Self {
        Self(Arc::new(Inner {
            id: id.into(),
            name: name.into(),
            palette,
        }))
    }

    /// The built-in theme a layout chose; Bonfire for none or an unknown one.
    pub fn built_in(id: Option<&str>) -> Self {
        let theme = builtin::chosen(id);
        Self::new(theme.id, theme.name, theme.palette)
    }

    /// The id `layout.json` stores, such as `bonfire` or `custom:1`.
    pub fn id(&self) -> &str {
        &self.0.id
    }

    /// The name Appearance shows.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn palette(&self) -> &Palette {
        &self.0.palette
    }

    pub fn is_dark(&self) -> bool {
        self.0.palette.dark
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::built_in(None)
    }
}

impl theme::Base for Theme {
    fn default(preference: Mode) -> Self {
        match preference {
            Mode::Light => Self::built_in(Some("catppuccin-latte")),
            Mode::Dark | Mode::None => Self::built_in(None),
        }
    }

    fn mode(&self) -> Mode {
        if self.is_dark() {
            Mode::Dark
        } else {
            Mode::Light
        }
    }

    fn base(&self) -> theme::Style {
        theme::Style {
            background_color: self.palette().chat,
            text_color: self.palette().text,
        }
    }

    fn seed(&self) -> Option<Seed> {
        let palette = self.palette();
        Some(Seed {
            background: palette.chat,
            text: palette.text,
            primary: palette.accent,
            success: palette.live,
            warning: palette.loud,
            danger: palette.danger,
        })
    }

    /// iced tells themes apart by this: the id, unique where names may not be.
    fn name(&self) -> &str {
        self.id()
    }
}
