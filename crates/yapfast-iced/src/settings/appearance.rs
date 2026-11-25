//! Appearance's theme shortcuts and profile switches, and the dedicated
//! Themes page: installed packages, the person's own themes, their options
//! and performance. The editor's changes show at once; Save keeps them and
//! Cancel puts the theme from before back. Import beside New theme and Export
//! in the editor share a theme as a file. See docs/themes.md.

use std::path::{Path, PathBuf};

use iced::{
    Alignment, Border, Color, Element, Length, Task,
    widget::{Id, Space, button, column, container, operation, row, stack, text},
};
use yapfast_core::{
    application::Action,
    i18n::tr,
    i18n::{self, Language},
    theme::{self as palette, Custom},
    themes::Performance,
};

use super::{
    Drop, Event as SettingsEvent, Page, Section, Settings, pages::Pref, theme_marketplace,
};
use crate::{
    app::Message,
    controls::{self, Kind, action, lights, placed},
    ctx::{Cx, Pref as Shown, Request},
    fonts::{BOLD, MEDIUM},
    form::{self, switch},
    pictures::Icon,
    themes::color,
    widgets::{ColourPad, Hsv, hsv_rgb, rgb_hsv},
};

/// A theme's tile: at least this wide, this tall, this far apart.
const TILE: f32 = 150.0;
const TILE_HEIGHT: f32 = 124.0;
const TILE_GAP: f32 = 12.0;
/// The open colour picker's code field.
pub(crate) const COLOUR_CODE: &str = "theme-colour-code";

/// The theme editor while it's open: the theme as edited, each colour's
/// text and whether it's wrong, its name, and what it started from.
pub struct Editor {
    /// The custom theme edited, by id; none for a new one.
    pub editing: Option<String>,
    pub base: &'static str,
    pub theme: palette::Theme,
    /// The parts it colours on its own. One not set follows its colour, so
    /// a new chat or text colour moves it too until it's given its own.
    pub parts: palette::Parts,
    pub name: String,
    /// Each colour's field, in `Theme::NAMES`' order and then the parts' in
    /// `Parts::TOKENS`': its name as `layout.json` has it, its text and
    /// whether it's a colour.
    pub fields: Vec<(String, String, bool)>,
    /// The theme a new one started as, among `palette::starts`.
    pub start: usize,
    /// Delete was pressed once; the next press deletes.
    pub confirming: bool,
    /// Which colour's picker is open (by index in `fields`), and its HSV
    /// while open so hue isn't lost on grey or black.
    pub picker: Option<usize>,
    pub hsv: Hsv,
}

impl Editor {
    fn new(
        editing: Option<String>,
        base: &'static str,
        theme: palette::Theme,
        parts: palette::Parts,
        name: String,
    ) -> Self {
        let mut editor = Self {
            editing,
            base,
            theme,
            parts,
            name,
            fields: Vec::new(),
            start: 0,
            confirming: false,
            picker: None,
            hsv: [0.0; 3],
        };
        editor.fill();
        editor
    }

    /// Every field's text from the theme's colours.
    fn fill(&mut self) {
        self.fields = palette::Theme::NAMES
            .iter()
            .map(|name| palette::token(name))
            .chain(palette::Parts::TOKENS.iter().map(|&part| part.to_owned()))
            .enumerate()
            .filter_map(|(index, token)| Some((token, palette::hex(self.colour(index)?), false)))
            .collect();
    }

    /// Field `index`'s colour as the window shows it: a part not set shows
    /// the colour it follows.
    fn colour(&self, index: usize) -> Option<palette::Colour> {
        match index.checked_sub(palette::Theme::NAMES.len()) {
            None => self.theme.colour(index),
            Some(part) => self.parts.shown(part, &self.theme),
        }
    }

    /// Gives field `index` `colour`; a part takes it as its own. Returns
    /// whether the window's colours changed.
    fn set(&mut self, index: usize, colour: palette::Colour) -> bool {
        if self.colour(index) == Some(colour) {
            return false;
        }
        match index.checked_sub(palette::Theme::NAMES.len()) {
            None => {
                let Some(slot) = self.theme.colour_mut(index) else {
                    return false;
                };
                *slot = colour;
                self.follow();
            }
            Some(part) => {
                let Some(slot) = self.parts.own_mut(part) else {
                    return false;
                };
                *slot = Some(colour);
            }
        }
        true
    }

    /// The fields of the parts not set show the colours they follow.
    fn follow(&mut self) {
        let first = palette::Theme::NAMES.len();
        for part in 0..palette::Parts::TOKENS.len() {
            if self.parts.own(part) == Some(None)
                && let Some(colour) = self.parts.shown(part, &self.theme)
                && let Some(field) = self.fields.get_mut(first + part)
            {
                (field.1, field.2) = (palette::hex(colour), false);
            }
        }
    }

    /// Takes a colour's text as typed: the theme takes it if it's a colour.
    /// Returns whether the theme changed.
    fn edit(&mut self, index: usize, text: String) -> bool {
        let parsed = palette::parse(&text);
        let Some(field) = self.fields.get_mut(index) else {
            return false;
        };
        (field.1, field.2) = (text, parsed.is_none());
        if self.picker == Some(index)
            && let Some(colour) = parsed
        {
            let rgb = colour.rgb_part();
            if rgb != hsv_rgb(self.hsv) {
                self.hsv = rgb_hsv(rgb);
            }
        }
        parsed.is_some_and(|colour| self.set(index, colour))
    }

