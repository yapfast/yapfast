//! Appearance: the themes as cards, the one chosen, the languages, the font
//! files, the title bar and the profile switches (`apply_appearance`,
//! `theme::cards`, `language::show` and `Fonts::show` of the Slint app).
//! [`Kept`] holds what the intents keep for Settings between refreshes: the
//! person's own themes typed, the theme being edited, the fonts loaded.

use anomi_theme::custom::{self, Custom};
use anomi_theme::fonts::Repaint;
use anomi_theme::fonts::custom::{Loaded, Problem};
use anomi_theme::{Palette, builtin};
use anomi_vm::settings::appearance::{Appearance, FontFile, ThemeCard};
use yapfast_lang::{tr, trn};

use crate::layout::Layout;

/// What Settings' intents keep beside the view-model, which the pages show.
#[derive(Default)]
pub struct Kept {
    /// Counts the changes to what follows that a page shows.
    pub(crate) version: u64,
    /// The person's own themes, as `layout.custom_themes` holds them, with
    /// their ids tidied as the Slint app's `Layout::read` tidies them.
    pub(crate) custom: Vec<Custom>,
    /// The theme being edited, while it is.
    pub(crate) editor: Option<Editor>,
    /// How the last import or export of a theme file went, and whether it
    /// went wrong; said until Appearance is left or the themes change.
    pub(crate) note: Option<(String, bool)>,
    /// The person's font files, loaded in place of DM Sans.
    pub(crate) fonts: Loaded,
    /// The files the last choice brought that weren't added, and why.
    pub(crate) refused: Vec<(String, Problem)>,
    /// Files chosen are being copied in.
    pub(crate) importing: bool,
    /// The window's background, moved as the font changes so every pixel is
    /// drawn again (`anomi_theme::fonts::redrawn`).
    pub(crate) repaint: Repaint,
    /// A tray is there to close the window to, as Settings opened.
    pub(crate) tray: bool,
    /// The voice gate's level and the microphone test, while measured.
    #[cfg(feature = "discord")]
    pub(crate) gate: Option<crate::intent::settings::gate::Gate>,
    /// The camera filming for Voice & video's test, while it does.
    pub(crate) camera: Option<crate::intent::settings::camera::Filming>,
    /// The copies of the build, by number: the latest says so for 2 s.
    pub(crate) copied: u64,
    /// The server an entrance sound is chosen for, by guild id; `None` for
    /// all.
    pub(crate) entrance: Option<String>,
}

impl Kept {
    /// What a page shows of this changed.
    pub(crate) fn changed(&mut self) {
        self.version += 1;
    }
}

/// The theme editor: the theme as edited so far.
pub struct Editor {
    /// The person's own theme edited, by id; `None` makes a new one.
    pub editing: Option<String>,
    /// The built-in theme it began as.
    pub base: &'static str,
    pub palette: Palette,
}

/// The id the layout chose, or the default's for none or an unknown one.
pub fn chosen_id(layout: &Layout, custom: &[Custom]) -> String {
    layout
        .theme()
        .filter(|id| custom::lookup(id, custom).is_some())
        .unwrap_or(builtin::DEFAULT)
        .to_owned()
}

/// The page, built again when what it reads changed.
pub fn show(layout: &Layout, kept: &Kept, page: &mut Appearance) {
    let (note, note_wrong) = kept.note.clone().unwrap_or_default();
    let cosmetics = crate::layout::carry::cosmetics(layout);
    let built = Appearance {
        themes: cards(&kept.custom),
        theme_id: chosen_id(layout, &kept.custom),
        editing: kept.editor.is_some(),
        note,
        note_wrong,
        languages: crate::language::listed()
            .map(|language| language.name.to_owned())
            .collect(),
        language: crate::language::row_of(layout.language()),
        font_files: font_rows(&kept.fonts),
        font_note: font_note(kept),
        font_importing: kept.importing,
        title_bar_choice: cfg!(any(target_os = "linux", windows)),
        system_title_bar: !crate::platform::window::own_bar(layout),
        profile_effects: cosmetics.effects,
        decorations: cosmetics.decorations,
        nameplates: layout.nameplates.unwrap_or(true),
        moving: cosmetics.moving,
        name_styles: cosmetics.name_styles,
        server_tags: cosmetics.server_tags,
        profile_frames: cosmetics.frames,
    };
    if *page != built {
        *page = built;
    }
}

/// Each theme's card: the built-in ones, then the person's own.
fn cards(custom: &[Custom]) -> Vec<ThemeCard> {
    let card = |id: &str, name: &str, custom: bool, palette: &Palette| ThemeCard {
        id: id.to_owned(),
        name: name.to_owned(),
        custom,
        rail: palette.rail,
        sidebar: palette.sidebar,
        chat: palette.chat,
        raised: palette.raised,
        accent: palette.accent,
        text: palette.text,
        muted: palette.muted,
    };
    (builtin::BUILT_IN.iter())
        .map(|theme| card(theme.id, theme.name, false, &theme.palette))
        .chain((custom.iter()).map(|theme| card(&theme.id, &theme.name, true, &theme.palette)))
        .collect()
}

/// The font files' rows: each one's faces or why it gave none, and a
/// warning when Russian's letters come from another font.
fn font_rows(loaded: &Loaded) -> Vec<FontFile> {
    (loaded.entries().iter().enumerate())
        .map(|(index, entry)| {
            let first = (entry.outcome.as_ref().ok()).and_then(|given| given.faces.first());
            FontFile {
                name: entry.stored.name.clone(),
                detail: entry.detail(),
                problem: entry.outcome.is_err(),
                warning: if loaded.lacks_cyrillic(index) {
                    tr!("No Cyrillic: Russian letters are drawn in another font.").to_owned()
                } else {
                    String::new()
                },
                weight: first.map_or(400, |face| face.weight.round().clamp(100.0, 900.0) as u16),
                italic: first.is_some_and(|face| face.italic),
            }
        })
        .collect()
}

/// What Appearance says under the files: which of those chosen last
/// weren't added, and why, or that none of those kept loaded.
fn font_note(kept: &Kept) -> String {
    let loaded = &kept.fonts;
    match kept.refused.as_slice() {
        [] if !loaded.replaces() && !loaded.entries().is_empty() => {
            tr!("None of the font files loaded, so the interface keeps its own font.").to_owned()
        }
        [] => String::new(),
        [(name, problem)] => format!("{name}: {}", problem.text()),
        [(name, problem), rest @ ..] => trn!(
            "{}: {} And {n} more file wasn't added.",
            "{}: {} And {n} more files weren't added.",
            rest.len(),
            name,
            problem.text()
        ),
    }
}
