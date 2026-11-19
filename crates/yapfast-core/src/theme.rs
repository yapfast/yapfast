//! The window's colours as data: a full set of them, the themes Yapfast
//! comes with, and the person's own as `layout.json` keeps them. The window
//! paints with them in its own colour type; see docs/themes.md.

/// A colour as `0xaarrggbb`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Colour(u32);

impl Colour {
    /// Opaque `#rrggbb`.
    pub const fn rgb(hex: u32) -> Self {
        Self(0xff00_0000 | hex)
    }

    /// `#rrggbbaa`: a colour with its alpha last.
    pub const fn rgba(hex: u32) -> Self {
        Self(hex.rotate_right(8))
    }

    /// From `0xaarrggbb`.
    pub const fn from_argb(argb: u32) -> Self {
        Self(argb)
    }

    pub const fn argb(self) -> u32 {
        self.0
    }

    /// `0xrrggbb`, without the alpha.
    pub const fn rgb_part(self) -> u32 {
        self.0 & 0xff_ffff
    }

    pub const fn red(self) -> u8 {
        (self.0 >> 16) as u8
    }

    pub const fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub const fn blue(self) -> u8 {
        self.0 as u8
    }

    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }
}

const fn rgb(hex: u32) -> Colour {
    Colour::rgb(hex)
}

const fn rgba(hex: u32) -> Colour {
    Colour::rgba(hex)
}

/// Declares `Theme` with one field per colour.
macro_rules! theme {
    ($($name:ident,)*) => {
        /// A full set of the window's colours, and whether they make a dark
        /// theme.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct Theme {
            pub dark: bool,
            $(pub $name: Colour,)*
        }

        impl Theme {
            /// Each colour's name, as `layout.json` has it but with `_` for
            /// `-`, in the order of [`Self::colour`].
            pub const NAMES: &[&str] = &[$(stringify!($name),)*];

            pub fn colour(&self, index: usize) -> Option<Colour> {
                [$(self.$name,)*].get(index).copied()
            }

            pub fn colour_mut(&mut self, index: usize) -> Option<&mut Colour> {
                [$(&mut self.$name,)*].into_iter().nth(index)
            }
        }
    };
}

theme! {
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

/// Yapfast's own colours: layered graphite with a marigold accent.
pub const YAP: Theme = Theme {
    dark: true,
    rail: rgb(0x111215),
    sidebar: rgb(0x17181c),
    panel: rgb(0x121316),
    chat: rgb(0x1e1f24),
    stage: rgb(0x1e1f24),
    raised: rgb(0x26272e),
    field: rgb(0x2a2b32),
    sunken: rgb(0x0e0f12),
    hover: rgb(0x222329),
    active: rgb(0x2c2d35),
    row_hover: rgb(0x1a1b20),
    line: rgb(0x2c2d34),
    outline: rgb(0x2c2d34),
    outline_chat: rgb(0x2c2d34),
    outline_field: rgb(0x2c2d34),
    edge: rgb(0x0c0d0f),
    text: rgb(0xeef0f4),
    secondary: rgb(0xb5b8c1),
    muted: rgb(0x8d919c),
    faint: rgb(0x5c606a),
    accent: rgb(0xffbf47),
    accent_hover: rgb(0xffcd70),
    on_accent: rgb(0x1d1607),
    accent_soft: rgba(0xffbf4726),
    danger: rgb(0xf0545e),
    danger_hover: rgb(0xf46f77),
    on_danger: rgb(0xffffff),
    live: rgb(0x3ecf7e),
    loud: rgb(0xff8c42),
    link: rgb(0x64adf6),
    button_default: rgb(0x33343c),
    button_default_hover: rgb(0x3c3d46),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x4a4c56),
    tooltip: rgb(0x0b0c0e),
    avatar: rgb(0x6b7280),
};