    /// Takes a colour chosen on the picker's pad, preserving the token's
    /// existing alpha channel (e.g. `#rrggbbaa` tokens like `accent-soft`).
    /// Returns whether the theme changed.
    fn pick_hsv(&mut self, index: usize, hsv: Hsv) -> bool {
        let Some(alpha) = self.colour(index).map(palette::Colour::alpha) else {
            return false;
        };
        let colour = palette::Colour::from_argb((u32::from(alpha) << 24) | hsv_rgb(hsv));
        let changed = self.set(index, colour);
        if let Some(field) = self.fields.get_mut(index) {
            (field.1, field.2) = (palette::hex(colour), false);
        }
        changed
    }

    fn valid(&self) -> bool {
        self.fields.iter().all(|(_, _, wrong)| !wrong)
    }

    /// The name in the field, or "My theme" if it holds none.
    fn kept_name(&self) -> String {
        match self.name.trim() {
            "" => tr("My theme").to_owned(),
            name => name.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Choose(String),
    /// A tab of the page: leaving the creator or the editor cancels it.
    Tab(theme_marketplace::Tab),
    TurnOff,
    Revert,
    Performance(i32),
    ReduceMotion(bool),
    ReadableText(bool),
    ThemeOption(String, String, serde_json::Value),
    ThemeContent(String, bool),
    ThemeNotificationSounds(String, bool),
    ThemeInterfaceSounds(String, bool),
    ThemeInterfaceVolume(String, u8),
    ThemeTransitions(bool),
    Creator(super::theme_creator::Event),
    New,
    Edit(String),
    Start(usize),
    Colour(usize, String),
    /// Opens or closes a colour's picker (`Some(index)` toggles it, `None` closes).
    Picker(Option<usize>),
    /// A colour chosen on the open picker's pad.
    Hsv(Hsv),
    Dark(bool),
    Name(String),
    Save,
    Cancel,
    Delete,
    /// Opens the system's file dialog for a theme file to import.
    Import,
    /// The file it gave, by its path.
    Imported(PathBuf),
    /// Opens the system's file dialog to save the theme in the editor.
    Export,
    /// The name the file was saved under, or why it couldn't be.
    Exported(String, Result<(), String>),
    /// Puts the theme in the editor on the clipboard as a theme file's text.
    Copy,
    /// Reads the clipboard for a theme file's text to import.
    Paste,
    /// The clipboard's text, if it held any.
    Pasted(Option<String>),
    Profile(Cosmetic, bool),
}

/// Appearance's profile switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cosmetic {
    Effects,
    Decorations,
    Nameplates,
    Moving,
    NameStyles,
    ServerTags,
    Frames,
}

pub(super) fn theme_message(event: Event) -> Message {
    crate::app::settings(SettingsEvent::Theme(event))
}

pub(super) fn marketplace_message(event: theme_marketplace::Event) -> Message {
    crate::app::settings(SettingsEvent::Marketplace(event))
}

impl Settings {
    pub(super) fn theme_event(&mut self, event: Event, cx: &mut Cx<'_>) -> iced::Task<Message> {
        match event {
            Event::Creator(event) => return self.creator_event(event, cx),
            Event::Choose(id) => {
                self.theme_note = None;
                choose(cx, &id);
            }
            Event::Tab(tab) => {
                self.cancel_theme(cx);
                return self.update(
                    SettingsEvent::Marketplace(theme_marketplace::Event::Tab(tab)),
                    cx,
                );
            }
            Event::TurnOff => {
                // Off stays off: no theme is kept in layout.json, so the
                // default shows at the next start too, and the theme that
                // was showing is the one Back returns to.
                cx.themes.turn_off(cx.client.root());
                // A trial ends with it: what it was given goes.
                cx.layout.theme_sounds.remove("trial");
                cx.layout.theme_content.remove("trial");
                cx.layout.theme = None;
                cx.layout.previous_theme = cx.themes.previous().map(|(id, _)| id.to_owned());
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Revert => {
                if cx.themes.revert(cx.client.root()) {
                    cx.request(Request::Changed(Shown::Theme));
                }
            }
            Event::Performance(index) => {
                let performance = Performance::from_index(index);
                cx.layout.theme_performance = performance;
                cx.keep("theme_performance", performance);
                cx.themes
                    .motion_changed(performance, cx.layout.reduce_motion);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::ReduceMotion(on) => {
                cx.layout.reduce_motion = on;
                cx.keep("reduce_motion", on);
                cx.themes.motion_changed(cx.layout.theme_performance, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::ReadableText(on) => {
                cx.layout.readable_text = on;
                cx.keep("readable_text", on);
                cx.themes.readable_changed(on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::ThemeOption(id, option, value) => {
                let Some(choices) = cx.themes.set_option(
                    &id,
                    &option,
                    value,
                    cx.layout.theme_performance,
                    cx.layout.reduce_motion,
                ) else {
                    return iced::Task::none();
                };
                if choices.is_empty() {
                    cx.layout.theme_options.remove(&id);
                } else {
                    cx.layout.theme_options.insert(id, choices);
                }
                cx.keep("theme_options", &cx.layout.theme_options);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::ThemeContent(id, on) => {
                if on {
                    cx.layout.theme_content.insert(id);
                } else {
                    cx.layout.theme_content.remove(&id);
                }
                cx.keep("theme_content", &cx.layout.theme_content);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::ThemeNotificationSounds(id, on) => {
                change_sound_prefs(cx, &id, |prefs| prefs.on = on);
            }
            Event::ThemeInterfaceSounds(id, on) => {
                change_sound_prefs(cx, &id, |prefs| prefs.interface = on);
            }
            Event::ThemeInterfaceVolume(id, volume) => {
                change_sound_prefs(cx, &id, |prefs| prefs.volume = volume.min(100));
            }
            Event::ThemeTransitions(on) => {
                cx.layout.theme_transitions = on;
                cx.keep("theme_transitions", on);
            }
            Event::New => {
                self.theme_note = None;
                self.new_theme(cx);
            }
            Event::Edit(id) => {
                if self.edit_creator(&id, cx) {
                    return iced::Task::none();
                }
                let Some(custom) = cx.themes.custom.iter().find(|custom| custom.id == id) else {
                    return iced::Task::none();
                };
                let editor = Editor::new(
                    Some(custom.id.clone()),
                    custom.base,
                    custom.theme,
                    custom.parts,
                    custom.name.clone(),
                );
                self.open_editor(editor, cx);
            }
            Event::Start(index) => {
                let starts: Vec<_> = palette::starts(&cx.themes.custom)
                    .map(|start| (start.base, start.theme, start.parts, copy_name(start.name)))
                    .collect();
                let Some(editor) = self.editor.as_mut() else {
                    return iced::Task::none();
                };
                let Some((base, theme, parts, name)) = starts.get(index).cloned() else {
                    return iced::Task::none();
                };
                // It takes the new start's name too, unless the person
                // named it.
                if starts
                    .get(editor.start)
                    .is_some_and(|before| before.3 == editor.name)
                {
                    editor.name = name;
                }
                editor.base = base;
                editor.theme = theme;
                editor.parts = parts;
                editor.start = index;
                editor.picker = None;
                editor.fill();
                self.open = None;
                self.show_edited(cx);
            }
            Event::Colour(index, text) => {
                let changed = self
                    .editor
                    .as_mut()
                    .is_some_and(|editor| editor.edit(index, text));
                if changed {
                    self.show_edited(cx);
                }
            }
            Event::Picker(which) => {
                let Some(editor) = self.editor.as_mut() else {
                    return Task::none();
                };
                self.open = None;
                match which {
                    Some(index) if editor.picker != Some(index) => {
                        if let Some(colour) = editor.colour(index) {
                            editor.picker = Some(index);
                            editor.hsv = rgb_hsv(colour.rgb_part());
                            return operation::focus(Id::new(COLOUR_CODE));
                        }
                    }
                    _ => editor.picker = None,
                }
            }
            Event::Hsv(hsv) => {
                let Some(editor) = self.editor.as_mut() else {
                    return Task::none();
                };
                let Some(index) = editor.picker else {
                    return Task::none();
                };
                editor.hsv = hsv;
                if editor.pick_hsv(index, hsv) {
                    self.show_edited(cx);
                }
            }
            Event::Dark(dark) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.theme.dark = dark;
                }
                self.show_edited(cx);
            }
            Event::Name(name) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.name = name;
                }
            }
            Event::Save => self.save_theme(cx),
            Event::Cancel => self.cancel_theme(cx),
            Event::Delete => self.delete_theme(cx),
            Event::Import => return import_dialog(),
            Event::Imported(path) => self.import_theme(&path, cx),
            Event::Export => return self.export_dialog(),
            Event::Exported(name, result) => self.exported(&name, result),
            Event::Copy => return self.copy_theme(),
            Event::Paste => {
                return iced::clipboard::read().map(|text| theme_message(Event::Pasted(text)));
            }
            Event::Pasted(text) => self.paste_theme(text.as_deref(), cx),
            Event::Profile(cosmetic, on) => self::cosmetic(cx, cosmetic, on),
        }
        iced::Task::none()
    }

    /// Opens the editor on a copy of the theme shown, which can start from
    /// any other.
    fn new_theme(&mut self, cx: &mut Cx<'_>) {
        let current = &cx.theme.id;
        let starts: Vec<_> = palette::starts(&cx.themes.custom)
            .map(|start| {
                (
                    start.id.to_owned(),
                    start.base,
                    start.theme,
                    start.parts,
                    copy_name(start.name),
                )
            })
            .collect();
        let start = starts
            .iter()
            .position(|(id, ..)| id == current)
            .unwrap_or(0);
        let (_, base, theme, parts, name) = starts[start].clone();
        let mut editor = Editor::new(None, base, theme, parts, name);
        editor.start = start;
        self.open_editor(editor, cx);
    }

    fn open_editor(&mut self, editor: Editor, cx: &mut Cx<'_>) {
        // As in the Slint build, New theme and Edit in Appearance lead to the
        // creator inside Settings → Themes rather than turning Appearance
        // into a second theme-management page.
        self.section = Some(Section::Themes);
        self.editor = Some(editor);
        self.show_edited(cx);
    }

    /// The window in the theme as edited, without keeping it.
    fn show_edited(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let id = editor.editing.as_deref().unwrap_or("editing");
        self.preview = Some(crate::themes::custom_theme(id, &editor.theme, editor.parts));
        cx.request(Request::Changed(Shown::Theme));
    }

    /// Closes the editor: the window shows the theme chosen again.
    pub(super) fn close_editor(&mut self, cx: &mut Cx<'_>) -> Option<Editor> {
        let editor = self.editor.take()?;
        if self.preview.take().is_some() {
            cx.request(Request::Changed(Shown::Theme));
        }
        Some(editor)
    }

    /// Keeps the theme as edited among the person's own, and chooses it.
    fn save_theme(&mut self, cx: &mut Cx<'_>) {
        if !self.editor.as_ref().is_some_and(Editor::valid) {
            return;
        }
        let Some(editor) = self.close_editor(cx) else {
            return;
        };
        self.theme_note = None;
        let name = editor.kept_name();
        let custom = &mut cx.themes.custom;
        let saved = editor
            .editing
            .as_ref()
            .and_then(|id| custom.iter_mut().find(|custom| custom.id == *id));
        let id = match saved {
            Some(saved) => {
                saved.name = name;
                saved.theme = editor.theme;
                saved.parts = editor.parts;
                saved.id.clone()
            }
            None => {
                let id = palette::new_id(custom);
                custom.push(Custom {
                    id: id.clone(),
                    name,
                    base: editor.base,
                    theme: editor.theme,
                    parts: editor.parts,
                    creator: None,
                    folder: None,
                });
                id
            }
        };
        cx.keep("custom_themes", &cx.themes.custom);
        choose(cx, &id);
    }

    /// The theme in the editor as a file to share, and the name to suggest
    /// for it: nothing while a colour's field holds no colour, or the
    /// editor is closed.
    pub(crate) fn theme_file(&self) -> Option<(String, String)> {
        let editor = self.editor.as_ref().filter(|editor| editor.valid())?;
        let name = editor.kept_name();
        let text = palette::export(&name, editor.base, &editor.theme, &editor.parts);
        Some((palette::file_name(&name), text))
    }

    /// Asks where to save the theme in the editor.
    fn export_dialog(&self) -> iced::Task<Message> {
        let Some((file_name, text)) = self.theme_file() else {
            return iced::Task::none();
        };
        iced::Task::perform(
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Export theme")
                    .set_file_name(file_name)
                    .add_filter("Yapfast theme", &["json"])
                    .save_file()
                    .await?;
                let result = std::fs::write(file.path(), text).map_err(|error| error.to_string());
                Some((file.file_name(), result))
            },
            |saved| match saved {
                Some((name, result)) => theme_message(Event::Exported(name, result)),
                None => Message::Nothing,
            },
        )
    }

    /// Says how writing the theme file went.
    fn exported(&mut self, name: &str, result: Result<(), String>) {
        self.theme_note = Some(match result {
            Ok(()) => (format!("Exported to {name}."), false),
            Err(error) => {
                tracing::warn!(%error, "could not write a theme file");
                (format!("Couldn't save the theme file: {error}"), true)
            }
        });
    }

    /// Puts the theme in the editor on the clipboard, as the text Export
    /// would write to a file.
    fn copy_theme(&mut self) -> iced::Task<Message> {
        let Some((_, text)) = self.theme_file() else {
            return iced::Task::none();
        };
        self.theme_note = Some(("Copied the theme to the clipboard.".to_owned(), false));
        iced::clipboard::write(text)
    }

    /// Keeps the theme in the clipboard's text among the person's own, as
    /// Import does a file's.
    fn paste_theme(&mut self, text: Option<&str>, cx: &mut Cx<'_>) {
        if self.editing_theme() {
            return;
        }
        match text {
            Some(text) if !text.trim().is_empty() => self.keep_theme_file(text.as_bytes(), cx),
            _ => self.theme_note = Some(("The clipboard holds no text.".to_owned(), true)),
        }
    }

    /// Whether a theme is being edited, saying so: nothing is imported
    /// meanwhile, as the colours the window shows are the edit's.
    fn editing_theme(&mut self) -> bool {
        let editing = self.editor.is_some() || self.creator.is_some();
        if editing {
            self.theme_note = Some((
                "Save or cancel the theme being edited first.".to_owned(),
                true,
            ));
        }
        editing
    }

    /// Keeps the theme in a file among the person's own, and chooses it, or
    /// says why it can't. Nothing changes while the editor is open: the
    /// colours the window shows are the edit's.
    fn import_theme(&mut self, path: &Path, cx: &mut Cx<'_>) {
        use std::io::Read;
        if self.editing_theme() {
            return;
        }
        let mut bytes = Vec::new();
        let read = std::fs::File::open(path)
            .and_then(|file| file.take(palette::FILE_LIMIT + 1).read_to_end(&mut bytes));
        if let Err(error) = read {
            tracing::warn!(%error, "could not read a theme file");
            self.theme_note = Some((format!("Couldn't read the file: {error}"), true));
            return;
        }
        self.keep_theme_file(&bytes, cx);
    }

    /// Keeps the theme in a theme file's bytes among the person's own, and
    /// chooses it, or says why it can't.
    fn keep_theme_file(&mut self, bytes: &[u8], cx: &mut Cx<'_>) {
        let imported = match palette::import(bytes) {
            Ok(imported) => imported,
            Err(error) => {
                self.theme_note = Some((error.to_string(), true));
                return;
            }
        };
        let custom = &mut cx.themes.custom;
        let id = palette::new_id(custom);
        custom.push(Custom {
            id: id.clone(),
            name: imported.name.clone(),
            base: imported.base,
            theme: imported.theme,
            parts: imported.parts,
            creator: None,
            folder: None,
        });
        cx.keep("custom_themes", &cx.themes.custom);
        choose(cx, &id);
        let skipped = match imported.skipped {
            0 => String::new(),
            1 => " 1 colour in the file isn't a colour, so it uses the base theme's.".to_owned(),
            skipped => format!(
                " {skipped} colours in the file aren't colours, so it uses the base theme's."
            ),
        };
        let note = format!("Imported {}.{skipped}", imported.name);
        self.theme_note = Some((note, false));
    }

    /// Closes the editor and shows the theme from before it opened.
    pub(super) fn cancel_theme(&mut self, cx: &mut Cx<'_>) {
        if self.close_editor(cx).is_some() {
            self.theme_note = None;
        }
        self.close_creator(cx);
    }

    /// Deletes the theme edited; if it was the chosen one, Yap shows.
    fn delete_theme(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if !editor.confirming {
            editor.confirming = true;
            return;
        }
        let Some(editor) = self.close_editor(cx) else {
            return;
        };
        self.theme_note = None;
        if let Some(id) = editor.editing {
            let changed = cx.themes.forget_custom(cx.client.root(), &id);
            cx.layout.custom_themes.clone_from(&cx.themes.custom);
            cx.keep("custom_themes", &cx.themes.custom);
            if changed {
                cx.request(Request::Changed(Shown::Theme));
            }
        }
    }
}

/// The system's file dialog for a theme file: `Event::Imported` with it, or
/// nothing if it's closed.
fn import_dialog() -> iced::Task<Message> {
    iced::Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Import theme")
                .add_filter("Yapfast theme", &["json"])
                .pick_file()
                .await
                .map(|file| file.path().to_owned())
        },
        |path| match path {
            Some(path) => theme_message(Event::Imported(path)),
            None => Message::Nothing,
        },
    )
}

/// Chooses the theme `id` and keeps the choice, if there's such a theme:
/// the window shows it.
fn choose(cx: &mut Cx<'_>, id: &str) {
    let Some(theme) = cx.themes.find(id) else {
        return;
    };
    cx.themes.choose(cx.client.root(), &theme.id);
    cx.request(Request::ThemeApplied);
}

/// Changes one applied package's sound choices, omitting the map entry when
/// every value is the Slint build's default. Notification replacements are
/// reapplied immediately when this is the active theme.
fn change_sound_prefs(
    cx: &mut Cx<'_>,
    id: &str,
    change: impl FnOnce(&mut crate::themes::SoundPrefs),
) {
    let Some(mut prefs) = cx.themes.sound_prefs(id) else {
        return;
    };
    change(&mut prefs);
    if !cx.themes.set_sound_prefs(id, prefs) {
        return;
    }
    if prefs == crate::themes::SoundPrefs::default() {
        cx.layout.theme_sounds.remove(id);
    } else {
        cx.layout.theme_sounds.insert(id.to_owned(), prefs);
    }
    cx.keep("theme_sounds", &cx.layout.theme_sounds);
    let active = cx.themes.chosen_id().to_owned();
    if active == id {
        cx.themes.apply_sounds(&active);
    }
}

/// A profile switch: kept, and the core told what may now show.
fn cosmetic(cx: &mut Cx<'_>, cosmetic: Cosmetic, on: bool) {
    let prefs = &mut cx.layout.prefs;
    let (slot, key) = match cosmetic {
        Cosmetic::Effects => (&mut prefs.profile_effects, "profile_effects"),
        Cosmetic::Decorations => (&mut prefs.decorations, "decorations"),
        Cosmetic::Nameplates => (&mut prefs.nameplates, "nameplates"),
        Cosmetic::Moving => (&mut prefs.moving_cosmetics, "moving_cosmetics"),
        Cosmetic::NameStyles => (&mut prefs.name_styles, "name_styles"),
        Cosmetic::ServerTags => (&mut prefs.server_tags, "server_tags"),
        Cosmetic::Frames => (&mut prefs.profile_frames, "profile_frames"),
    };
    *slot = Some(on);
    cx.keep(key, on);
    if cosmetic == Cosmetic::Nameplates {
        cx.dispatch(Action::HideNameplates(!on));
    } else {
        let cosmetics = cx.layout.prefs.cosmetics();
        cx.dispatch(Action::SetCosmetics(cosmetics));
    }
    cx.request(Request::Changed(Shown::Roster));
}

impl<'a> Page<'a> {
    pub(super) fn appearance_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tiles = self.theme_tiles(true);
        let prefs = &self.cx.layout.prefs;
        let cosmetics = prefs.cosmetics();
        let profile = |label, description, on: bool, which| {
            switch(label, on, Some(theme_message(Event::Profile(which, !on))))
                .description(description)
                .view(self.cx.look)
        };
        let theme_options = self
            .cx
            .themes
            .installed
            .iter()
            .find(|installed| self.cx.themes.chosen_id() == installed.id)
            .and_then(|installed| self.theme_options(installed))
            .unwrap_or_else(|| Space::new().height(0).into());
        let installed_name = self
            .cx
            .themes
            .installed
            .iter()
            .find(|installed| self.cx.themes.chosen_id() == installed.id)
            .map(|installed| installed.name.clone());
        let actions = row![
            action(
                theme,
                tr("Browse themes"),
                Kind::Plain,
                Some(crate::app::settings(SettingsEvent::Open(Section::Themes))),
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Brush, 16, crate::theme::rgb(theme.text))
            ),
            action(
                theme,
                tr("Import"),
                Kind::Plain,
                Some(theme_message(Event::Import))
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Folder, 16, crate::theme::rgb(theme.text))
            ),
            action(
                theme,
                tr("Paste"),
                Kind::Plain,
                Some(theme_message(Event::Paste))
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Paste, 16, crate::theme::rgb(theme.text))
            ),
            action(
                theme,
                tr("New theme"),
                Kind::Plain,
                Some(theme_message(Event::New))
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Plus, 16, crate::theme::rgb(theme.text))
            ),
        ]
        .spacing(8);
        let installed_note: Element<'a, Message> = installed_name.map_or_else(
            || Space::new().height(0).into(),
            |name| {
                form::note(
                    theme,
                    i18n::fill(
                        tr("Showing {}, an installed theme. Choose one above, or manage it in Themes."),
                        &[&name],
                    ),
                )
            },
        );
        let mut head = vec![form::title(theme, tr("Appearance"))];
        head.extend(self.theme_note());
        head.extend([
            form::group([
                form::heading(theme, tr("LANGUAGE")),
                form::dropdown(
                    Language::offered()
                        .into_iter()
                        .map(|language| language.name().to_owned())
                        .collect(),
                    Some(prefs.language_place()),
                    self.s.open == Some(Drop::Language),
                    crate::app::settings(SettingsEvent::Drop(Drop::Language)),
                    |place| crate::app::settings(SettingsEvent::Pref(Pref::Language(place))),
                )
                .width(240.0)
                .view(self.cx.look),
            ]),
            form::group([
                form::beside(form::heading(theme, tr("THEME")), actions),
                tiles,
            ]),
            self.font_section(),
            installed_note,
            theme_options,
            switch(
                tr("Readable text"),
                self.cx.layout.readable_text,
                Some(theme_message(Event::ReadableText(
                    !self.cx.layout.readable_text,
                ))),
            )
            .description(tr(
                "Messages, names, channels and what you type use Yapfast's own font in every theme. The theme's art, colours and layout stay.",
            ))
            .view(self.cx.look),
            self.desktop_icons(),
            self.title_bar_switch(),
            column![
                form::heading(theme, tr("PROFILES")),
                profile(
                    tr("Play profile effects"),
                    tr("The animations some people put over their profile cards."),
                    cosmetics.effects,
                    Cosmetic::Effects,
                ),
                profile(
                    tr("Show avatar decorations"),
                    tr("The pictures around people's avatars."),
                    cosmetics.decorations,
                    Cosmetic::Decorations,
                ),
                profile(
                    tr("Show nameplates"),
                    tr("The pictures Discord members equip behind their rows in the member list and on your account card."),
                    prefs.nameplates.unwrap_or(true),
                    Cosmetic::Nameplates,
                ),
                profile(
                    tr("Animate decorations and nameplates"),
                    tr("They play when pointed at, as in Discord. Off, they hold still."),
                    cosmetics.moving,
                    Cosmetic::Moving,
                ),
                profile(
                    tr("Show name styles"),
                    tr("The fonts and colours people give their display names."),
                    cosmetics.name_styles,
                    Cosmetic::NameStyles,
                ),
                profile(
                    tr("Show server tags"),
                    tr("The tag and its badge beside people's names."),
                    cosmetics.server_tags,
                    Cosmetic::ServerTags,
                ),
                profile(
                    tr("Show profile frames"),
                    tr("The frames some people put around their profile cards."),
                    cosmetics.frames,
                    Cosmetic::Frames,
                ),
            ]
            .spacing(12)
            .into(),
        ]);
        form::page(head)
    }

    /// How the last theme import or export went, under the title, in the
    /// danger's ink if it failed.
    pub(super) fn theme_note(self) -> Option<Element<'a, Message>> {
        let (note, wrong) = self.s.theme_note.as_ref()?;
        let theme = self.cx.look.theme;
        Some(form::note_in(
            note.as_str(),
            if *wrong {
                theme.danger_text
            } else {
                theme.muted
            },
        ))
    }

    /// The themes as tiles, as many 150 px or wider across as fit.
    pub(super) fn theme_tiles(self, include_built_in: bool) -> Element<'a, Message> {
        iced::widget::responsive(move |size| {
            let columns = ((size.width + TILE_GAP) / (TILE + TILE_GAP))
                .floor()
                .max(1.0) as usize;
            let width = (size.width - (columns - 1) as f32 * TILE_GAP) / columns as f32;
            let built_in = if include_built_in {
                palette::BUILT_IN.as_slice()
            } else {
                &[]
            };
            let cards: Vec<(String, String, bool, palette::Theme)> =
                built_in
                    .iter()
                    .map(|built_in| {
                        (
                            built_in.id.to_owned(),
                            built_in.name.to_owned(),
                            false,
                            built_in.theme,
                        )
                    })
                    .chain(
                        self.cx.themes.custom.iter().map(|custom| {
                            (custom.id.clone(), custom.name.clone(), true, custom.theme)
                        }),
                    )
                    .collect();
            let mut rows = column![].spacing(TILE_GAP);
            for chunk in cards.chunks(columns) {
                let mut line = row![].spacing(TILE_GAP);
                for (id, name, custom, palette) in chunk {
                    line = line.push(self.theme_tile(id, name, *custom, palette, width));
                }
                rows = rows.push(line);
            }
            rows.into()
        })
        .height(Length::Shrink)
        .into()
    }

    /// A theme to choose: a small window in its colours, its name under it,
    /// an accent ring and a tick on the one shown, and a pencil on the
    /// person's own.
    fn theme_tile(
        self,
        id: &str,
        name: &str,
        custom: bool,
        palette: &palette::Theme,
        width: f32,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let current = self.cx.themes.chosen_id() == id;
        let preview = preview(palette, width - 16.0);
        let mut label = row![
            text(name.to_owned())
                .size(14)
                .font(if current { BOLD } else { MEDIUM })
                .color(if current { theme.text } else { theme.secondary })
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .height(24);
        if current {
            label = label.push(self.cx.look.glyph(Icon::Check, 16, theme.accent));
        }
        let (accent, faint, line, hover) = (theme.accent, theme.faint, theme.line, theme.hover);
        let face = button(
            column![preview, container(label).padding([0, 2])]
                .spacing(6)
                .padding(8),
        )
        .padding(0)
        .width(width)
        .height(TILE_HEIGHT)
        .on_press(theme_message(Event::Choose(id.to_owned())))
        .style(move |_, status| {
            let lit = lights(status);
            button::Style {
                background: lit.then(|| hover.into()),
                border: Border {
                    color: if current {
                        accent
                    } else if lit {
                        faint
                    } else {
                        line
                    },
                    width: if current { 2.0 } else { 1.0 },
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        });
        if !custom {
            return face.into();
        }
        let pencil = self
            .cx
            .look
            .icon_button(
                Icon::Pencil,
                tr("Edit"),
                theme_message(Event::Edit(id.to_owned())),
            )
            .size(24.0, 14);
        stack![face, placed(pencil, width - 8.0 - 24.0, 8.0 + 78.0 + 6.0)].into()
    }

    /// The colour editor, for one of the person's themes made of colours
    /// alone: the creator's bar, then every colour beside Delete.
    pub(super) fn editor_view(self, editor: &'a Editor) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let starts_open = self.s.open == Some(Drop::Start);
        let bar = self.editor_bar(
            &editor.name,
            |name| theme_message(Event::Name(name)),
            None,
            editor.editing.is_some(),
            editor.valid().then(|| theme_message(Event::Save)),
            theme_message(Event::Cancel),
        );
        let controls = move |_: f32| -> Element<'a, Message> {
            let mut parts = Vec::new();
            parts.extend(self.theme_note());
            if editor.editing.is_none() {
                let names: Vec<String> = palette::starts(&self.cx.themes.custom)
                    .map(|start| start.name.to_owned())
                    .collect();
                parts.push(form::group([
                    form::heading(theme, tr("START FROM")),
                    form::dropdown(
                        names,
                        Some(editor.start),
                        starts_open,
                        crate::app::settings(SettingsEvent::Drop(Drop::Start)),
                        |index| theme_message(Event::Start(index)),
                    )
                    .view(self.cx.look),
                ]));
            }
            parts.push(
                switch(
                    tr("Dark"),
                    editor.theme.dark,
                    Some(theme_message(Event::Dark(!editor.theme.dark))),
                )
                .description(tr("Drop-down lists and scroll bars draw dark to match."))
                .view(self.cx.look),
            );
            parts.push(form::group([
                form::heading(theme, tr("EVERY COLOUR")),
                form::note(
                    theme,
                    tr("Each as #rgb, #rrggbb or #rrggbbaa, or picked from its swatch. The window shows a change as you make it."),
                ),
                picker_colour_rows(theme, editor),
            ]));
            column(parts).spacing(24).into()
        };
        let aside = move |_: f32| -> Element<'a, Message> {
            // Greyed with the label while a field holds no colour.
            let ink = crate::theme::rgb(if editor.valid() {
                theme.text
            } else {
                theme.faint
            });
            let share = |label, event: fn() -> Event, icon| {
                action(
                    theme,
                    label,
                    Kind::Plain,
                    editor.valid().then(|| theme_message(event())),
                )
                .small()
                .icon(self.cx.look.icon(icon, 16, ink))
            };
            let copy = share("Copy", || Event::Copy, Icon::Copy);
            let export = share("Export", || Event::Export, Icon::Download);
            if editor.editing.is_none() {
                return row![copy, export].spacing(8).into();
            }
            row![
                copy,
                export,
                action(
                    theme,
                    if editor.confirming {
                        tr("Delete it for good")
                    } else {
                        tr("Delete theme")
                    },
                    Kind::Danger,
                    Some(theme_message(Event::Delete)),
                )
                .small()
                .icon(self.cx.look.icon(
                    Icon::Trash,
                    16,
                    crate::theme::rgb(theme.on_danger)
                ),)
            ]
            .spacing(8)
            .into()
        };
        self.editor_frame(bar, None, controls, aside)
    }
}

