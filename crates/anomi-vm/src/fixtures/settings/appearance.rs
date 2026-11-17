//! Samples of the Appearance settings page for tests and shots, after the
//! fake Discord's data: task S06's.

use iced::Color;

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::appearance::{Appearance, FontFile, ThemeCard};
use crate::settings::frame::Page;

/// The twelve themes Yapfast comes with, as `anomi_theme::builtin` has them:
/// id, name, and rail, sidebar, chat, raised, accent, text and muted.
const BUILT_IN: [(&str, &str, [u32; 7]); 12] = [
    (
        "yap",
        "Yap",
        [
            0x111215, 0x17181c, 0x1e1f24, 0x26272e, 0xffbf47, 0xeef0f4, 0x8d919c,
        ],
    ),
    (
        "bonfire",
        "Bonfire",
        [
            0x121214, 0x121214, 0x1a1a1e, 0x242429, 0x5865f2, 0xefeff1, 0x96979e,
        ],
    ),
    (
        "midnight",
        "Midnight",
        [
            0x060607, 0x09090b, 0x0c0c0e, 0x19191d, 0x5865f2, 0xe6e7ea, 0x878a93,
        ],
    ),
    (
        "black",
        "Black",
        [
            0x000000, 0x000000, 0x000000, 0x141417, 0x5865f2, 0xe2e3e6, 0x878a93,
        ],
    ),
    (
        "catppuccin-mocha",
        "Catppuccin Mocha",
        [
            0x11111b, 0x181825, 0x1e1e2e, 0x313244, 0xcba6f7, 0xcdd6f4, 0x9399b2,
        ],
    ),
    (
        "dracula",
        "Dracula",
        [
            0x191a21, 0x21222c, 0x282a36, 0x343746, 0xbd93f9, 0xf8f8f2, 0x9ea8c3,
        ],
    ),
    (
        "nord",
        "Nord",
        [
            0x242932, 0x292e39, 0x2e3440, 0x3b4252, 0x88c0d0, 0xeceff4, 0x99a1b0,
        ],
    ),
    (
        "gruvbox-dark",
        "Gruvbox Dark",
        [
            0x1d2021, 0x222424, 0x282828, 0x3c3836, 0xfe8019, 0xebdbb2, 0xa89984,
        ],
    ),
    (
        "tokyo-night",
        "Tokyo Night",
        [
            0x13131a, 0x16161e, 0x1a1b26, 0x1f2335, 0x7aa2f7, 0xc0caf5, 0x737aa2,
        ],
    ),
    (
        "catppuccin-latte",
        "Catppuccin Latte",
        [
            0xdce0e8, 0xe6e9ef, 0xeff1f5, 0xfafbfc, 0x8839ef, 0x4c4f69, 0x7c7f93,
        ],
    ),
    (
        "rose-pine-dawn",
        "Rosé Pine Dawn",
        [
            0xdfdad9, 0xf2e9e1, 0xfaf4ed, 0xfffdf9, 0xd7827e, 0x575279, 0x87829b,
        ],
    ),
    (
        "sakura",
        "Sakura",
        [
            0xf4d5e0, 0xfcecf2, 0xfff8fb, 0xf9e4ec, 0xd5588a, 0x4a3541, 0x957685,
        ],
    ),
];

fn colour(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn card(
    id: &str,
    name: &str,
    custom: bool,
    [rail, sidebar, chat, raised, accent, text, muted]: [u32; 7],
) -> ThemeCard {
    ThemeCard {
        id: id.into(),
        name: name.into(),
        custom,
        rail: colour(rail),
        sidebar: colour(sidebar),
        chat: colour(chat),
        raised: colour(raised),
        accent: colour(accent),
        text: colour(text),
        muted: colour(muted),
    }
}

/// Bonfire chosen among the twelve, English first in the languages, no font
/// files, every look on: the golden screenshot's.
pub fn appearance() -> Appearance {
    Appearance {
        themes: BUILT_IN
            .iter()
            .map(|(id, name, colours)| card(id, name, false, *colours))
            .collect(),
        theme_id: "bonfire".into(),
        languages: ["Русский", "English", "Deutsch", "Español", "Français"]
            .map(String::from)
            .into(),
        title_bar_choice: true,
        system_title_bar: true,
        ..Appearance::default()
    }
}

/// The window with the settings open on Appearance.
pub fn page() -> Vm {
    let mut vm = settings(Page::Appearance);
    vm.settings.appearance = appearance();
    vm
}

/// The page with Catppuccin Latte chosen, as the light golden screenshot has
/// it.
pub fn latte() -> Vm {
    let mut vm = page();
    vm.settings.appearance.theme_id = "catppuccin-latte".into();
    vm
}

/// With a theme of the person's own, a note on an import, two font files (one
/// that gave no face, one with a warning) and a note on the files left out.
pub fn with_fonts() -> Vm {
    let mut vm = page();
    let page = &mut vm.settings.appearance;
    page.themes.push(card(
        "custom-1",
        "My theme",
        true,
        [
            0x1b1d23, 0x20232a, 0x262a33, 0x30343f, 0xe0507a, 0xf2f2f5, 0x9a9eab,
        ],
    ));
    page.theme_id = "custom-1".into();
    page.note = "Imported My theme.".into();
    page.font_files = vec![
        FontFile {
            name: "Inter-SemiBold.ttf".into(),
            detail: "Inter, Semi Bold".into(),
            warning: "No italic: slanted text is drawn slanted by the system.".into(),
            weight: 600,
            ..FontFile::default()
        },
        FontFile {
            name: "broken.woff".into(),
            detail: "The file holds no font.".into(),
            problem: true,
            ..FontFile::default()
        },
    ];
    page.font_note = "notes.txt isn't a font file.".into();
    vm
}
