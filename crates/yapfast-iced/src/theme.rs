//! The window's colours, as every widget reads them: one of the core's
//! themes (`yapfast_core::theme`, the Slint client's `ui/theme.slint`) in
//! iced's colour type, with the colours the Slint global works out from
//! them. See docs/themes.md.
//!
//! The theme is the one `layout.json` names, as the Slint client keeps it:
//! a built-in id or one of the person's own (`custom_themes`), Bonfire when
//! it names none (the Slint client's default with its Discord look, which
//! is the only look this client has), Yap for an id Yapfast doesn't know.
//! The window-control command `theme ID` chooses another and saves it, as
//! Appearance does; Appearance itself is Settings', M3's.

use std::path::Path;

use iced::Color;
use yapfast_core::theme::{self as palette, Colour, Custom};

use crate::layout::{self, Layout};

/// The theme shown when `layout.json` names none: the Slint client's with
/// its Discord look on.
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
        let (id, theme) = palette::chosen(self.chosen.as_deref().or(Some(DEFAULT)), &self.custom);
        Theme::new(id, &theme)
    }

    /// The chosen theme's id, as Appearance marks it.
    pub fn chosen_id(&self) -> &str {
        palette::chosen(self.chosen.as_deref().or(Some(DEFAULT)), &self.custom).0
    }

    /// `id`'s theme, if it's one Yapfast has or the person made.
    pub fn find(&self, id: &str) -> Option<Theme> {
        palette::lookup(id, &self.custom).map(|(id, theme)| Theme::new(id, &theme))
    }

    /// Keeps `id` as the chosen theme in `layout.json`, leaving the rest of
    /// what the Slint client keeps there as it is.
    pub fn choose(&mut self, root: &Path, id: &str) {
        self.chosen = Some(id.to_owned());
        layout::store(root, "theme", id);
    }
}

/// Declares [`Theme`]'s colours, each the core's of the same name.
macro_rules! colours {
    ($($name:ident,)*) => {
        /// A theme's colours, as the widgets paint with them. `ui/theme.slint`
        /// says what each one paints.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Theme {
            /// The theme's id, as `layout.json` names it.
            pub id: String,
            pub dark: bool,
            $(pub $name: Color,)*
            /// What `ui/theme.slint` works out from those: a context menu's
            /// ground and outline, the composer's ground and placeholder, a
            /// destructive row's text.
            pub menu: Color,
            pub menu_line: Color,
            pub composer: Color,
            pub placeholder: Color,
            pub danger_text: Color,
            /// Text selected in the timeline and fields: the accent at 35%.
            pub selection: Color,
            /// A mention's colour without one of its own.
            pub mention: Color,
            /// What iced's own styles read: the theme's ground, text and
            /// accent.
            iced: iced::Theme,
        }

        impl Theme {
            pub fn new(id: &str, theme: &palette::Theme) -> Self {
                Self {
                    id: id.to_owned(),
                    dark: theme.dark,
                    $($name: color(theme.$name),)*
                    menu: Color::TRANSPARENT,
                    menu_line: Color::TRANSPARENT,
                    composer: Color::TRANSPARENT,
                    placeholder: Color::TRANSPARENT,
                    danger_text: Color::TRANSPARENT,
                    selection: Color::TRANSPARENT,
                    mention: color(theme.mention()),
                    iced: iced::Theme::Dark,
                }
                .worked_out()
            }
        }
    };
}

colours! {
    rail,
    sidebar,
    panel,
    chat,
    stage,
    raised,
    field,
    sunken,
    hover,
    active,
    row_hover,
    line,
    edge,
    outline,
    outline_chat,
    outline_field,
    text,
    secondary,
    muted,
    faint,
    accent,
    accent_hover,
    on_accent,
    accent_soft,
    danger,
    danger_hover,
    on_danger,
    live,
    loud,
    link,
    button_default,
    button_default_hover,
    overlay,
    shimmer,
    reply_spine,
    tooltip,
    avatar,
}

impl Theme {
    /// The colours `ui/theme.slint` works out from the others.
    fn worked_out(self) -> Self {
        let dark = self.dark;
        let iced = iced::Theme::custom(
            format!("Yapfast {}", self.id),
            iced::theme::Palette {
                background: self.chat,
                text: self.text,
                primary: self.accent,
                success: self.live,
                warning: self.loud,
                danger: self.danger,
            },
        );
        Self {
            menu: if dark {
                mix(self.raised, Color::WHITE, 0.98)
            } else {
                self.raised
            },
            menu_line: if dark {
                mix(self.line, Color::BLACK, 0.95)
            } else {
                self.line
            },
            composer: if dark {
                mix(self.field, Color::WHITE, 0.99)
            } else {
                self.field
            },
            placeholder: if dark {
                mix(self.muted, self.field, 0.66)
            } else {
                self.muted
            },
            danger_text: if dark {
                crate::style::hex(0xfa777c)
            } else {
                self.danger
            },
            selection: Color {
                a: 0.35,
                ..self.accent
            },
            iced,
            ..self
        }
    }

    /// iced's own theme in these colours, for the styles iced draws itself.
    pub fn iced(&self) -> iced::Theme {
        self.iced.clone()
    }

    /// The overlay colour at `alpha`: what lifts a surface under the
    /// pointer, white on a dark theme, dark on a light one.
    pub fn wash(&self, alpha: f32) -> Color {
        Color {
            a: alpha,
            ..self.overlay
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        let id = DEFAULT;
        Self::new(id, &palette::find(id).expect("Bonfire is built in").theme)
    }
}

/// A core colour as iced's.
pub fn color(colour: Colour) -> Color {
    Color::from_rgba8(
        colour.red(),
        colour.green(),
        colour.blue(),
        f32::from(colour.alpha()) / 255.0,
    )
}

/// An opaque colour as `0xrrggbb`, for the client's own pictures (icons,
/// dots), which are drawn once per colour.
pub fn rgb(color: Color) -> u32 {
    let [r, g, b, _] = color.into_rgba8();
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Slint's `mix`: `factor` of `color` and the rest of `other`, weighted by
/// their alpha as Sass mixes them.
pub fn mix(color: Color, other: Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let normal = 2.0 * factor - 1.0;
    let distance = color.a - other.a;
    let weighted = normal * distance;
    let combined = if weighted == -1.0 {
        normal
    } else {
        (normal + distance) / (1.0 + weighted)
    };
    let channels = (combined + 1.0) / 2.0;
    let lerp = |a: f32, b: f32, f: f32| (a * f + b * (1.0 - f)).clamp(0.0, 1.0);
    Color {
        r: lerp(color.r, other.r, channels),
        g: lerp(color.g, other.g, channels),
        b: lerp(color.b, other.b, channels),
        a: lerp(color.a, other.a, factor),
    }
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
    fn the_worked_out_colours_follow_ui_theme_slint() {
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
