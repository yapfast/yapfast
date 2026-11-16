//! A theme's colours: the tokens `Theme` in `ui/theme.slint` holds, each
//! described there, and the colours derived from them.

use iced::Color;

/// Opaque `#rrggbb`.
pub const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// `#rrggbbaa`, as Slint and the theme files write a colour with alpha.
pub const fn rgba(hex: u32) -> Color {
    Color::from_rgba8(
        (hex >> 24) as u8,
        (hex >> 16) as u8,
        (hex >> 8) as u8,
        (hex & 0xff) as f32 / 255.0,
    )
}

/// Declares [`Palette`] with one field per colour token, and its list of
/// names, in the order `layout.json` and the theme files know them.
macro_rules! palette {
    ($($(#[$doc:meta])* $name:ident,)*) => {
        /// A full set of a theme's colours, and whether it is dark.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Palette {
            pub dark: bool,
            $($(#[$doc])* pub $name: Color,)*
        }

        impl Palette {
            /// Each colour's name, as `src/ui/theme.rs` has it: the Slint
            /// token with `_` for `-`.
            pub const NAMES: &[&str] = &[$(stringify!($name),)*];

            /// The colour at an index of [`Self::NAMES`].
            pub fn colour(&self, index: usize) -> Option<Color> {
                [$(self.$name,)*].get(index).copied()
            }

            pub fn colour_mut(&mut self, index: usize) -> Option<&mut Color> {
                [$(&mut self.$name,)*].into_iter().nth(index)
            }
        }
    };
}

palette! {
    /// The server rail: the deepest ground.
    rail,
    /// The channel and DM list, the member list and the settings' navigation.
    sidebar,
    /// The account card and the voice dock, settings pages, popover headers.
    panel,
    /// The conversation.
    chat,
    stage,
    /// Dialogs, popovers, cards, reaction chips.
    raised,
    /// The message box on the chat and a search field on the sidebar.
    field,
    /// A text field's well and a segmented control's trough.
    sunken,
    /// A row under the pointer.
    hover,
    /// The chosen row or segment.
    active,
    /// A message under the pointer.
    row_hover,
    /// Dividers.
    line,
    /// The border of what's raised.
    edge,
    /// The sidebar's outline and the top of the window.
    outline,
    /// Lines on the conversation's side.
    outline_chat,
    /// The message box's outline.
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
    /// Live voice: speaking rings, the dock's status.
    live,
    loud,
    link,
    /// The neutral face of a control.
    button_default,
    button_default_hover,
    /// Laid over a surface at low alpha to lift it on hover.
    overlay,
    /// The sweep across a skeleton while it loads.
    shimmer,
    reply_spine,
    tooltip,
    /// An avatar's ground before its person's own tint is known.
    avatar,
}

/// What Slint's `color.mix(other, factor)` gives: `factor` of `base` and the
/// rest of `other`.
pub fn mix(base: Color, other: Color, factor: f32) -> Color {
    let blend = |a: f32, b: f32| a * factor + b * (1.0 - factor);
    Color {
        r: blend(base.r, other.r),
        g: blend(base.g, other.g),
        b: blend(base.b, other.b),
        a: blend(base.a, other.a),
    }
}

impl Palette {
    /// A message that mentions the person: Discord's hsl(39.4 100% 48.6%) at
    /// 12%, and 16% under the pointer. The same in every theme.
    pub const MENTIONED: Color = rgba(0xf8a5_001f);
    pub const MENTIONED_HOVER: Color = rgba(0xf8a5_0029);

    /// A context menu's ground: a step above what's raised on a dark theme.
    pub fn menu(&self) -> Color {
        if self.dark {
            mix(self.raised, Color::WHITE, 0.98)
        } else {
            self.raised
        }
    }

    /// A context menu's outline and separators.
    pub fn menu_line(&self) -> Color {
        if self.dark {
            mix(self.menu(), Color::WHITE, 0.94)
        } else {
            self.line
        }
    }

    /// Discord's composer: a hair lighter than a plain field.
    pub fn composer(&self) -> Color {
        if self.dark {
            mix(self.field, Color::WHITE, 0.99)
        } else {
            self.field
        }
    }

    /// A text field's placeholder, dimmer than muted text on a dark theme.
    pub fn placeholder(&self) -> Color {
        if self.dark {
            mix(self.muted, self.field, 0.66)
        } else {
            self.muted
        }
    }

    /// A destructive menu row's text and icon.
    pub fn danger_text(&self) -> Color {
        if self.dark {
            rgb(0xfa777c)
        } else {
            self.danger
        }
    }

    /// A mention's text: Discord's light blurple on Bonfire, the link colour
    /// on the others.
    pub fn mention(&self) -> Color {
        if self.dark && self.link == crate::builtin::BONFIRE.link {
            rgb(0xc9cdfb)
        } else {
            self.link
        }
    }

    /// The overlay at an alpha: how a hovered surface lifts.
    pub fn lift(&self, alpha: f32) -> Color {
        self.overlay.scale_alpha(alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_as_slint_writes_it() {
        assert_eq!(rgb(0x5865f2), Color::from_rgb8(0x58, 0x65, 0xf2));
        let soft = rgba(0x5865_f226);
        assert_eq!((soft.r, soft.a), (0x58 as f32 / 255.0, 0x26 as f32 / 255.0));
    }

    #[test]
    fn every_token_is_named_once() {
        let palette = crate::builtin::YAP;
        assert_eq!(Palette::NAMES.len(), 37);
        assert!(palette.colour(Palette::NAMES.len()).is_none());
        let mut names = Palette::NAMES.to_vec();
        names.dedup();
        assert_eq!(names.len(), Palette::NAMES.len());
    }
}