/// What a theme copied from another is called until it's named.
fn copy_name(name: &str) -> String {
    i18n::fill(tr("{} copy"), &[&name])
}

/// A theme's small window, 78 px tall: the rail with three servers, the
/// sidebar's channels, two messages and the composer, in its colours.
fn preview<'a>(palette: &palette::Theme, width: f32) -> Element<'a, Message> {
    let c = |colour| color(colour);
    let (rail, sidebar, chat, raised, accent, ink, muted) = (
        c(palette.rail),
        c(palette.sidebar),
        c(palette.chat),
        c(palette.raised),
        c(palette.accent),
        c(palette.text),
        c(palette.muted),
    );
    let blob = |w: f32, h: f32, colour: Color, radius: f32| -> Element<'a, Message> {
        container(Space::new())
            .width(w)
            .height(h)
            .style(move |_| crate::style::rounded(colour, radius))
            .into()
    };
    let mut layers = stack![blob(width, 78.0, chat, 6.0)];
    layers = layers.push(blob(16.0, 78.0, rail, 0.0));
    for index in 0..3 {
        let (colour, radius) = if index == 0 {
            (accent, 3.0)
        } else {
            (raised, 5.0)
        };
        layers = layers.push(placed(
            blob(10.0, 10.0, colour, radius),
            3.0,
            6.0 + index as f32 * 13.0,
        ));
    }
    layers = layers.push(placed(blob(38.0, 78.0, sidebar, 0.0), 16.0, 0.0));
    for index in 0..4 {
        let w = if index == 1 {
            24.0
        } else {
            18.0 + (index % 2) as f32 * 4.0
        };
        let colour = if index == 1 { ink } else { muted };
        layers = layers.push(placed(
            blob(w, 4.0, colour, 2.0),
            22.0,
            8.0 + index as f32 * 11.0,
        ));
    }
    let room = width - 66.0;
    for index in 0..2 {
        let y = 8.0 + index as f32 * 22.0;
        let first = index == 0;
        layers = layers
            .push(placed(
                blob(10.0, 10.0, if first { accent } else { muted }, 5.0),
                60.0,
                y,
            ))
            .push(placed(
                blob(if first { 22.0 } else { 16.0 }, 4.0, ink, 2.0),
                74.0,
                y + 1.0,
            ))
            .push(placed(
                blob(
                    (room - 14.0).min(if first { 52.0 } else { 40.0 }),
                    3.0,
                    muted,
                    1.5,
                ),
                74.0,
                y + 8.0,
            ));
    }
    layers = layers
        .push(placed(blob(room, 12.0, raised, 4.0), 60.0, 78.0 - 18.0))
        .push(placed(
            blob(6.0, 6.0, accent, 3.0),
            60.0 + room - 10.0,
            78.0 - 18.0,
        ));
    container(layers.width(width).height(78))
        .clip(true)
        .style(|_| iced::widget::container::Style {
            border: crate::style::round(6),
            ..Default::default()
        })
        .into()
}

