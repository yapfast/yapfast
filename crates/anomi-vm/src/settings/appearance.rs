//! The Appearance settings page: what it shows and what it asks for. Task S06
//! (docs/iced/tasks/settings.md). The Slint side: `AppearanceSettings`
//! (`ui/settings.slint:1371`), set from `src/ui/appearance.rs` and
//! `src/ui/theme.rs:1125` (`cards`), acted on in `src/ui/callbacks.rs:2193`-
//! `:2239`.

use iced::Color;

/// A theme as the page offers it: its name, and the colours of its preview.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeCard {
    pub id: String,
    pub name: String,
    /// One of the person's own, which can be edited.
    pub custom: bool,
    pub rail: Color,
    pub sidebar: Color,
    pub chat: Color,
    pub raised: Color,
    pub accent: Color,
    pub text: Color,
    pub muted: Color,
}

/// A font file the person chose: its name, the faces it gave or why it gave
/// none, a warning on what it lacks, and the weight and slant of its first
/// face, which its sample is drawn in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontFile {
    pub name: String,
    pub detail: String,
    /// It gave no face: its sample is a warning sign.
    pub problem: bool,
    /// Empty for none.
    pub warning: String,
    /// 100 to 900.
    pub weight: u16,
    pub italic: bool,
}

/// The switches under PROFILES, which keep how people's decorations show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    ProfileEffects,
    Decorations,
    Nameplates,
    Moving,
    NameStyles,
    ServerTags,
    ProfileFrames,
}

/// The Appearance settings page: what its screen draws. The window holds it
/// as `vm.settings.appearance`.
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    /// The twelve themes Yapfast comes with, then the person's own.
    pub themes: Vec<ThemeCard>,
    /// The chosen theme's id.
    pub theme_id: String,
    /// A theme is being edited: the page shows the editor (S07) instead.
    pub editing: bool,
    /// How the last import or export of a theme file went, under THEME.
    pub note: String,
    pub note_wrong: bool,
    /// The interface's languages, the default first.
    pub languages: Vec<String>,
    pub language: usize,
    pub font_files: Vec<FontFile>,
    /// Which of the files chosen last weren't added, and why.
    pub font_note: String,
    /// Files chosen are being copied in.
    pub font_importing: bool,
    /// Offered where Yapfast can draw its own title bar.
    pub title_bar_choice: bool,
    pub system_title_bar: bool,
    pub profile_effects: bool,
    pub decorations: bool,
    pub nameplates: bool,
    pub moving: bool,
    pub name_styles: bool,
    pub server_tags: bool,
    pub profile_frames: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            themes: Vec::new(),
            theme_id: String::new(),
            editing: false,
            note: String::new(),
            note_wrong: false,
            languages: Vec::new(),
            language: 0,
            font_files: Vec::new(),
            font_note: String::new(),
            font_importing: false,
            title_bar_choice: false,
            system_title_bar: false,
            profile_effects: true,
            decorations: true,
            nameplates: true,
            moving: true,
            name_styles: true,
            server_tags: true,
            profile_frames: true,
        }
    }
}

/// The Appearance settings page: what a click or a key on it asks for. The
/// app shows the theme at once, keeps the rest in the layout and saves the
/// preferences. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Choose a theme file to import.
    Import,
    /// Open the editor on a new theme.
    NewTheme,
    /// Show a theme at once, by id.
    ChooseTheme(String),
    /// Edit one of the person's own, by id.
    EditTheme(String),
    /// A language was chosen, by its place in the list.
    Language(usize),
    /// Choose font files to add.
    AddFonts,
    RemoveFont(usize),
    /// Back to the bundled font.
    ResetFont,
    /// Use the desktop's title bar, or Yapfast's own.
    SystemTitleBar(bool),
    Look(Look, bool),
}

impl Appearance {
    /// What an intent of the page changes in it, for the app's update and
    /// for the simulator's tests; opening a file picker or the editor is the
    /// app's.
    pub fn apply(&mut self, intent: &Intent) {
        match intent {
            Intent::ChooseTheme(id) => {
                self.theme_id.clone_from(id);
                self.note.clear();
            }
            Intent::Language(index) => {
                self.language = (*index).min(self.languages.len().saturating_sub(1))
            }
            Intent::RemoveFont(index) if *index < self.font_files.len() => {
                self.font_files.remove(*index);
            }
            Intent::ResetFont => {
                self.font_files.clear();
                self.font_note.clear();
            }
            Intent::SystemTitleBar(on) => self.system_title_bar = *on,
            Intent::Look(look, on) => *self.look_mut(*look) = *on,
            _ => {}
        }
    }

    fn look_mut(&mut self, look: Look) -> &mut bool {
        match look {
            Look::ProfileEffects => &mut self.profile_effects,
            Look::Decorations => &mut self.decorations,
            Look::Nameplates => &mut self.nameplates,
            Look::Moving => &mut self.moving,
            Look::NameStyles => &mut self.name_styles,
            Look::ServerTags => &mut self.server_tags,
            Look::ProfileFrames => &mut self.profile_frames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_flips_and_a_font_goes() {
        let mut page = Appearance {
            font_files: vec![FontFile::default(), FontFile::default()],
            ..Appearance::default()
        };
        page.apply(&Intent::Look(Look::Moving, false));
        page.apply(&Intent::RemoveFont(0));
        page.apply(&Intent::RemoveFont(9));
        assert!(!page.moving);
        assert_eq!(page.font_files.len(), 1);
        page.apply(&Intent::ResetFont);
        assert!(page.font_files.is_empty());
    }
}