/// Discord's own dark theme (Dark, `.theme-dark` with `.theme-darker` in its
/// web client's CSS) on Yapfast's surfaces, with blurple for the accent.
const BONFIRE: Theme = Theme {
    dark: true,
    rail: rgb(0x121214),
    sidebar: rgb(0x121214),
    panel: rgb(0x202024),
    chat: rgb(0x1a1a1e),
    stage: rgb(0x1a1a1e),
    raised: rgb(0x242429),
    field: rgb(0x202024),
    sunken: rgb(0x121214),
    hover: rgb(0x222225),
    active: rgb(0x2c2c30),
    row_hover: rgb(0x242428),
    line: rgb(0x38383f),
    outline: rgb(0x222225),
    outline_chat: rgb(0x29292d),
    outline_field: rgb(0x27282b),
    edge: rgb(0x303036),
    text: rgb(0xefeff1),
    secondary: rgb(0xabacb2),
    muted: rgb(0x96979e),
    faint: rgb(0x6c6f7c),
    accent: rgb(0x5865f2),
    accent_hover: rgb(0x4452bb),
    on_accent: rgb(0xffffff),
    accent_soft: rgba(0x5865f226),
    danger: rgb(0xd22d39),
    danger_hover: rgb(0xa9232e),
    on_danger: rgb(0xffffff),
    live: rgb(0x23a55a),
    loud: rgb(0xf0b232),
    link: rgb(0x4d96ee),
    button_default: rgb(0x323237),
    button_default_hover: rgb(0x36363c),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x474851),
    tooltip: rgb(0x242429),
    avatar: rgb(0x5c5e66),
};

/// Catppuccin's darkest flavour, with mauve for the accent.
const CATPPUCCIN_MOCHA: Theme = Theme {
    dark: true,
    rail: rgb(0x11111b),
    sidebar: rgb(0x181825),
    panel: rgb(0x11111b),
    chat: rgb(0x1e1e2e),
    stage: rgb(0x1e1e2e),
    raised: rgb(0x313244),
    field: rgb(0x313244),
    sunken: rgb(0x11111b),
    hover: rgb(0x282839),
    active: rgb(0x45475a),
    row_hover: rgb(0x1a1a29),
    line: rgb(0x313244),
    outline: rgb(0x313244),
    outline_chat: rgb(0x313244),
    outline_field: rgb(0x313244),
    edge: rgb(0x11111b),
    text: rgb(0xcdd6f4),
    secondary: rgb(0xbac2de),
    muted: rgb(0x9399b2),
    faint: rgb(0x6c7086),
    accent: rgb(0xcba6f7),
    accent_hover: rgb(0xccb4f6),
    on_accent: rgb(0x11111b),
    accent_soft: rgba(0xcba6f726),
    danger: rgb(0xf38ba8),
    danger_hover: rgb(0xeba0ac),
    on_danger: rgb(0x11111b),
    live: rgb(0xa6e3a1),
    loud: rgb(0xfab387),
    link: rgb(0x89b4fa),
    button_default: rgb(0x45475a),
    button_default_hover: rgb(0x585b70),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x585b70),
    tooltip: rgb(0x11111b),
    avatar: rgb(0x6c7086),
};

/// Catppuccin's light flavour, with mauve for the accent.
const CATPPUCCIN_LATTE: Theme = Theme {
    dark: false,
    rail: rgb(0xdce0e8),
    sidebar: rgb(0xe6e9ef),
    panel: rgb(0xdce0e8),
    chat: rgb(0xeff1f5),
    stage: rgb(0xeff1f5),
    raised: rgb(0xe6e9ef),
    field: rgb(0xdce0e8),
    sunken: rgb(0xdce0e8),
    hover: rgb(0xe0e4eb),
    active: rgb(0xccd0da),
    row_hover: rgb(0xeaedf2),
    line: rgb(0xccd0da),
    outline: rgb(0xccd0da),
    outline_chat: rgb(0xccd0da),
    outline_field: rgb(0xccd0da),
    edge: rgb(0xbcc0cc),
    text: rgb(0x4c4f69),
    secondary: rgb(0x5c5f77),
    muted: rgb(0x7c7f93),
    faint: rgb(0x8c8fa1),
    accent: rgb(0x8839ef),
    accent_hover: rgb(0x793ece),
    on_accent: rgb(0xeff1f5),
    accent_soft: rgba(0x8839ef26),
    danger: rgb(0xd20f39),
    danger_hover: rgb(0xb01f45),
    on_danger: rgb(0xeff1f5),
    live: rgb(0x40a02b),
    loud: rgb(0xfe640b),
    link: rgb(0x1e66f5),
    button_default: rgb(0xccd0da),
    button_default_hover: rgb(0xbcc0cc),
    overlay: rgb(0x000000),
    shimmer: rgba(0xffffff80),
    reply_spine: rgb(0xacb0be),
    tooltip: rgb(0xeff1f5),
    avatar: rgb(0x7c7f93),
};

