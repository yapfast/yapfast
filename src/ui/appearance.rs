//! Appearance: choosing a theme, and making and editing the person's own.

use super::*;

impl Adapter {
    /// Shows the window in a theme, built in or the person's own, and keeps
    /// it chosen. False for an id Yapfast doesn't know, which changes nothing.
    pub(super) fn choose_theme(&mut self, window: &YapfastWindow, id: &str) -> bool {
        let Some((id, chosen)) =
            theme::lookup(id, &self.layout.custom_themes).map(|(id, theme)| (id.to_owned(), theme))
        else {
            return false;
        };
        window.set_theme_id(id.as_str().into());
        self.set_theme(window, &chosen);
        self.layout.theme = Some(id);
        self.layout.write(&self.root);
        true
    }

    /// Shows the window in another theme's colours, building again what
    /// carries the old mention colour in its markup: message rows, the
    /// topic, pins and search results.
    pub(super) fn set_theme(&mut self, window: &YapfastWindow, theme: &theme::Theme) {
        if theme.apply(window) {
            self.restyle_mentions();
            self.refresh(window);
        }
    }

    /// Marks for building again, at the next refresh, what carries the
    /// mention colour in its markup.
    pub(super) fn restyle_mentions(&mut self) {
        self.restyle = true;
        self.topic_shown = None;
        self.models.pins.clear();
        self.models.search_results.clear();
    }

    /// Opens the editor on a copy of the theme shown, which the person can
    /// swap for any other.
    pub(super) fn new_theme(&mut self, window: &YapfastWindow) {
        let current = window.get_theme_id();
        let starts: Vec<_> = theme::starts(&self.layout.custom_themes).collect();
        let index = starts
            .iter()
            .position(|start| start.id == current.as_str())
            .unwrap_or(0);
        let names: Vec<SharedString> = starts.iter().map(|start| start.name.into()).collect();
        let (base, start) = (starts[index].base, starts[index].theme);
        window.set_theme_name(copy_name(starts[index].name).into());
        window.set_theme_starts(ModelRc::new(VecModel::from(names)));
        window.set_theme_start(index as i32);
        self.open_theme_editor(window, theme::Editor::new(None, base, start));
    }

    /// Opens the editor on one of the person's own themes.
    pub(super) fn edit_theme(&mut self, window: &YapfastWindow, id: &str) {
        let Some(custom) = self
            .layout
            .custom_themes
            .iter()
            .find(|custom| custom.id == id)
        else {
            return;
        };
        window.set_theme_name(custom.name.as_str().into());
        let editor = theme::Editor::new(Some(custom.id.clone()), custom.base, custom.theme);
        self.open_theme_editor(window, editor);
    }

    fn open_theme_editor(&mut self, window: &YapfastWindow, editor: theme::Editor) {
        window.set_theme_editing_saved(editor.editing.is_some());
        window.set_theme_dark(editor.theme.dark);
        window.set_theme_colours(editor.rows());
        window.set_theme_can_save(true);
        window.set_theme_editor(true);
        self.set_theme(window, &editor.theme);
        self.theme_editor = Some(editor);
    }

    /// The new theme starts again from another, and takes its name too
    /// unless the person named it.
    pub(super) fn theme_start_chosen(&mut self, window: &YapfastWindow, index: i32) {
        let custom = &self.layout.custom_themes;
        let start = |index: i32| {
            let index = usize::try_from(index).ok()?;
            theme::starts(custom)
                .nth(index)
                .map(|start| (start.base, start.theme, copy_name(start.name)))
        };
        let (Some(editor), Some((base, theme, name))) = (self.theme_editor.as_mut(), start(index))
        else {
            return;
        };
        let named = start(window.get_theme_start())
            .is_none_or(|(_, _, before)| before != window.get_theme_name().as_str());
        if !named {
            window.set_theme_name(name.into());
        }
        editor.restart(base, theme);
        window.set_theme_start(index);
        window.set_theme_dark(theme.dark);
        window.set_theme_can_save(true);
        self.set_theme(window, &theme);
    }

    /// A colour's field changed: the window shows it at once if it's a
    /// colour.
    pub(super) fn theme_colour_edited(&mut self, window: &YapfastWindow, index: i32, text: &str) {
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        let changed = usize::try_from(index).is_ok_and(|index| editor.edit(index, text));
        window.set_theme_can_save(editor.valid());
        if changed {
            let theme = editor.theme;
            self.set_theme(window, &theme);
        }
    }

    pub(super) fn theme_dark_toggled(&mut self, window: &YapfastWindow, dark: bool) {
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        editor.theme.dark = dark;
        let theme = editor.theme;
        self.set_theme(window, &theme);
    }

    /// Keeps the theme as edited, among the person's own, and chooses it.
    pub(super) fn save_theme(&mut self, window: &YapfastWindow) {
        let Some(editor) = self.theme_editor.take_if(|editor| editor.valid()) else {
            return;
        };
        let name = match window.get_theme_name().trim() {
            "" => "My theme".to_owned(),
            name => name.to_owned(),
        };
        let custom = &mut self.layout.custom_themes;
        let saved = editor
            .editing
            .as_ref()
            .and_then(|id| custom.iter_mut().find(|custom| custom.id == *id));
        let id = match saved {
            Some(saved) => {
                saved.name = name;
                saved.theme = editor.theme;
                saved.id.clone()
            }
            None => {
                let id = theme::new_id(custom);
                custom.push(theme::Custom {
                    id: id.clone(),
                    name,
                    base: editor.base,
                    theme: editor.theme,
                });
                id
            }
        };
        close_theme_editor(window);
        window.set_themes(theme::cards(&self.layout.custom_themes));
        self.choose_theme(window, &id);
    }

