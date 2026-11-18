//! The theme editor (`src/ui/appearance.rs:45`-`:330`): a theme made from
//! another or one of the person's own changed, shown on the whole window as
//! it is edited. What the window shows while editing is the edit's alone:
//! cancelling, closing Settings or leaving Appearance shows the theme
//! chosen again, and nothing of an unsaved edit reaches the layout.

use iced::Task;
use yapfast_lang::tr;

use anomi_theme::custom::{self, Custom};
use anomi_theme::{Palette, Theme};
use anomi_vm::settings::theme_editor::{Colour, Intent, ThemeEditor};

use super::Due;
use super::appearance::{choose_theme, keep_themes, note};
use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::project::settings::appearance::{Editor, chosen_id};

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    if app.shown.settings.kept.editor.is_none() {
        return Task::none();
    }
    app.vm.settings.theme_editor.apply(&intent);
    match intent {
        // The name, and Delete asking again: the editor alone.
        Intent::Name(_) | Intent::Delete => {}
        Intent::Dark(dark) => {
            if let Some(editor) = &mut app.shown.settings.kept.editor {
                editor.palette.dark = dark;
            }
            preview(app);
        }
        Intent::Colour(index, _) => {
            let row = app.vm.settings.theme_editor.colours.get(index);
            let colour = row.filter(|row| !row.wrong).map(|row| row.colour);
            let editor = app.shown.settings.kept.editor.as_mut();
            let slot = editor.and_then(|editor| editor.palette.colour_mut(index));
            if let (Some(colour), Some(slot)) = (colour, slot)
                && *slot != colour
            {
                *slot = colour;
                preview(app);
            }
        }
        Intent::Start(index) => restart(app, index),
        Intent::Save => save(app),
        Intent::Cancel => cancel(app),
        Intent::Export => return export(app),
        Intent::ConfirmDelete => delete(app),
        intent => tracing::debug!(?intent, "not handled by the iced interface yet"),
    }
    Task::none()
}

/// What a theme copied from another is called until the person names it.
fn copy_name(name: &str) -> String {
    tr!("{} copy", name)
}

/// A row for each colour of `palette`, by its name in `layout.json`.
fn rows(palette: &Palette) -> Vec<Colour> {
    (Palette::NAMES.iter().enumerate())
        .filter_map(|(index, name)| Some(Colour::new(custom::token(name), palette.colour(index)?)))
        .collect()
}

/// Opens the editor on a copy of the theme shown, which the person can swap
/// for any other.
pub(super) fn new_theme(app: &mut App) -> Task<Message> {
    let kept = &app.shown.settings.kept;
    let starts: Vec<_> = custom::starts(&kept.custom).collect();
    let current = app.theme.id();
    let index = (starts.iter().position(|start| start.id == current)).unwrap_or(0);
    let start = &starts[index];
    let shown = ThemeEditor {
        saved: false,
        name: copy_name(start.name),
        dark: start.palette.dark,
        starts: starts.iter().map(|start| start.name.to_owned()).collect(),
        start: index,
        colours: rows(&start.palette),
        ..ThemeEditor::default()
    };
    let editor = Editor {
        editing: None,
        base: start.base,
        palette: start.palette,
    };
    open(app, shown, editor);
    Task::none()
}

/// Opens the editor on one of the person's own themes.
pub(super) fn edit(app: &mut App, id: &str) -> Task<Message> {
    let kept = &app.shown.settings.kept;
    let Some(theme) = kept.custom.iter().find(|theme| theme.id == id) else {
        return Task::none();
    };
    let shown = ThemeEditor {
        saved: true,
        name: theme.name.clone(),
        dark: theme.palette.dark,
        colours: rows(&theme.palette),
        ..ThemeEditor::default()
    };
    let editor = Editor {
        editing: Some(theme.id.clone()),
        base: theme.base,
        palette: theme.palette,
    };
    open(app, shown, editor);
    Task::none()
}

fn open(app: &mut App, shown: ThemeEditor, editor: Editor) {
    app.vm.settings.theme_editor = shown;
    let kept = &mut app.shown.settings.kept;
    kept.note = None;
    kept.editor = Some(editor);
    kept.changed();
    preview(app);
}

/// Shows the whole window in the theme as edited so far.
fn preview(app: &mut App) {
    let Some(editor) = &app.shown.settings.kept.editor else {
        return;
    };
    let name = &app.vm.settings.theme_editor.name;
    app.theme = Theme::new("editing", name.clone(), editor.palette);
    app.show(Reach::SETTINGS);
}