/// Dracula, with purple for the accent.
const DRACULA: Theme = Theme {
    dark: true,
    rail: rgb(0x191a21),
    sidebar: rgb(0x21222c),
    panel: rgb(0x191a21),
    chat: rgb(0x282a36),
    stage: rgb(0x282a36),
    raised: rgb(0x343746),
    field: rgb(0x343746),
    sunken: rgb(0x191a21),
    hover: rgb(0x2e303e),
    active: rgb(0x44475a),
    row_hover: rgb(0x242631),
    line: rgb(0x343746),
    outline: rgb(0x343746),
    outline_chat: rgb(0x343746),
    outline_field: rgb(0x343746),
    edge: rgb(0x191a21),
    text: rgb(0xf8f8f2),
    secondary: rgb(0xcbd0db),
    muted: rgb(0x9ea8c3),
    faint: rgb(0x6272a4),
    accent: rgb(0xbd93f9),
    accent_hover: rgb(0xccacf7),
    on_accent: rgb(0x191a21),
    accent_soft: rgba(0xbd93f926),
    danger: rgb(0xff5555),
    danger_hover: rgb(0xfe7674),
    on_danger: rgb(0x191a21),
    live: rgb(0x50fa7b),
    loud: rgb(0xffb86c),
    link: rgb(0x8be9fd),
    button_default: rgb(0x44475a),
    button_default_hover: rgb(0x4e5674),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x44475a),
    tooltip: rgb(0x191a21),
    avatar: rgb(0x6272a4),
};

/// Nord's Polar Night and Snow Storm, with frost for the accent. Polar
/// Night has no shade below nord0, so the rail and sidebar darken it.
const NORD: Theme = Theme {
    dark: true,
    rail: rgb(0x242932),
    sidebar: rgb(0x292e39),
    panel: rgb(0x242932),
    chat: rgb(0x2e3440),
    stage: rgb(0x2e3440),
    raised: rgb(0x3b4252),
    field: rgb(0x3b4252),
    sunken: rgb(0x21252e),
    hover: rgb(0x343b49),
    active: rgb(0x434c5e),
    row_hover: rgb(0x2b313c),
    line: rgb(0x3b4252),
    outline: rgb(0x3b4252),
    outline_chat: rgb(0x3b4252),
    outline_field: rgb(0x3b4252),
    edge: rgb(0x1f232c),
    text: rgb(0xeceff4),
    secondary: rgb(0xd8dee9),
    muted: rgb(0x99a1b0),
    faint: rgb(0x616e88),
    accent: rgb(0x88c0d0),
    accent_hover: rgb(0xa6cedb),
    on_accent: rgb(0x2e3440),
    accent_soft: rgba(0x88c0d026),
    danger: rgb(0xbf616a),
    danger_hover: rgb(0xc6767f),
    on_danger: rgb(0xeceff4),
    live: rgb(0xa3be8c),
    loud: rgb(0xd08770),
    link: rgb(0x81a1c1),
    button_default: rgb(0x434c5e),
    button_default_hover: rgb(0x4c566a),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x4c566a),
    tooltip: rgb(0x1f232c),
    avatar: rgb(0x616e88),
};