impl<'a> Page<'a> {
    /// Appearance's "Use the system title bar", where Yapfast can draw its
    /// own: the desktop's way until it's chosen.
    pub(super) fn title_bar_switch(self) -> Element<'a, Message> {
        if !crate::window_bar::offered() {
            return Space::new().into();
        }
        let system = !self.cx.own_title_bar;
        switch(
            tr("Use the system title bar"),
            system,
            Some(crate::app::settings(SettingsEvent::Pref(
                Pref::SystemTitleBar(!system),
            ))),
        )
        .description(tr("Your desktop draws the window's title bar and buttons, as on its other windows. Off, Yapfast draws its own."))
        .view(self.cx.look)
    }
}

/// The editor's colour rows: `colour_rows` with each swatch opening a colour
/// picker under it.
fn picker_colour_rows<'a>(
    theme: &'a crate::theme::Theme,
    editor: &'a Editor,
) -> Element<'a, Message> {
    iced::widget::responsive(move |size| {
        let columns = if size.width >= super::theme_creator::TWO_COLUMNS {
            2
        } else {
            1
        };
        let per = editor.fields.len().div_ceil(columns);
        let mut line = row![].spacing(super::theme_creator::COLOUR_GAP);
        for column_index in 0..columns {
            let mut cells = column![];
            for index in column_index * per..((column_index + 1) * per).min(editor.fields.len()) {
                let (name, hex, wrong) = &editor.fields[index];
                let shown = editor.colour(index).map_or(Color::TRANSPARENT, color);
                let open = editor.picker == Some(index);
                let card = open.then(|| {
                    let mut code = form::field(theme, COLOUR_CODE, "#rrggbb", hex)
                        .on_input(move |text| theme_message(Event::Colour(index, text)))
                        .on_submit(theme_message(Event::Picker(None)));
                    if *wrong {
                        code = code.style(controls::field_style(
                            theme,
                            Color {
                                a: 0.16,
                                ..theme.danger
                            },
                            true,
                        ));
                    }
                    form::colour_card(
                        theme,
                        ColourPad::new(editor.hsv, |hsv| theme_message(Event::Hsv(hsv))),
                        code,
                    )
                });
                cells = cells.push(
                    form::colour_row(theme, name, shown, hex, *wrong, move |text| {
                        theme_message(Event::Colour(index, text))
                    })
                    .picker(
                        open,
                        theme_message(Event::Picker(Some(index))),
                        card,
                        theme_message(Event::Picker(None)),
                    ),
                );
            }
            line = line.push(container(cells).width(Length::FillPortion(1)));
        }
        line.into()
    })
    .height(Length::Shrink)
    .into()
}