/// A new theme starts again from another, and takes its name too unless
/// the person named it.
fn restart(app: &mut App, index: usize) {
    let kept = &mut app.shown.settings.kept;
    let starts: Vec<_> = custom::starts(&kept.custom).collect();
    let (Some(start), Some(editor)) = (starts.get(index), kept.editor.as_mut()) else {
        return;
    };
    let shown = &mut app.vm.settings.theme_editor;
    let before = starts.iter().map(|start| copy_name(start.name));
    if before.into_iter().any(|name| name == shown.name) {
        shown.name = copy_name(start.name);
    }
    (editor.base, editor.palette) = (start.base, start.palette);
    shown.colours = rows(&start.palette);
    shown.dark = start.palette.dark;
    preview(app);
}

/// The name in the editor's field, or "My theme" if it holds none.
fn name(app: &App) -> String {
    match app.vm.settings.theme_editor.name.trim() {
        "" => tr!("My theme").to_owned(),
        name => name.to_owned(),
    }
}

/// Keeps the theme as edited among the person's own, and chooses it.
fn save(app: &mut App) {
    if !app.vm.settings.theme_editor.can_save() {
        return;
    }
    let name = name(app);
    let kept = &mut app.shown.settings.kept;
    let Some(editor) = kept.editor.take() else {
        return;
    };
    let custom = &mut kept.custom;
    let saved = (editor.editing.as_ref()).and_then(|id| custom.iter_mut().find(|t| t.id == *id));
    let id = match saved {
        Some(saved) => {
            (saved.name, saved.palette) = (name, editor.palette);
            saved.id.clone()
        }
        None => {
            let id = custom::new_id(custom);
            custom.push(Custom {
                id: id.clone(),
                name,
                base: editor.base,
                palette: editor.palette,
            });
            id
        }
    };
    closed(app);
    keep_themes(app);
    choose_theme(app, &id);
}

/// Closes the editor and shows the theme chosen before it opened. Closing
/// Settings or leaving Appearance does this too.
pub(super) fn cancel(app: &mut App) {
    if app.shown.settings.kept.editor.take().is_none() {
        return;
    }
    closed(app);
    restore(app);
}

/// Shows the theme the layout chose again.
fn restore(app: &mut App) {
    let custom = &app.shown.settings.kept.custom;
    app.theme = custom::chosen(Some(&chosen_id(&app.layout, custom)), custom);
    app.show(Reach::SETTINGS);
}

/// The editor's rows go, so they're held only while it's open.
fn closed(app: &mut App) {
    app.vm.settings.theme_editor = ThemeEditor::default();
    let kept = &mut app.shown.settings.kept;
    kept.note = None;
    kept.changed();
}

/// Deletes the theme being edited. If it was the chosen one, the default
/// shows.
fn delete(app: &mut App) {
    let Some(editor) = app.shown.settings.kept.editor.take() else {
        return;
    };
    if let Some(id) = editor.editing {
        app.shown
            .settings
            .kept
            .custom
            .retain(|theme| theme.id != id);
        keep_themes(app);
    }
    closed(app);
    let id = chosen_id(&app.layout, &app.shown.settings.kept.custom);
    choose_theme(app, &id);
}

/// Asks where to save the theme being edited, as it stands now, and writes
/// it there, off the window's thread: the folder can be on a slow disk.
fn export(app: &mut App) -> Task<Message> {
    let Some(editor) = app.shown.settings.kept.editor.as_ref() else {
        return Task::none();
    };
    if !app.vm.settings.theme_editor.can_save() {
        return Task::none();
    }
    let name = name(app);
    let text = custom::export(&name, editor.base, &editor.palette);
    let file_name = custom::file_name(&name);
    Task::perform(
        async move {
            let write = move || {
                let file = rfd::FileDialog::new()
                    .set_title(tr!("Export theme"))
                    .set_file_name(file_name)
                    .add_filter(tr!("Yapfast theme"), &["json"])
                    .save_file()?;
                let name = file
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
                let written = std::fs::write(&file, text).map_err(|error| error.to_string());
                Some((name.unwrap_or_default(), written))
            };
            tokio::task::spawn_blocking(write).await.ok().flatten()
        },
        |written| written,
    )
    .and_then(|(name, written)| Task::done(Message::Settings(Due::ThemeWritten(name, written))))
}

/// Says how writing the theme file went, under the editor's name.
pub(super) fn exported(app: &mut App, name: &str, written: Result<(), String>) {
    match written {
        Ok(()) => note(app, tr!("Exported to {}.", name), false),
        Err(error) => {
            tracing::warn!(%error, "could not write a theme file");
            note(app, tr!("Couldn't save the theme file: {}", error), true);
        }
    }
}
