//! Themes as packages: a `theme.json` and the files it names, which change how
//! Yapfast looks and never what it does. See `docs/themes.md`.
//!
//! A theme is data. [`package`] reads `theme.json` into named values, and
//! [`check`] decides whether a folder may be applied or listed: every colour
//! readable, every file bundled and of a kind Yapfast decodes, nothing reaching
//! outside the folder. [`motion`] works out a declared animation's value at a
//! moment, and a text effect's text, so the interface only draws what these
//! say. [`install`] files a fetched theme into the plugin store, beside
//! plugins, and the lock file pins it.
//!
//! There are no selectors. A theme names a token, a surface, a slot or a part
//! from closed lists Yapfast publishes, and Yapfast applies the value where it
//! already draws that thing; a name it doesn't know is ignored with a note, so
//! a theme written for a later Yapfast still opens.

pub mod check;
pub mod content;
pub mod install;
pub mod layout;
pub mod media;
pub mod motion;
pub mod package;

pub use package::{Mode, Package, Resolved};

/// The version of `theme.json` this Yapfast reads, which a theme names as
/// `theme_api`. Fields are only ever added within one version.
pub const THEME_API: u32 = 1;

/// What a theme's folder names its description.
pub const MANIFEST: &str = "theme.json";

/// Parts a theme may one day rebuild, and the contract version of each this
/// Yapfast offers: the fields a part receives. None are offered yet; a theme
/// that declares one draws the stock part and its page says so. A part's
/// minor version only adds fields, and a part built for another major draws
/// the stock part, so a theme never breaks on an update.
pub const PART_CONTRACTS: &[(&str, u32, u32)] = &[];

/// The parts a theme will be able to rebuild, by name, so a theme that
/// declares one is told it's for a later Yapfast rather than that the name is
/// wrong.
pub const PARTS: [&str; 5] = [
    "message-row",
    "member-row",
    "account-panel",
    "server-header",
    "profile-card",
];

/// How much of a theme runs, which Settings → Appearance sets and every
/// theme's cost is measured against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Performance {
    /// Everything, animations at the rate they ask for.
    Full,
    /// Animations marked `heavy` stay still, the rest run at most 30 times a
    /// second and pause while Yapfast isn't the focused window.
    #[default]
    Balanced,
    /// Colours, fonts, shapes and spacing; no declared animation, no text
    /// effect, and the stock hover and popup motion.
    Light,
}

impl Performance {
    pub const ALL: [Self; 3] = [Self::Full, Self::Balanced, Self::Light];

    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Balanced => "Balanced",
            Self::Light => "Light",
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Self::Full => 0,
            Self::Balanced => 1,
            Self::Light => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Full,
            2 => Self::Light,
            _ => Self::Balanced,
        }
    }
}