/// Gruvbox's dark palette at medium contrast, with orange for the accent.
const GRUVBOX_DARK: Theme = Theme {
    dark: true,
    rail: rgb(0x1d2021),
    sidebar: rgb(0x222424),
    panel: rgb(0x1d2021),
    chat: rgb(0x282828),
    stage: rgb(0x282828),
    raised: rgb(0x3c3836),
    field: rgb(0x32302f),
    sunken: rgb(0x171a1a),
    hover: rgb(0x32302f),
    active: rgb(0x504945),
    row_hover: rgb(0x222424),
    line: rgb(0x3c3836),
    outline: rgb(0x3c3836),
    outline_chat: rgb(0x3c3836),
    outline_field: rgb(0x3c3836),
    edge: rgb(0x141617),
    text: rgb(0xebdbb2),
    secondary: rgb(0xd5c4a1),
    muted: rgb(0xa89984),
    faint: rgb(0x7c6f64),
    accent: rgb(0xfe8019),
    accent_hover: rgb(0xfc9822),
    on_accent: rgb(0x1d2021),
    accent_soft: rgba(0xfe801926),
    danger: rgb(0xfb4934),
    danger_hover: rgb(0xfb6b51),
    on_danger: rgb(0x1d2021),
    live: rgb(0xb8bb26),
    loud: rgb(0xfabd2f),
    link: rgb(0x83a598),
    button_default: rgb(0x504945),
    button_default_hover: rgb(0x665c54),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x665c54),
    tooltip: rgb(0x141617),
    avatar: rgb(0x7c6f64),
};

/// Tokyo Night, the Night style, with blue for the accent.
const TOKYO_NIGHT: Theme = Theme {
    dark: true,
    rail: rgb(0x13131a),
    sidebar: rgb(0x16161e),
    panel: rgb(0x13131a),
    chat: rgb(0x1a1b26),
    stage: rgb(0x1a1b26),
    raised: rgb(0x1f2335),
    field: rgb(0x1f2335),
    sunken: rgb(0x121218),
    hover: rgb(0x1c1f2e),
    active: rgb(0x292e42),
    row_hover: rgb(0x181822),
    line: rgb(0x292e42),
    outline: rgb(0x292e42),
    outline_chat: rgb(0x292e42),
    outline_field: rgb(0x292e42),
    edge: rgb(0x0f0f15),
    text: rgb(0xc0caf5),
    secondary: rgb(0xa9b1d6),
    muted: rgb(0x737aa2),
    faint: rgb(0x545c7e),
    accent: rgb(0x7aa2f7),
    accent_hover: rgb(0x80bafa),
    on_accent: rgb(0x16161e),
    accent_soft: rgba(0x7aa2f726),
    danger: rgb(0xf7768e),
    danger_hover: rgb(0xec87a3),
    on_danger: rgb(0x16161e),
    live: rgb(0x9ece6a),
    loud: rgb(0xff9e64),
    link: rgb(0x7dcfff),
    button_default: rgb(0x292e42),
    button_default_hover: rgb(0x3b4261),
    overlay: rgb(0xffffff),
    shimmer: rgba(0xffffff10),
    reply_spine: rgb(0x3b4261),
    tooltip: rgb(0x0f0f15),
    avatar: rgb(0x565f89),
};

