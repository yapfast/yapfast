//! Appearance's intents: a theme shown at once and kept, a theme file
//! imported, the language, the person's font files, the title bar and the
//! profile switches (`src/ui/appearance.rs`, `src/ui/language.rs`,
//! `on_preferences_changed`'s cosmetics).

use std::path::PathBuf;

use iced::Task;
use yapfast_core::application::Action;
use yapfast_lang::{tr, trn};

use anomi_theme::custom::{self, ImportError, Imported as Theme};
use anomi_theme::fonts::custom::{self as fonts, Loaded, Problem, Stored};
use anomi_vm::settings::appearance::{Intent, Look};
use anomi_vm::settings::frame::Page;

use super::Due;
use crate::app::App;
use crate::message::{Message, Once};
use crate::project::Reach;

/// What became of a font file chosen: its copy, or its name and why none.
pub type Imported = Result<Stored, (String, Problem)>;

/// The extensions the font dialog offers, in both cases: some dialogs match
/// them as written. WOFF2 is offered so that choosing one says it isn't
/// read, rather than hiding the file.
const FONT_EXTENSIONS: &[&str] = &[
    "ttf", "otf", "ttc", "woff", "woff2", "TTF", "OTF", "TTC", "WOFF", "WOFF2",
];

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Import => choose(|| {
            let file = rfd::FileDialog::new()
                .set_title(tr!("Import theme"))
                .add_filter(tr!("Yapfast theme"), &["json"])
                .pick_file()?;
            Some(Due::ThemeChosen(file))
        }),
        Intent::NewTheme => super::editor::new_theme(app),
        Intent::ChooseTheme(id) => {
            choose_theme(app, &id);
            Task::none()
        }
        Intent::EditTheme(id) => super::editor::edit(app, &id),
        Intent::Language(index) => {
            if let Some(language) = crate::language::listed().nth(index) {
                crate::language::choose(app, language.code);
                crate::platform::keybinds::record::show(app);
            }
            Task::none()
        }
        Intent::AddFonts if !app.shown.settings.kept.importing => choose(|| {
            let files = rfd::FileDialog::new()
                .set_title(tr!("Choose font files"))
                .add_filter(tr!("Fonts"), FONT_EXTENSIONS)
                .pick_files()?;
            Some(Due::FontsChosen(files))
        }),
        Intent::RemoveFont(index) => {
            let mut stored = app.shown.settings.kept.fonts.stored();
            if index >= stored.len() {
                return Task::none();
            }
            stored.remove(index);
            set_fonts(app, &stored)
        }
        Intent::ResetFont => set_fonts(app, &[]),
        Intent::SystemTitleBar(on) => title_bar(app, on),
        Intent::Look(look, on) => self::look(app, look, on),
        intent => {
            tracing::debug!(?intent, "nothing to do");
            Task::none()
        }
    }
}

/// Runs a file dialog off the window's thread; nothing comes of one closed
/// without a choice.
fn choose(pick: impl FnOnce() -> Option<Due> + Send + 'static) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(pick).await.ok().flatten() },
        |due| due,
    )
    .and_then(|due| Task::done(Message::Settings(due)))
}

/// Shows the window in a theme, built in or the person's own, and keeps it
/// chosen. False for an id Anomi doesn't know, which changes nothing.
pub fn choose_theme(app: &mut App, id: &str) -> bool {
    let kept = &mut app.shown.settings.kept;
    let Some(theme) = custom::lookup(id, &kept.custom) else {
        return false;
    };
    kept.note = None;
    kept.changed();
    app.theme = theme;
    app.layout.edit(|saved| saved.theme = Some(id.to_owned()));
    app.show(Reach::SETTINGS);
    true
}

/// Keeps the person's own themes in the layout, as the Slint app writes them.
pub(super) fn keep_themes(app: &mut App) {
    let values = (app.shown.settings.kept.custom.iter())
        .filter_map(|theme| serde_json::to_value(theme).ok())
        .collect();
    app.layout.edit(|saved| saved.custom_themes = values);
}

/// Says how a theme file went, under THEME or the editor's name.
pub(super) fn note(app: &mut App, note: String, wrong: bool) {
    let kept = &mut app.shown.settings.kept;
    kept.note = Some((note.clone(), wrong));
    kept.changed();
    if kept.editor.is_some() {
        let editor = &mut app.vm.settings.theme_editor;
        (editor.note, editor.note_wrong) = (note, wrong);
    }
    app.show(Reach::SETTINGS);
}

/// Reads the theme file chosen off the window's thread, as it can be on a
/// slow or stalled network disk.
pub(super) fn read_theme(path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || custom::read(&path))
                .await
                .unwrap_or_else(|error| Err(ImportError::Unreadable(error.to_string())))
        },
        |read| Message::Settings(Due::ThemeRead(Once::new(read))),
    )
}