    /// Closes the editor and shows the theme from before it opened.
    pub(super) fn cancel_theme(&mut self, window: &YapfastWindow) {
        if self.theme_editor.take().is_some() {
            close_theme_editor(window);
            let (_, before) =
                theme::chosen(self.layout.theme.as_deref(), &self.layout.custom_themes);
            self.set_theme(window, &before);
        }
    }

    /// Deletes the theme being edited. If it was the chosen one, Yap shows.
    pub(super) fn delete_theme(&mut self, window: &YapfastWindow) {
        let Some(editor) = self.theme_editor.take() else {
            return;
        };
        close_theme_editor(window);
        if let Some(id) = editor.editing {
            self.layout.custom_themes.retain(|custom| custom.id != id);
        }
        window.set_themes(theme::cards(&self.layout.custom_themes));
        let (id, _) = theme::chosen(self.layout.theme.as_deref(), &self.layout.custom_themes);
        let id = id.to_owned();
        self.choose_theme(window, &id);
    }
}

/// What a new theme copied from another is called until the person names it.
fn copy_name(name: &str) -> String {
    format!("{name} copy")
}

/// Drops the editor's models from the window, so they're held only while
/// it's open.
fn close_theme_editor(window: &YapfastWindow) {
    window.set_theme_editor(false);
    window.set_theme_colours(ModelRc::default());
    window.set_theme_starts(ModelRc::default());
}

/// What Appearance says under the font files: that some saved file didn't
/// load, and that Yapfast then uses its own font when none did.
pub(super) fn font_note(loaded: &crate::custom_font::Loaded) -> String {
    let failed = loaded
        .entries()
        .iter()
        .filter(|entry| entry.outcome.is_err())
        .count();
    match (failed, loaded.family().is_empty()) {
        (0, _) => String::new(),
        (1, true) => "The font file didn't load, so Yapfast uses its own font.".into(),
        (_, true) => "None of the font files loaded, so Yapfast uses its own font.".into(),
        (1, false) => "One font file didn't load. The others are in use.".into(),
        (_, false) => format!("{failed} font files didn't load. The others are in use."),
    }
}

/// Shows the font files in Appearance, and draws the window in their family,
/// or in the bundled font while there are none.
pub(super) fn show_font(window: &YapfastWindow, loaded: &crate::custom_font::Loaded, note: &str) {
    let files: Vec<FontFile> = loaded
        .entries()
        .iter()
        .map(|entry| FontFile {
            name: entry.name().into(),
            detail: entry.detail().into(),
            problem: entry.outcome.is_err(),
        })
        .collect();
    window.set_font_files(ModelRc::new(VecModel::from(files)));
    window.set_font_note(note.into());
    window
        .global::<ClientStyle>()
        .set_custom_font(loaded.family().into());
}

impl Adapter {
    /// Loads these font files in place of the ones loaded, and keeps them
    /// chosen. The text of the window is measured again, because the family
    /// it is drawn in is a new one: see [`crate::custom_font`].
    fn set_font_files(&mut self, window: &YapfastWindow, paths: &[String]) {
        let previous = std::mem::take(&mut self.ui_font);
        self.ui_font = crate::custom_font::Loaded::load(previous, paths);
        self.layout.ui_font_files = self.ui_font.paths();
        self.layout.write(&self.root);
        self.font_note = font_note(&self.ui_font);
        show_font(window, &self.ui_font, &self.font_note);
    }

    /// Adds the font files chosen to the ones loaded. A file that isn't a
    /// usable font is left out, and the note says why.
    pub(super) fn add_fonts(&mut self, window: &YapfastWindow, chosen: &[String]) {
        let mut paths = self.ui_font.paths();
        paths.extend(chosen.iter().cloned());
        self.set_font_files(window, &paths);
        let failed = self.ui_font.drop_failed(chosen);
        if failed.is_empty() {
            return;
        }
        self.layout.ui_font_files = self.ui_font.paths();
        self.layout.write(&self.root);
        let named = |entry: &crate::custom_font::Entry| match &entry.outcome {
            Err(problem) => format!("{}: {problem}", entry.name()),
            Ok(_) => entry.name().to_owned(),
        };
        self.font_note = match failed.as_slice() {
            [one] => named(one),
            [first, rest @ ..] => {
                format!("{} And {} more weren't loaded.", named(first), rest.len())
            }
            [] => String::new(),
        };
        show_font(window, &self.ui_font, &self.font_note);
    }

    /// Takes the font file in this row out of the ones loaded.
    pub(super) fn remove_font(&mut self, window: &YapfastWindow, index: usize) {
        let mut paths = self.ui_font.paths();
        if index < paths.len() {
            paths.remove(index);
            self.set_font_files(window, &paths);
        }
    }

    /// Goes back to the bundled font.
    pub(super) fn reset_font(&mut self, window: &YapfastWindow) {
        self.set_font_files(window, &[]);
    }
}