/// Rosé Pine's light variant, with rose for the accent.
const ROSE_PINE_DAWN: Theme = Theme {
    dark: false,
    rail: rgb(0xdfdad9),
    sidebar: rgb(0xf2e9e1),
    panel: rgb(0xdfdad9),
    chat: rgb(0xfaf4ed),
    stage: rgb(0xfaf4ed),
    raised: rgb(0xfffaf3),
    field: rgb(0xf2e9e1),
    sunken: rgb(0xf4ede8),
    hover: rgb(0xeae3de),
    active: rgb(0xdfdad9),
    row_hover: rgb(0xf4ede8),
    line: rgb(0xdfdad9),
    outline: rgb(0xdfdad9),
    outline_chat: rgb(0xdfdad9),
    outline_field: rgb(0xdfdad9),
    edge: rgb(0xcecacd),
    text: rgb(0x575279),
    secondary: rgb(0x797593),
    muted: rgb(0x87829b),
    faint: rgb(0x9893a5),
    accent: rgb(0xd7827e),
    accent_hover: rgb(0xc9767c),
    on_accent: rgb(0x39354f),
    accent_soft: rgba(0xd7827e2e),
    danger: rgb(0xb4637a),
    danger_hover: rgb(0x9d5f7a),
    on_danger: rgb(0xfffaf3),
    live: rgb(0x56949f),
    loud: rgb(0xea9d34),
    link: rgb(0x286983),
    button_default: rgb(0xdfdad9),
    button_default_hover: rgb(0xcecacd),
    overlay: rgb(0x575279),
    shimmer: rgba(0xffffff99),
    reply_spine: rgb(0xcecacd),
    tooltip: rgb(0xfffaf3),
    avatar: rgb(0x9893a5),
};

/// Yapfast's own: cherry blossom on rice paper, pink on a pale pink ground.
const SAKURA: Theme = Theme {
    dark: false,
    rail: rgb(0xf6dbe5),
    sidebar: rgb(0xfcecf2),
    panel: rgb(0xf6dbe5),
    chat: rgb(0xfff8fb),
    stage: rgb(0xfff8fb),
    raised: rgb(0xfbe9f0),
    field: rgb(0xfbe5ee),
    sunken: rgb(0xf8e1ea),
    hover: rgb(0xf8e2eb),
    active: rgb(0xf2d1de),
    row_hover: rgb(0xfdf1f6),
    line: rgb(0xf2d6e1),
    outline: rgb(0xf2d6e1),
    outline_chat: rgb(0xf2d6e1),
    outline_field: rgb(0xf2d6e1),
    edge: rgb(0xe9c6d3),
    text: rgb(0x4a3541),
    secondary: rgb(0x6d5361),
    muted: rgb(0x957685),
    faint: rgb(0xa8899a),
    accent: rgb(0xd5588a),
    accent_hover: rgb(0xc44a7b),
    on_accent: rgb(0xffffff),
    accent_soft: rgba(0xd5588a24),
    danger: rgb(0xd2404f),
    danger_hover: rgb(0xbd3242),
    on_danger: rgb(0xffffff),
    live: rgb(0x2f9e6e),
    loud: rgb(0xee8a45),
    link: rgb(0x6a5ad0),
    button_default: rgb(0xf4d6e1),
    button_default_hover: rgb(0xedc6d4),
    overlay: rgb(0x6b2d4a),
    shimmer: rgba(0xffffffa0),
    reply_spine: rgb(0xe2bccb),
    tooltip: rgb(0xffffff),
    avatar: rgb(0xc08aa2),
};

impl Theme {
    /// Whether it's Bonfire, or a theme of the person's own made from it:
    /// dark, with Bonfire's link colour. Bonfire draws a few things in
    /// Discord's measured inks rather than its palette's.
    pub fn bonfire(&self) -> bool {
        self.dark && self.link.rgb_part() == BONFIRE.link.rgb_part()
    }

    /// The colour of a mention without one of its own: the link colour,
    /// but Bonfire draws a mention's text as Discord's dark theme does, a
    /// light blurple.
    pub fn mention(&self) -> Colour {
        if self.bonfire() {
            rgb(0xc9cdfb)
        } else {
            self.link
        }
    }
}

/// A theme Yapfast comes with: the id a layout stores and the name Appearance
/// shows.
pub struct BuiltIn {
    pub id: &'static str,
    pub name: &'static str,
    pub theme: Theme,
}