/// Keeps a theme read from a file among the person's own and shows it, or
/// says why it can't. Nothing changes while the editor is open: the colours
/// shown are the edit's.
pub(super) fn import_theme(app: &mut App, read: Result<Theme, ImportError>) -> Task<Message> {
    if app.shown.settings.kept.editor.is_some() {
        note(
            app,
            tr!("Save or cancel the theme being edited first.").to_owned(),
            true,
        );
        return Task::none();
    }
    let imported = match read {
        Ok(imported) => imported,
        Err(error) => {
            note(app, error.text(), true);
            return Task::none();
        }
    };
    let said = match imported.skipped {
        0 => tr!("Imported {}.", imported.name),
        skipped => trn!(
            "Imported {}. {n} colour in the file couldn't be read, so the base theme's stands in.",
            "Imported {}. {n} colours in the file couldn't be read, so the base theme's stand in.",
            skipped,
            imported.name
        ),
    };
    let custom = &mut app.shown.settings.kept.custom;
    let id = custom::new_id(custom);
    custom.push(custom::Custom {
        id: id.clone(),
        name: imported.name,
        base: imported.base,
        palette: imported.palette,
    });
    keep_themes(app);
    choose_theme(app, &id);
    note(app, said, false);
    Task::none()
}

/// Where the copies of the person's font files are kept.
fn folder(app: &App) -> PathBuf {
    app.root.join(fonts::FOLDER)
}

/// Copies the font files chosen into the data folder, off the window's
/// thread; choosing waits until they're in.
pub(super) fn import_fonts(app: &mut App, paths: Vec<PathBuf>) -> Task<Message> {
    let kept = &mut app.shown.settings.kept;
    if kept.importing || paths.is_empty() {
        return Task::none();
    }
    kept.importing = true;
    kept.changed();
    app.show(Reach::SETTINGS);
    let folder = folder(app);
    Task::perform(
        async move {
            let copying = tokio::task::spawn_blocking(move || fonts::import(&folder, &paths));
            copying.await.unwrap_or_else(|error| {
                tracing::error!(%error, "the font files' copying stopped");
                Vec::new()
            })
        },
        |imported| Message::Settings(Due::FontsCopied(Once::new(imported))),
    )
}

/// Adds the fonts copied in to those loaded. A file that gave no font is
/// left out, and the note says why.
pub(super) fn add_fonts(app: &mut App, imported: Vec<Imported>) -> Task<Message> {
    let kept = &mut app.shown.settings.kept;
    kept.importing = false;
    let mut stored = kept.fonts.stored();
    let (mut fresh, mut refused) = (Vec::new(), Vec::new());
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
    let previous = std::mem::take(&mut app.shown.settings.kept.fonts);
    let mut loaded = Loaded::load(&folder(app), previous, &stored);
    for entry in loaded.drop_failed(&fresh) {
        if let Err(problem) = entry.outcome {
            refused.push((entry.stored.name, problem));
        }
    }
    let kept = &mut app.shown.settings.kept;
    (kept.fonts, kept.refused) = (loaded, refused);
    keep_fonts(app)
}

/// Loads these fonts in place of the ones loaded.
fn set_fonts(app: &mut App, stored: &[Stored]) -> Task<Message> {
    let previous = std::mem::take(&mut app.shown.settings.kept.fonts);
    let loaded = Loaded::load(&folder(app), previous, stored);
    let kept = &mut app.shown.settings.kept;
    kept.fonts = loaded;
    kept.refused.clear();
    keep_fonts(app)
}

/// Keeps the fonts loaded chosen, deletes the copies no longer chosen, and
/// draws the window again in them. While files are being copied in, their
/// copies wait for the next change.
fn keep_fonts(app: &mut App) -> Task<Message> {
    let stored = app.shown.settings.kept.fonts.stored();
    let values = stored.iter().map(super::start::font_value).collect();
    app.layout.edit(|saved| saved.ui_fonts = values);
    if !app.shown.settings.kept.importing {
        fonts::sweep(&folder(app), &stored);
    }
    app.shown.settings.kept.changed();
    app.show(Reach::SETTINGS);
    anomi_theme::fonts::redrawn(Message::Settings(Due::Repainted))
}

/// The desktop's title bar or Anomi's own: kept, and the window made again,
/// as a window is made with its frame or without it (`remake_window`).
fn title_bar(app: &mut App, on: bool) -> Task<Message> {
    app.layout.edit(|saved| saved.system_title_bar = Some(on));
    if app.window.is_none() {
        return Task::none();
    }
    let closed = crate::platform::forget::window(app);
    let opened = crate::platform::window::open(app, None);
    let back = super::open(app, Page::Appearance);
    Task::batch([closed, opened, back])
}

/// A profile switch: kept, handed to the core, and the moving ones held
/// still or let move.
fn look(app: &mut App, look: Look, on: bool) -> Task<Message> {
    app.layout.edit(|saved| match look {
        Look::ProfileEffects => saved.profile_effects = Some(on),
        Look::Decorations => saved.decorations = Some(on),
        Look::Nameplates => saved.nameplates = Some(on),
        Look::Moving => saved.moving_cosmetics = Some(on),
        Look::NameStyles => saved.name_styles = Some(on),
        Look::ServerTags => saved.server_tags = Some(on),
        Look::ProfileFrames => saved.profile_frames = Some(on),
    });
    let cosmetics = crate::layout::carry::cosmetics(&app.layout);
    crate::pictures::set_moving_collectibles(cosmetics.moving);
    let hidden = !app.layout.nameplates.unwrap_or(true);
    let nameplates = app.dispatch(Action::HideNameplates(hidden));
    Task::batch([nameplates, app.dispatch(Action::SetCosmetics(cosmetics))])
}
