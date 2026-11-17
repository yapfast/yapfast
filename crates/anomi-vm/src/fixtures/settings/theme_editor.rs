//! Samples of the theme editor for tests and shots, after the fake Discord's
//! data: task S07's.

use iced::Color;
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::settings::appearance::page;
use crate::settings::theme_editor::{Colour, ThemeEditor};

/// Bonfire's colours by name, as `anomi_theme::builtin` has them: the ones
/// above `0xffffff` are `rrggbbaa`.
const BONFIRE: [(&str, u32); 37] = [
    ("rail", 0x121214),
    ("sidebar", 0x121214),
    ("panel", 0x202024),
    ("chat", 0x1a1a1e),
    ("stage", 0x1a1a1e),
    ("raised", 0x242429),
    ("field", 0x202024),
    ("sunken", 0x121214),
    ("hover", 0x222225),
    ("active", 0x2d2d30),
    ("row-hover", 0x1f1f23),
    ("line", 0x38383f),
    ("outline", 0x222225),
    ("outline-chat", 0x29292d),
    ("outline-field", 0x27282b),
    ("edge", 0x303036),
    ("text", 0xefeff1),
    ("secondary", 0xabacb2),
    ("muted", 0x96979e),
    ("faint", 0x6c6f7c),
    ("accent", 0x5865f2),
    ("accent-hover", 0x4452bb),
    ("on-accent", 0xffffff),
    ("accent-soft", 0x5865f226),
    ("danger", 0xd22d39),
    ("danger-hover", 0xa9232e),
    ("on-danger", 0xffffff),
    ("live", 0x23a55a),
    ("loud", 0xf0b232),
    ("link", 0x4d96ee),
    ("button-default", 0x323237),
    ("button-default-hover", 0x36363c),
    ("overlay", 0xffffff),
    ("shimmer", 0xffffff10),
    ("reply-spine", 0x474851),
    ("tooltip", 0x242429),
    ("avatar", 0x5c5e66),
];

fn colour(value: u32) -> Color {
    if value > 0xff_ffff {
        Color::from_rgba8(
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            f32::from(value as u8) / 255.0,
        )
    } else {
        Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }
}

/// The editor on a new theme copied from Bonfire.
pub fn editor() -> ThemeEditor {
    ThemeEditor {
        saved: false,
        name: tr!("{} copy", "Bonfire"),
        dark: true,
        starts: [
            "Yap",
            "Bonfire",
            "Midnight",
            "Black",
            "Catppuccin Mocha",
            "Dracula",
            "Nord",
            "Gruvbox Dark",
            "Tokyo Night",
            "Catppuccin Latte",
            "Rose Pine Dawn",
            "Sakura",
        ]
        .map(String::from)
        .into(),
        start: 1,
        colours: BONFIRE
            .iter()
            .map(|&(name, value)| Colour::new(name, colour(value)))
            .collect(),
        ..ThemeEditor::default()
    }
}

/// The editor of a saved theme, with an export note.
pub fn saved() -> ThemeEditor {
    ThemeEditor {
        saved: true,
        name: "Evening".into(),
        starts: Vec::new(),
        note: tr!("Exported to {}.", "Evening.yapfast-theme.json"),
        ..editor()
    }
}

/// The window on the Appearance page with the editor open on a new theme.
pub fn open() -> Vm {
    let mut vm = page();
    vm.settings.appearance.editing = true;
    vm.settings.theme_editor = editor();
    vm
}

/// A saved theme's editor with a colour's field holding no colour.
pub fn wrong() -> Vm {
    let mut vm = page();
    vm.settings.appearance.editing = true;
    let mut editor = saved();
    editor.colours[3].text = "#12zz".into();
    editor.colours[3].wrong = true;
    vm.settings.theme_editor = editor;
    vm
}