/// The themes Yapfast comes with, as Appearance lists them: the dark ones, then
/// the light ones. The palettes credited in docs/themes.md are MIT licensed.
pub const BUILT_IN: [BuiltIn; 10] = [
    BuiltIn {
        id: "yap",
        name: "Yap",
        theme: YAP,
    },
    BuiltIn {
        id: "bonfire",
        name: "Bonfire",
        theme: BONFIRE,
    },
    BuiltIn {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        theme: CATPPUCCIN_MOCHA,
    },
    BuiltIn {
        id: "dracula",
        name: "Dracula",
        theme: DRACULA,
    },
    BuiltIn {
        id: "nord",
        name: "Nord",
        theme: NORD,
    },
    BuiltIn {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        theme: GRUVBOX_DARK,
    },
    BuiltIn {
        id: "tokyo-night",
        name: "Tokyo Night",
        theme: TOKYO_NIGHT,
    },
    BuiltIn {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        theme: CATPPUCCIN_LATTE,
    },
    BuiltIn {
        id: "rose-pine-dawn",
        name: "Rosé Pine Dawn",
        theme: ROSE_PINE_DAWN,
    },
    BuiltIn {
        id: "sakura",
        name: "Sakura",
        theme: SAKURA,
    },
];

pub fn find(id: &str) -> Option<&'static BuiltIn> {
    BUILT_IN.iter().find(|theme| theme.id == id)
}

/// A theme Appearance offers, built in or the person's own, and its id.
pub fn lookup<'a>(id: &str, custom: &'a [Custom]) -> Option<(&'a str, Theme)> {
    find(id)
        .map(|built_in| (built_in.id, built_in.theme))
        .or_else(|| {
            custom
                .iter()
                .find(|custom| custom.id == id)
                .map(|custom| (custom.id.as_str(), custom.theme))
        })
}

/// The theme a layout chose, and its id: Yap when it chose none, or one
/// this version doesn't have or the person deleted.
pub fn chosen<'a>(id: Option<&str>, custom: &'a [Custom]) -> (&'a str, Theme) {
    id.and_then(|id| lookup(id, custom))
        .unwrap_or((BUILT_IN[0].id, YAP))
}

/// A colour's name as `layout.json` has it.
pub fn token(name: &str) -> String {
    name.replace('_', "-")
}

/// Where a colour named as `layout.json` has it is in [`Theme::colour`].
fn token_index(token: &str) -> Option<usize> {
    Theme::NAMES.iter().position(|name| {
        name.bytes()
            .map(|byte| if byte == b'_' { b'-' } else { byte })
            .eq(token.bytes())
    })
}

/// A colour written `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse(text: &str) -> Option<Colour> {
    let digits = text.trim().strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        3 => {
            let nibble = |shift: u32| ((value >> shift) & 0xf) * 0x11;
            Some(rgb((nibble(8) << 16) | (nibble(4) << 8) | nibble(0)))
        }
        6 => Some(rgb(value)),
        8 => Some(rgba(value)),
        _ => None,
    }
}

/// A colour as `#rrggbb`, or `#rrggbbaa` when it isn't opaque.
pub fn hex(colour: Colour) -> String {
    let rgb = colour.rgb_part();
    match colour.alpha() {
        0xff => format!("#{rgb:06x}"),
        alpha => format!("#{rgb:06x}{alpha:02x}"),
    }
}

/// One of the person's own themes, as a layout keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Custom {
    /// `custom:` and a number: no built-in id has a colon. See [`tidy`].
    pub id: String,
    pub name: String,
    /// The built-in theme it began as a copy of, whose colours stand in for
    /// any it doesn't name, such as one added since it was saved.
    pub base: &'static str,
    pub theme: Theme,
}

/// A custom theme in `layout.json`: each colour as text, by its name as
/// [`token`] gives it.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Stored {
    id: String,
    name: String,
    base: String,
    dark: Option<bool>,
    colours: std::collections::BTreeMap<String, String>,
}

impl From<Stored> for Custom {
    fn from(stored: Stored) -> Self {
        let base = find(&stored.base).unwrap_or(&BUILT_IN[0]);
        let mut theme = base.theme;
        theme.dark = stored.dark.unwrap_or(theme.dark);
        for (token, text) in &stored.colours {
            if let (Some(slot), Some(colour)) = (
                token_index(token).and_then(|index| theme.colour_mut(index)),
                parse(text),
            ) {
                *slot = colour;
            }
        }
        Self {
            id: stored.id,
            name: stored.name,
            base: base.id,
            theme,
        }
    }
}

