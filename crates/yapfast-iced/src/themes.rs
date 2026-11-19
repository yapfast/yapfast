//! The themes the window shows: one of the core's (`yapfast_core::theme`)
//! made the kit's [`Theme`]. See docs/themes.md.
//!
//! The theme is the one `layout.json` names: a built-in id or one of the
//! person's own (`custom_themes`), Bonfire when it names none, Yap for an
//! id Yapfast doesn't know. The window-control command `theme ID` chooses
//! another and saves it, as Appearance does; Appearance itself is
//! Settings' (`settings/appearance.rs`).

use std::path::Path;

use iced::Color;
use yapfast_core::theme::{self as palette, Colour, Custom};

use crate::{
    layout::{self, Layout},
    theme::Theme,
};

/// The theme shown when `layout.json` names none.
const DEFAULT: &str = "bonfire";

/// The themes this data folder offers and the one it chose.
pub struct Themes {
    pub custom: Vec<Custom>,
    pub chosen: Option<String>,
}

impl Themes {
    /// The themes `layout` keeps.
    pub fn read(layout: &Layout) -> Self {
        let mut custom = layout.custom_themes.clone();
        palette::tidy(&mut custom);
        Self {
            custom,
            chosen: layout.theme.clone(),
        }
    }

    /// The theme to show.
    pub fn shown(&self) -> Theme {
        let (id, found) = palette::chosen(self.chosen.as_deref().or(Some(DEFAULT)), &self.custom);
        theme(id, &found)
    }

    /// The chosen theme's id, as Appearance marks it.
    pub fn chosen_id(&self) -> &str {
        palette::chosen(self.chosen.as_deref().or(Some(DEFAULT)), &self.custom).0
    }

    /// `id`'s theme, if it's one Yapfast has or the person made.
    pub fn find(&self, id: &str) -> Option<Theme> {
        palette::lookup(id, &self.custom).map(|(id, found)| theme(id, &found))
    }

    /// Keeps `id` as the chosen theme in `layout.json`, leaving the rest of
    /// what's kept there as it is.
    pub fn choose(&mut self, root: &Path, id: &str) {
        self.chosen = Some(id.to_owned());
        layout::store(root, "theme", id);
    }
}

/// Fills the kit's theme from the core's.
macro_rules! from_palette {
    ($($name:ident,)*) => {
        /// The core's `theme`, named `id`, as the window paints with it.
        pub fn theme(id: &str, theme: &palette::Theme) -> Theme {
            let mut shown = Theme::new(id, theme.dark);
            shown.bonfire = theme.bonfire();
            $(shown.$name = color(theme.$name);)*
            shown.mention = color(theme.mention());
            shown.worked_out()
        }
    };
}

yapfast_iced_kit::for_theme_colours!(from_palette);

/// A core colour as iced's.
pub fn color(colour: Colour) -> Color {
    Color::from_rgba8(
        colour.red(),
        colour.green(),
        colour.blue(),
        f32::from(colour.alpha()) / 255.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LAYOUT;

    #[test]
    fn a_layout_without_a_theme_shows_bonfire_and_an_unknown_one_yap() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(
            Themes::read(&Layout::read(folder.path())).shown().id,
            "bonfire"
        );
        std::fs::write(
            folder.path().join(LAYOUT),
            r#"{"theme": "gone", "zoom": 1.2}"#,
        )
        .unwrap();
        assert_eq!(Themes::read(&Layout::read(folder.path())).shown().id, "yap");
    }

    #[test]
    fn choosing_a_theme_keeps_the_rest_of_the_layout() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LAYOUT),
            r##"{"zoom": 1.2, "custom_themes": [{"id": "custom:1", "name": "Plum",
                "base": "sakura", "colours": {"chat": "#2a1a3a"}}]}"##,
        )
        .unwrap();
        let mut themes = Themes::read(&Layout::read(folder.path()));
        let plum = themes.find("custom:1").unwrap();
        assert!(!plum.dark, "Sakura's");
        assert_eq!(plum.chat, crate::style::hex(0x2a1a3a));
        themes.choose(folder.path(), "custom:1");
        crate::layout::flush(folder.path());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.path().join(LAYOUT)).unwrap()).unwrap();
        assert_eq!(written["theme"], "custom:1");
        assert_eq!(written["zoom"], 1.2);
        assert_eq!(Themes::read(&Layout::read(folder.path())).shown(), plum);
    }

    #[test]
    fn the_worked_out_colours_follow_the_theme() {
        let yap = Themes {
            custom: Vec::new(),
            chosen: Some("yap".into()),
        }
        .shown();
        // Discord's menu on its dark theme is 40, 40, 45: a step over raised.
        let [r, g, b, _] = yap.menu.into_rgba8();
        assert!(r > 0x26 && g > 0x27 && b > 0x2e);
        assert_eq!(yap.danger_text, crate::style::hex(0xfa777c));
        let sakura = Themes {
            custom: Vec::new(),
            chosen: Some("sakura".into()),
        }
        .shown();
        assert_eq!(sakura.menu, sakura.raised);
        assert_eq!(sakura.danger_text, sakura.danger);
    }
}
