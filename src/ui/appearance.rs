//! Appearance: choosing a theme, and making and editing the person's own;
//! and the font files the interface is drawn in.

use super::*;
use crate::custom_font;

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
            "" => tr!("My theme").to_owned(),
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
    tr!("{} copy", name)
}

/// Drops the editor's models from the window, so they're held only while
/// it's open.
fn close_theme_editor(window: &YapfastWindow) {
    window.set_theme_editor(false);
    window.set_theme_colours(ModelRc::default());
    window.set_theme_starts(ModelRc::default());
}

/// The font files the interface is drawn in, as Appearance shows them; see
/// [`custom_font`].
#[derive(Default)]
pub(super) struct Fonts {
    loaded: custom_font::Loaded,
    /// The files the last choice brought that weren't added, by name, and why.
    refused: Vec<(String, custom_font::Problem)>,
    /// Files chosen are being copied in.
    importing: bool,
}

/// The extensions the file dialog offers, in both cases: some dialogs match
/// them as written. WOFF2 is offered so that choosing one says it isn't
/// read, rather than hiding the file.
const FONT_EXTENSIONS: &[&str] = &[
    "ttf", "otf", "ttc", "woff", "woff2", "TTF", "OTF", "TTC", "WOFF", "WOFF2",
];

impl Fonts {
    /// The fonts `layout.json` keeps, loaded from the data folder `root`.
    pub(super) fn load(root: &std::path::Path, stored: &[custom_font::Stored]) -> Self {
        Self {
            loaded: custom_font::Loaded::load(
                &root.join(custom_font::FOLDER),
                custom_font::Loaded::default(),
                stored,
            ),
            ..Self::default()
        }
    }

