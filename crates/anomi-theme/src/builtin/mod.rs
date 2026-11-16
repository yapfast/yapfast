//! The themes Anomi comes with, as Appearance lists them: the dark ones,
//! then the light ones. `docs/themes.md` credits each palette.

mod community;
mod graphite;
mod light;

pub use community::{CATPPUCCIN_MOCHA, DRACULA, GRUVBOX_DARK, NORD, TOKYO_NIGHT};
pub use graphite::{BLACK, BONFIRE, MIDNIGHT, YAP};
pub use light::{CATPPUCCIN_LATTE, ROSE_PINE_DAWN, SAKURA};

use crate::Palette;

/// A built-in theme: the id `layout.json` stores and the name Appearance
/// shows.
#[derive(Debug)]
pub struct BuiltIn {
    pub id: &'static str,
    pub name: &'static str,
    pub palette: Palette,
}

/// The theme shown while `layout.json` names none: Bonfire, Discord's dark
/// theme, as the Slint interface in its Discord style picks
/// (`apply_appearance` in `src/ui.rs`).
pub const DEFAULT: &str = "bonfire";

pub const BUILT_IN: [BuiltIn; 12] = [
    BuiltIn {
        id: "yap",
        name: "Yap",
        palette: YAP,
    },
    BuiltIn {
        id: "bonfire",
        name: "Bonfire",
        palette: BONFIRE,
    },
    BuiltIn {
        id: "midnight",
        name: "Midnight",
        palette: MIDNIGHT,
    },
    BuiltIn {
        id: "black",
        name: "Black",
        palette: BLACK,
    },
    BuiltIn {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        palette: CATPPUCCIN_MOCHA,
    },
    BuiltIn {
        id: "dracula",
        name: "Dracula",
        palette: DRACULA,
    },
    BuiltIn {
        id: "nord",
        name: "Nord",
        palette: NORD,
    },
    BuiltIn {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        palette: GRUVBOX_DARK,
    },
    BuiltIn {
        id: "tokyo-night",
        name: "Tokyo Night",
        palette: TOKYO_NIGHT,
    },
    BuiltIn {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        palette: CATPPUCCIN_LATTE,
    },
    BuiltIn {
        id: "rose-pine-dawn",
        name: "Rosé Pine Dawn",
        palette: ROSE_PINE_DAWN,
    },
    BuiltIn {
        id: "sakura",
        name: "Sakura",
        palette: SAKURA,
    },
];

/// A built-in theme by its id.
pub fn find(id: &str) -> Option<&'static BuiltIn> {
    BUILT_IN.iter().find(|theme| theme.id == id)
}

/// The built-in theme a layout chose, or [`DEFAULT`] when it chose none or
/// one that isn't built in.
pub fn chosen(id: Option<&str>) -> &'static BuiltIn {
    id.and_then(find)
        .or_else(|| find(DEFAULT))
        .unwrap_or(&BUILT_IN[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_dark_ones_come_first() {
        for (at, theme) in BUILT_IN.iter().enumerate() {
            assert!(BUILT_IN[at + 1..].iter().all(|other| other.id != theme.id));
        }
        let first_light = BUILT_IN
            .iter()
            .position(|theme| !theme.palette.dark)
            .unwrap();
        assert!(
            BUILT_IN[first_light..]
                .iter()
                .all(|theme| !theme.palette.dark)
        );
    }

    #[test]
    fn an_unknown_id_falls_back_to_bonfire() {
        assert_eq!(chosen(None).id, "bonfire");
        assert_eq!(chosen(Some("custom:9")).id, "bonfire");
        assert_eq!(chosen(Some("sakura")).id, "sakura");
    }
}
