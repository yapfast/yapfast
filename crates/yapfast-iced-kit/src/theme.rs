//! The window's colours, as every widget reads them, in iced's colour type,
//! with the colours worked out from them. The app fills one from a theme of
//! its own ([`Theme::new`], then [`Theme::worked_out`]). See docs/themes.md.

use iced::Color;

/// Calls `$then!` with the names of [`Theme`]'s own colours, so the app
/// that fills them names each once too.
#[macro_export]
macro_rules! for_theme_colours {
    ($then:ident) => {
        $then! {
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
    };
}

/// Declares [`Theme`] with its colours.
macro_rules! declare_theme {
    ($($name:ident,)*) => {
        /// A theme's colours, as the widgets paint with them.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Theme {
            /// The theme's id, as `layout.json` names it.
            pub id: String,
            pub dark: bool,
            /// Bonfire or a theme made from it, which draws a few things in
            /// Discord's measured dark-theme inks instead of its palette's.
            pub bonfire: bool,
            $(pub $name: Color,)*
            /// What's worked out from those: a context menu's ground and
            /// outline, the composer's ground and placeholder, a destructive
            /// row's text.
            pub menu: Color,
            pub menu_line: Color,
            pub composer: Color,
            pub placeholder: Color,
            pub danger_text: Color,
            /// The channel list's inks: a read channel's name, glyph and a
            /// category's name, and the open or unread channel's name. Bonfire
            /// draws Discord's, measured; other themes the muted and text
            /// colours.
            pub channel_read: Color,
            pub channel_white: Color,
            /// The Friends and Requests rows above the DM list: inactive,
            /// and open (Discord's on Bonfire; the muted and text colours on
            /// other themes).
            pub nav_read: Color,
            pub nav_open: Color,
            /// Text selected in the timeline and fields: the accent at 35%.
            pub selection: Color,
            /// A mention's colour without one of its own.
            pub mention: Color,
            /// What iced's own styles read: the theme's ground, text and
            /// accent.
            iced: iced::Theme,
        }

        impl Theme {
            /// A theme named `id`, every colour clear, for the app to fill
            /// and then [`work out`](Theme::worked_out).
            pub fn new(id: &str, dark: bool) -> Self {
                Self {
                    id: id.to_owned(),
                    dark,
                    bonfire: false,
                    $($name: Color::TRANSPARENT,)*
                    menu: Color::TRANSPARENT,
                    menu_line: Color::TRANSPARENT,
                    composer: Color::TRANSPARENT,
                    placeholder: Color::TRANSPARENT,
                    danger_text: Color::TRANSPARENT,
                    channel_read: Color::TRANSPARENT,
                    channel_white: Color::TRANSPARENT,
                    nav_read: Color::TRANSPARENT,
                    nav_open: Color::TRANSPARENT,
                    selection: Color::TRANSPARENT,
                    mention: Color::TRANSPARENT,
                    iced: iced::Theme::Dark,
                }
            }
        }
    };
}

for_theme_colours!(declare_theme);

impl Theme {
    /// The colours worked out from the others, once those are set.
    pub fn worked_out(self) -> Self {
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
            channel_read: if self.bonfire {
                crate::style::hex(0x81828a)
            } else {
                self.muted
            },
            channel_white: if self.bonfire {
                crate::style::hex(0xfbfbfb)
            } else {
                self.text
            },
            nav_read: if self.bonfire {
                crate::style::hex(0x96979e)
            } else {
                self.muted
            },
            nav_open: if self.bonfire {
                crate::style::hex(0xefeff1)
            } else {
                self.text
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