    /// Shows the files in Appearance, and draws the window in their family,
    /// or in the bundled font while there is none. Setting the family it
    /// already has measures nothing again.
    pub(super) fn show(&self, window: &YapfastWindow) {
        let loaded = &self.loaded;
        let rows: Vec<FontFile> = loaded
            .entries()
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let first = entry
                    .outcome
                    .as_ref()
                    .ok()
                    .and_then(|given| given.faces.first());
                FontFile {
                    name: entry.stored.name.as_str().into(),
                    detail: entry.detail().into(),
                    problem: entry.outcome.is_err(),
                    warning: if loaded.lacks_cyrillic(index) {
                        tr!("No Cyrillic: Russian letters are drawn in another font.").into()
                    } else {
                        SharedString::default()
                    },
                    weight: first.map_or(400, |face| face.weight.round() as i32),
                    italic: first.is_some_and(|face| face.italic),
                }
            })
            .collect();
        window.set_font_files(ModelRc::new(VecModel::from(rows)));
        window.set_font_note(self.note().into());
        window.set_font_importing(self.importing);
        window
            .global::<ClientStyle>()
            .set_custom_font(loaded.family().into());
    }

    /// What Appearance says under the files: which of those chosen last
    /// weren't added, and why, or that none of those kept loaded.
    fn note(&self) -> String {
        match self.refused.as_slice() {
            [] if self.loaded.family().is_empty() && !self.loaded.entries().is_empty() => {
                tr!("None of the font files loaded, so the interface keeps its own font.")
                    .to_owned()
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
}

impl Adapter {
    fn fonts_folder(&self) -> std::path::PathBuf {
        self.root.join(custom_font::FOLDER)
    }

    /// Asks for font files, and adds those chosen.
    pub(super) fn choose_fonts(&mut self) {
        let weak = self.weak.clone();
        self.dialog(async move {
            let files = rfd::AsyncFileDialog::new()
                .set_title(tr!("Choose font files"))
                .add_filter(tr!("Fonts"), FONT_EXTENSIONS)
                .pick_files()
                .await;
            let (Some(files), Some(adapter)) = (files, weak.upgrade()) else {
                return;
            };
            let paths = files.iter().map(|file| file.path().to_owned()).collect();
            let window = adapter.borrow().window.upgrade();
            if let Some(window) = window {
                adapter.borrow_mut().import_fonts(&window, paths);
            }
        });
    }

    /// Copies the font files at `paths` into the data folder, off the
    /// window's thread, and then adds them. Choosing is off until they're in.
    pub(super) fn import_fonts(&mut self, window: &YapfastWindow, paths: Vec<std::path::PathBuf>) {
        if self.fonts.importing || paths.is_empty() {
            return;
        }
        let folder = self.fonts_folder();
        let copying = self
            .runtime
            .spawn_blocking(move || custom_font::import(&folder, &paths));
        let weak = self.weak.clone();
        let task = slint::spawn_local(async move {
            let imported = copying.await.unwrap_or_else(|error| {
                tracing::error!(%error, "the font files' copying stopped");
                Vec::new()
            });
            let Some(adapter) = weak.upgrade() else {
                return;
            };
            let window = adapter.borrow().window.upgrade();
            let mut adapter = adapter.borrow_mut();
            adapter.fonts.importing = false;
            if let Some(window) = window {
                adapter.add_fonts(&window, imported);
            }
        });
        match task {
            Ok(_) => {
                self.fonts.importing = true;
                window.set_font_importing(true);
            }
            Err(error) => tracing::error!(%error, "could not add the font files"),
        }
    }

    /// Adds the fonts copied in to those loaded. A file that gave no font is
    /// left out, and the note says why; its copy is deleted.
    pub(super) fn add_fonts(
        &mut self,
        window: &YapfastWindow,
        imported: Vec<custom_font::Imported>,
    ) {
        let mut stored = self.fonts.loaded.stored();
        let mut fresh = Vec::new();
        let mut refused = Vec::new();
        for result in imported {
            match result {
                Ok(chosen) => {
                    fresh.push(chosen.file.clone());
                    if !stored.iter().any(|kept| kept.file == chosen.file) {
                        stored.push(chosen);
                    }
                }
                Err(refusal) => refused.push(refusal),
            }
        }
        let previous = std::mem::take(&mut self.fonts.loaded);
        let mut loaded = custom_font::Loaded::load(&self.fonts_folder(), previous, &stored);
        for entry in loaded.drop_failed(&fresh) {
            if let Err(problem) = entry.outcome {
                refused.push((entry.stored.name, problem));
            }
        }
        self.fonts.loaded = loaded;
        self.fonts.refused = refused;
        self.keep_fonts(window);
    }

    /// The settings closed: which files weren't added has been said. That
    /// none of those kept loaded is said again.
    pub(super) fn forget_font_note(&mut self, window: &YapfastWindow) {
        self.fonts.refused.clear();
        window.set_font_note(self.fonts.note().into());
    }

    /// Takes the font file in this row out of the ones loaded.
    pub(super) fn remove_font(&mut self, window: &YapfastWindow, index: usize) {
        let mut stored = self.fonts.loaded.stored();
        if index < stored.len() {
            stored.remove(index);
            self.set_fonts(window, &stored);
        }
    }

    /// Goes back to the bundled font.
    pub(super) fn reset_font(&mut self, window: &YapfastWindow) {
        self.set_fonts(window, &[]);
    }

    /// Loads these fonts in place of the ones loaded. The window's text is
    /// measured again, as the family it's drawn in is a new one.
    fn set_fonts(&mut self, window: &YapfastWindow, stored: &[custom_font::Stored]) {
        let previous = std::mem::take(&mut self.fonts.loaded);
        self.fonts.loaded = custom_font::Loaded::load(&self.fonts_folder(), previous, stored);
        self.fonts.refused.clear();
        self.keep_fonts(window);
    }

    /// Keeps the fonts loaded chosen, deletes the copies no longer chosen,
    /// and shows them. While files are being copied in, their copies wait
    /// for the next change.
    fn keep_fonts(&mut self, window: &YapfastWindow) {
        self.layout.ui_fonts = self.fonts.loaded.stored();
        self.layout.write(&self.root);
        if !self.fonts.importing {
            custom_font::sweep(&self.fonts_folder(), &self.layout.ui_fonts);
        }
        self.fonts.show(window);
    }
}