impl From<&Custom> for Stored {
    fn from(custom: &Custom) -> Self {
        let colours = Theme::NAMES
            .iter()
            .enumerate()
            .filter_map(|(index, name)| Some((token(name), hex(custom.theme.colour(index)?))))
            .collect();
        Self {
            id: custom.id.clone(),
            name: custom.name.clone(),
            base: custom.base.to_owned(),
            dark: Some(custom.theme.dark),
            colours,
        }
    }
}

impl serde::Serialize for Custom {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Stored::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Custom {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Stored::deserialize(deserializer).map(Self::from)
    }
}

/// The number in a custom theme's id, if it's one Yapfast gives.
fn custom_number(id: &str) -> Option<u32> {
    id.strip_prefix("custom:")?.parse().ok()
}

/// An id for a new custom theme, after those there are.
pub fn new_id(custom: &[Custom]) -> String {
    let last = custom
        .iter()
        .filter_map(|custom| custom_number(&custom.id))
        .max()
        .unwrap_or(0);
    format!("custom:{}", last + 1)
}

/// Gives a new id to each custom theme from `layout.json` whose id Yapfast
/// wouldn't have given, or that another has, so that ids stay apart from
/// the built-in ones and from each other.
pub fn tidy(custom: &mut [Custom]) {
    let mut seen = std::collections::HashSet::new();
    let unfit: Vec<usize> = (0..custom.len())
        .filter(|&index| {
            custom_number(&custom[index].id).is_none() || !seen.insert(custom[index].id.clone())
        })
        .collect();
    for index in unfit {
        custom[index].id = new_id(custom);
    }
}

/// A theme a new one can start as.
pub struct Start<'a> {
    pub id: &'a str,
    /// The built-in theme it is or began as.
    pub base: &'static str,
    pub name: &'a str,
    pub theme: Theme,
}

/// What a new theme can start as: every theme Appearance offers, in its
/// order.
pub fn starts(custom: &[Custom]) -> impl Iterator<Item = Start<'_>> {
    BUILT_IN
        .iter()
        .map(|built_in| Start {
            id: built_in.id,
            base: built_in.id,
            name: built_in.name,
            theme: built_in.theme,
        })
        .chain(custom.iter().map(|custom| Start {
            id: &custom.id,
            base: custom.base,
            name: &custom.name,
            theme: custom.theme,
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_and_write_as_hex() {
        assert_eq!(parse("#ffbf47"), Some(Colour::rgb(0xffbf47)));
        assert_eq!(parse("#fff"), Some(Colour::rgb(0xffffff)));
        assert_eq!(parse("#ffbf4726").map(Colour::alpha), Some(0x26));
        assert_eq!(parse("ffbf47"), None);
        assert_eq!(hex(Colour::rgba(0xffffff10)), "#ffffff10");
        assert_eq!(hex(YAP.accent), "#ffbf47");
    }

    #[test]
    fn a_custom_theme_takes_its_bases_colour_for_any_it_lacks() {
        let stored = r##"{"id": "custom:1", "name": "Plum", "base": "nord",
            "colours": {"chat": "#2a1a3a", "nonsense": "#000", "rail": "blue"}}"##;
        let custom: Custom = serde_json::from_str(stored).unwrap();
        let nord = find("nord").unwrap().theme;
        assert_eq!(custom.base, "nord");
        assert_eq!(custom.theme.chat, Colour::rgb(0x2a1a3a));
        assert_eq!(custom.theme.rail, nord.rail, "not a colour");
        assert_eq!(custom.theme.dark, nord.dark);
        assert_eq!(
            chosen(Some("custom:1"), std::slice::from_ref(&custom)).1,
            custom.theme
        );
        assert_eq!(chosen(Some("gone"), &[custom]).0, "yap");
    }
}
