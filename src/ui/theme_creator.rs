//! Appearance's theme creator: Simple, a few plain choices that make a whole
//! theme. Each choice derives the theme's colours at once
//! (`yapfast_core::themes::creator`), shows them on the window, and writes a
//! draft package whose shapes, fonts, layout and art are made ready on
//! another thread, as an installed theme's are, and shown when they are.
//! Save keeps the theme among the person's own, with its package.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use yapfast_core::themes::creator::{self, Density, Messages};
pub(super) use yapfast_core::themes::creator::{Background, Spec};
use yapfast_ui::{Creator, CreatorFont, CreatorStart};

use super::*;

/// The built-in fonts the creator offers, as `ui/fonts/` ships them.
pub(super) fn font_bytes(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "AtkinsonHyperlegibleNext-Regular.ttf" => {
            include_bytes!("../../ui/fonts/AtkinsonHyperlegibleNext-Regular.ttf")
        }
        "DisplayName-Tempo.ttf" => include_bytes!("../../ui/fonts/DisplayName-Tempo.ttf"),
        "DisplayName-Modern.ttf" => include_bytes!("../../ui/fonts/DisplayName-Modern.ttf"),
        "DisplayName-MonkeyBars.ttf" => include_bytes!("../../ui/fonts/DisplayName-MonkeyBars.ttf"),
        "DisplayName-Journal.ttf" => include_bytes!("../../ui/fonts/DisplayName-Journal.ttf"),
        "DisplayName-8Bit.ttf" => include_bytes!("../../ui/fonts/DisplayName-8Bit.ttf"),
        "DisplayName-Mainframe.ttf" => include_bytes!("../../ui/fonts/DisplayName-Mainframe.ttf"),
        _ => return None,
    })
}

/// What a picture read off the UI thread brings.
struct Arrival {
    prepared: Result<creator::PreparedPicture, String>,
}

/// The editor before a change, to take the change back: the creator's
/// choices and the theme's colours.
struct Before {
    spec: Spec,
    theme: theme::Theme,
    base: &'static str,
}

/// The most changes Undo takes back.
const UNDOS: usize = 64;

/// The grounds' tint when they're plain greys.
const NEUTRAL: u32 = 0x80_8080;

/// Whether a ground's colour tints nothing: a grey.
fn neutral(ground: u32) -> bool {
    creator::lch(ground)[1] < 0.004
}

thread_local! {
    /// What Undo goes back through, the latest change last; held only while
    /// the creator is open.
    static HISTORY: RefCell<Vec<Before>> = const { RefCell::new(Vec::new()) };
    /// What the last change was to, for a run of changes to one thing, as a
    /// colour typed letter by letter, to be one to take back.
    static LAST: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    /// The picture's thumbnail, which the preview shows blurred and dimmed
    /// as chosen; held only while the creator is open.
    static THUMBNAIL: RefCell<Option<image::RgbaImage>> = const { RefCell::new(None) };
    /// The pictures offered from the person's Pictures folder, by place in
    /// `Creator.suggestions`.
    static SUGGESTED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    /// A picture read off the UI thread, waiting for `Creator.arrived`.
    static ARRIVED: RefCell<Option<Arrival>> = const { RefCell::new(None) };
}

fn rgb(colour: slint::Color) -> u32 {
    colour.as_argb_encoded() & 0xff_ffff
}

fn colour(rgb: u32) -> slint::Color {
    slint::Color::from_argb_encoded(0xff00_0000 | rgb)
}

/// A theme's colours as the creator names them.
fn colours_of(theme: &theme::Theme) -> Vec<(&'static str, u32)> {
    creator::BASE_TOKENS
        .iter()
        .filter_map(|token| {
            let index = theme::token_index(token)?;
            Some((*token, theme.colour(index)?.as_argb_encoded()))
        })
        .collect()
}

/// Sets a theme to derived colours, over their base's for the rest.
fn take_colours(theme: &mut theme::Theme, derived: &creator::Derived) -> &'static str {
    let base = theme::find(derived.base).unwrap_or(&theme::BUILT_IN[0]);
    *theme = base.theme;
    theme.dark = derived.dark;
    for (token, value) in &derived.colours {
        if let Some(slot) = theme::token_index(token).and_then(|index| theme.colour_mut(index)) {
            *slot = slint::Color::from_argb_encoded(*value);
        }
    }
    base.id
}

/// A spec's colours over the built-in theme it started from, while that
/// one is as light or dark; otherwise over Yapfast's own.
fn based(spec: &Spec, mut derived: creator::Derived) -> creator::Derived {
    if let Some(base) = spec
        .base
        .as_deref()
        .and_then(theme::find)
        .filter(|base| base.theme.dark == derived.dark)
    {
        derived.base = base.id;
    }
    derived
}

/// A spec that starts as a theme looks, over `base`, the built-in theme it
/// is or began as.
pub(super) fn spec_from(theme: &theme::Theme, base: &str) -> Spec {
    Spec {
        base: Some(base.to_owned()),
        accent: rgb(theme.accent),
        ground: rgb(theme.chat),
        background: if theme.dark {
            Background::Dark
        } else {
            Background::Light
        },
        ..Spec::default()
    }
}

/// Writes any creator theme's package that's missing, as after a copy of
/// `layout.json` without its data folder, so it can show at start.
pub(super) fn ensure_folders(root: &Path, custom: &[theme::Custom]) {
    for theme in custom {
        let Some(spec) = &theme.creator else { continue };
        let folder = match &theme.folder {
            Some(folder) => root.join(folder),
            None => theme_look::creator_folder(root, &theme.id),
        };
        if folder.join(yapfast_core::themes::MANIFEST).exists() {
            continue;
        }
        let mut spec = spec.clone();
        if !folder.join(creator::PICTURE).exists() && spec.background == Background::Picture {
            spec.background = Background::Dark;
        }
        let derived = based(&spec, creator::finish(&spec, colours_of(&theme.theme)));
        if let Err(error) =
            creator::write_folder(&folder, &spec, &theme.name, &theme.id, &derived, font_bytes)
        {
            tracing::warn!(%error, id = theme.id, "couldn't write a creator theme's package");
        }
    }
}

/// Where the creator keeps a theme's package, relative to the data folder,
/// as `layout.json`'s `folder` names it.
pub(super) fn creator_relative(id: &str) -> String {
    format!(
        "themes/creator/{}",
        creator::slug(id.strip_prefix("custom:").unwrap_or(id))
    )
}

/// Tells `theme_look` which of the person's themes show from a package: the
/// creator's, from their folder, and any other that names a folder of its
/// own, its layers merged with its colours into a package under
/// `<data>/themes/applied/` (as Export merges them), so its fonts and art
/// load in the window too.
pub(super) fn register(root: &Path, custom: &[theme::Custom]) {
    use yapfast_core::themes::package;
    let mut folders = Vec::new();
    for theme in custom {
        let Some(folder) = &theme.folder else {
            continue;
        };
        let source = root.join(folder);
        if theme.creator.is_some() {
            folders.push((theme.id.clone(), source));
            continue;
        }
        let Ok(layers) = std::fs::read_to_string(source.join(yapfast_core::themes::MANIFEST))
        else {
            continue;
        };
        let colours = colours_of(&theme.theme);
        let named: Vec<(&str, u32)> = colours
            .iter()
            .map(|(token, colour)| (*token, *colour))
            .collect();
        let applied = root.join("themes").join("applied").join(creator::slug(
            theme.id.strip_prefix("custom:").unwrap_or(&theme.id),
        ));
        let _ = std::fs::remove_dir_all(&applied);
        let made = package::export_with(
            &theme.name,
            "",
            theme.base,
            theme.theme.dark,
            &named,
            Some(&layers),
        )
        .and_then(|text| package::write_export(&applied, &text, Some(&source)));
        match made {
            Ok(_) => folders.push((theme.id.clone(), applied)),
            Err(error) => {
                tracing::warn!(%error, id = theme.id, "a custom theme's folder couldn't be applied")
            }
        }
    }
    theme_look::set_created(folders);
}

impl Adapter {
    fn draft_folder(&self) -> PathBuf {
        theme_look::creator_folder(&self.root, theme_look::DRAFT)
    }

    /// Opens Simple on a spec, its draft a copy of `from`'s package if it
    /// has one. `recolour` derives the colours anew; otherwise the editor's
    /// stand, as fine-tuned in Advanced.
    pub(super) fn open_creator(
        &mut self,
        window: &YapfastWindow,
        spec: Spec,
        from: Option<PathBuf>,
        recolour: bool,
    ) {
        forget(window);
        self.begin_creator(window, spec, from, recolour);
    }

    /// Keeps the editor as it stands, for Undo, before a change to it.
    /// `what` names what a change in a run is to, so the run is one step.
    pub(super) fn creator_remember(&mut self, window: &YapfastWindow, what: Option<u32>) {
        let Some(editor) = self.theme_editor.as_ref() else {
            return;
        };
        let Some(spec) = editor.creator.clone() else {
            return;
        };
        if what.is_some() && LAST.get() == what {
            return;
        }
        LAST.set(what);
        HISTORY.with_borrow_mut(|history| {
            if history.len() == UNDOS {
                history.remove(0);
            }
            history.push(Before {
                spec,
                theme: editor.theme,
                base: editor.base,
            });
        });
        window.global::<Creator>().set_can_undo(true);
    }

    /// Undo: the editor as it stood before the last change.
    fn creator_undo(&mut self, window: &YapfastWindow) {
        let Some(before) = HISTORY.with_borrow_mut(Vec::pop) else {
            return;
        };
        LAST.set(None);
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        editor.creator = Some(before.spec);
        editor.theme = before.theme;
        editor.base = before.base;
        let page = window.global::<Creator>();
        page.set_can_undo(HISTORY.with_borrow(|history| !history.is_empty()));
        page.set_start(-1);
        self.creator_fonts(window);
        self.creator_changed(window, false);
    }

    fn begin_creator(
        &mut self,
        window: &YapfastWindow,
        spec: Spec,
        from: Option<PathBuf>,
        recolour: bool,
    ) {
        let draft = self.draft_folder();
        let _ = std::fs::remove_dir_all(&draft);
        let copied = from
            .filter(|from| from.is_dir())
            .is_some_and(|from| creator::copy_folder(&from, &draft).is_ok());
        let mut spec = spec;
        if !copied {
            spec.picture = None;
            spec.fonts.clear();
            if spec.background == Background::Picture {
                spec.background = Background::Dark;
            }
        }
        THUMBNAIL.set(None);
        if spec.picture.is_some()
            && let Ok(bytes) = std::fs::read(draft.join(creator::PICTURE))
            && let Ok(picture) = image::load_from_memory(&bytes)
        {
            THUMBNAIL.set(Some(picture.thumbnail(480, 480).to_rgba8()));
        }
        let page = window.global::<Creator>();
        page.set_simple(true);
        page.set_note("".into());
        page.set_note_bad(false);
        page.set_picking_headings(false);
        page.set_presets(ModelRc::new(VecModel::from(
            creator::ACCENTS
                .iter()
                .map(|accent| colour(*accent))
                .collect::<Vec<_>>(),
        )));
        let starts: Vec<CreatorStart> = theme::starts(&self.layout.custom_themes)
            .map(|start| CreatorStart {
                name: start.name.into(),
                ground: start.theme.chat,
                side: start.theme.sidebar,
                accent: start.theme.accent,
            })
            .collect();
        page.set_starts(ModelRc::new(VecModel::from(starts)));
        page.set_start(-1);
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.creator = Some(spec);
        }
        self.creator_fonts(window);
        suggest_pictures(window);
        self.creator_changed(window, recolour);
    }

    /// The font tiles: Yapfast's own, then the theme's imported ones, each
    /// registered for its sample.
    fn creator_fonts(&self, window: &YapfastWindow) {
        let Some(spec) = self
            .theme_editor
            .as_ref()
            .and_then(|editor| editor.creator.as_ref())
        else {
            return;
        };
        let draft = self.draft_folder();
        let mut fonts: Vec<CreatorFont> = creator::FONTS
            .iter()
            .map(|font| CreatorFont {
                key: font.key.into(),
                label: font.label.into(),
                family: font.family.into(),
                weight: font.weight,
                imported: false,
            })
            .collect();
        for font in &spec.fonts {
            let path = draft.join(&font.file);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(family) = theme_look::register_creator_font(bytes, &path) else {
                continue;
            };
            fonts.push(CreatorFont {
                key: font.file.as_str().into(),
                label: font.family.as_str().into(),
                family: family.into(),
                weight: 400,
                imported: true,
            });
        }
        window
            .global::<Creator>()
            .set_fonts(ModelRc::new(VecModel::from(fonts)));
    }

    /// A choice changed: the colours show at once, the controls say what's
    /// chosen, and the draft package is written and made ready to show.
    pub(super) fn creator_changed(&mut self, window: &YapfastWindow, recolour: bool) {
        let draft = self.draft_folder();
        let name = window.get_theme_name().to_string();
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        let Some(spec) = editor.creator.as_ref() else {
            return;
        };
        let derived = if recolour {
            based(spec, creator::derive(spec))
        } else {
            based(spec, creator::finish(spec, colours_of(&editor.theme)))
        };
        // Colours the person set that were moved to stay readable.
        let moved = !recolour
            && ["text", "secondary", "muted", "link"].iter().any(|token| {
                let set = theme::token_index(token)
                    .and_then(|index| editor.theme.colour(index))
                    .map(rgb);
                set != derived.colour(token).map(|colour| colour & 0xff_ffff)
            });
        show_readable(window, &derived, moved);
        editor.base = take_colours(&mut editor.theme, &derived);
        editor.fill();
        window.set_theme_dark(editor.theme.dark);
        window.set_theme_can_save(true);
        let spec = spec.clone();
        let theme = editor.theme;
        show_spec(window, &spec, &derived);
        self.set_theme(window, &theme);
        match creator::write_folder(&draft, &spec, &name, "draft", &derived, font_bytes) {
            Ok(()) => preview(window, theme_look::DRAFT),
            Err(error) => tracing::warn!(%error, "couldn't write the theme creator's draft"),
        }
    }

    /// Changes the spec and shows it.
    fn creator_edit(
        &mut self,
        window: &YapfastWindow,
        recolour: bool,
        change: impl FnOnce(&mut Spec),
    ) {
        self.creator_remember(window, None);
        if let Some(spec) = self
            .theme_editor
            .as_mut()
            .and_then(|editor| editor.creator.as_mut())
        {
            change(spec);
        }
        self.creator_changed(window, recolour);
    }

    /// Start from: another theme's colours, or all of a creator theme.
    fn creator_start(&mut self, window: &YapfastWindow, index: i32) {
        let Some(start) = usize::try_from(index)
            .ok()
            .and_then(|index| theme::starts(&self.layout.custom_themes).nth(index))
        else {
            return;
        };
        let (id, name, theme, base) = (
            start.id.to_owned(),
            start.name.to_owned(),
            start.theme,
            start.base,
        );
        let custom = self
            .layout
            .custom_themes
            .iter()
            .find(|custom| custom.id == id)
            .cloned();
        // The name follows the start unless the person named it.
        let named = window.get_theme_name().to_string();
        if named.is_empty() || named.ends_with(" copy") || named == "My theme" {
            window.set_theme_name(format!("{name} copy").into());
        }
        let spec = match custom.as_ref().and_then(|custom| custom.creator.clone()) {
            Some(spec) => spec,
            None => spec_from(&theme, base),
        };
        let from = custom
            .as_ref()
            .filter(|custom| custom.creator.is_some())
            .map(|custom| theme_look::creator_folder(&self.root, &custom.id));
        // A start can be taken back, but not one that brings another
        // theme's picture and fonts into the draft.
        if from.is_none() {
            self.creator_remember(window, None);
        } else {
            forget(window);
        }
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.theme = theme;
        }
        let recolour = from.is_none();
        self.begin_creator(window, spec, from, recolour);
        window.global::<Creator>().set_start(index);
        // Advanced's Start from names it too.
        window.set_theme_start(index);
    }

    /// A picture the person chose, read: it becomes the background, and its
    /// colours the accent and the grounds' tint.
    fn creator_picture_arrived(&mut self, window: &YapfastWindow) {
        let Some(arrival) = ARRIVED.take() else {
            return;
        };
        let page = window.global::<Creator>();
        page.set_reading_picture(false);
        let prepared = match arrival.prepared {
            Ok(prepared) => prepared,
            Err(reason) => {
                page.set_note(reason.into());
                page.set_note_bad(true);
                return;
            }
        };
        if let Err(error) = creator::store_picture(&self.draft_folder(), &prepared.jpeg) {
            page.set_note(format!("The picture couldn't be kept: {error}").into());
            page.set_note_bad(true);
            return;
        }
        page.set_note("".into());
        THUMBNAIL.set(Some(prepared.thumbnail));
        let first = self
            .theme_editor
            .as_ref()
            .and_then(|editor| editor.creator.as_ref())
            .is_some_and(|spec| spec.picture.is_none());
        // The picture's file replaced the one before it, which no Undo
        // could show again.
        if !first {
            forget(window);
        }
        self.creator_edit(window, true, |spec| {
            spec.picture = Some(prepared.picture);
            spec.background = Background::Picture;
            spec.accent = prepared.accent;
            spec.ground = prepared.ground;
            if first {
                spec.glass = true;
            }
        });
        if !first {
            forget(window);
        }
    }

    /// Import font…: a font file copied into the theme, offered beside the
    /// others and chosen for the role being picked.
    fn creator_font_imported(&mut self, window: &YapfastWindow, path: &Path) {
        let page = window.global::<Creator>();
        let size = std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let most = yapfast_core::themes::check::MAX_FONT_BYTES;
        let bytes = if size > most {
            Err(format!(
                "That font is larger than {} MB, the most a theme can carry",
                most >> 20
            ))
        } else {
            std::fs::read(path).map_err(|_| creator::NOT_A_FONT.to_owned())
        };
        let stored = bytes.and_then(|bytes| creator::store_font(&self.draft_folder(), &bytes));
        let imported = match stored {
            Ok(imported) => imported,
            Err(reason) => {
                page.set_note(reason.into());
                page.set_note_bad(true);
                return;
            }
        };
        let mut note = format!("Added {}.", imported.family);
        if imported.partial {
            note.push_str(" Some characters will use the default font.");
        }
        note.push_str(" Only share themes with fonts you're allowed to share.");
        page.set_note(note.into());
        page.set_note_bad(false);
        let headings = page.get_picking_headings();
        let file = imported.file.clone();
        self.creator_remember(window, None);
        if let Some(spec) = self
            .theme_editor
            .as_mut()
            .and_then(|editor| editor.creator.as_mut())
        {
            spec.fonts.retain(|font| font.file != imported.file);
            spec.fonts.push(imported);
            if headings {
                spec.headings = Some(file);
            } else {
                spec.body = file;
            }
        }
        self.creator_fonts(window);
        self.creator_changed(window, false);
    }

    /// Advanced: every colour of the theme Simple made, to fine-tune.
    pub(super) fn creator_advanced(&mut self, window: &YapfastWindow) {
        if let Some(editor) = self.theme_editor.as_ref() {
            editor.fill();
            window.set_theme_dark(editor.theme.dark);
        }
        window.global::<Creator>().set_simple(false);
    }

    /// The editor's theme changed in Advanced: the draft follows it.
    pub(super) fn creator_recoloured(&mut self, window: &YapfastWindow) {
        if self
            .theme_editor
            .as_ref()
            .is_some_and(|editor| editor.creator.is_some())
        {
            self.creator_changed(window, false);
        }
    }

    /// Saves the creator's theme: its package goes to the theme's own
    /// folder. False if it couldn't be written.
    pub(super) fn creator_save(
        &mut self,
        id: &str,
        name: &str,
        spec: &Spec,
        theme: &theme::Theme,
    ) -> bool {
        let draft = self.draft_folder();
        let derived = based(spec, creator::finish(spec, colours_of(theme)));
        let written =
            creator::write_folder(&draft, spec, name, id, &derived, font_bytes).and_then(|()| {
                creator::copy_folder(&draft, &theme_look::creator_folder(&self.root, id))
            });
        let _ = std::fs::remove_dir_all(&draft);
        match written {
            Ok(()) => true,
            Err(error) => {
                self.app.state.notice = Some(format!("The theme couldn't be saved: {error}"));
                false
            }
        }
    }

    /// The creator closed without saving: its draft goes.
    pub(super) fn creator_closed(&mut self, window: &YapfastWindow) {
        let _ = std::fs::remove_dir_all(self.draft_folder());
        THUMBNAIL.set(None);
        SUGGESTED.set(Vec::new());
        let page = window.global::<Creator>();
        page.set_suggestions(ModelRc::default());
        page.set_fonts(ModelRc::default());
        page.set_starts(ModelRc::default());
        page.set_picture(Image::default());
        page.set_picture_colours(ModelRc::default());
        page.set_note("".into());
        page.set_readable("".into());
        page.set_can_export(false);
        forget(window);
    }

    /// Shows the chosen theme again, made ready first if it's a package.
    pub(super) fn show_chosen(&mut self, window: &YapfastWindow) {
        let chosen = self.layout.theme.clone();
        match chosen.filter(|id| theme_look::is_package(id) && !theme_look::is_ready(id)) {
            Some(id) => preview(window, &id),
            None => {
                let chosen = self.layout.theme.clone();
                self.show_theme(window, chosen.as_deref());
            }
        }
    }

    /// A theme the creator asked for is ready: the draft while the creator
    /// is open, or the chosen theme after it closed.
    fn creator_ready(&mut self, window: &YapfastWindow, id: &str) {
        let open = self.theme_editor.is_some();
        let wanted = if id == theme_look::DRAFT {
            open
        } else {
            !open && self.layout.theme.as_deref() == Some(id)
        };
        if wanted && theme_look::is_ready(id) {
            self.show_theme(window, Some(id));
        }
    }

    /// Export: the theme as made, a package folder in a folder the person
    /// picks.
    fn creator_export(
        &mut self,
        window: &YapfastWindow,
        adapter_weak: std::rc::Weak<RefCell<Adapter>>,
    ) {
        let Some(editor) = self.theme_editor.as_ref() else {
            return;
        };
        let Some(spec) = editor.creator.clone() else {
            return;
        };
        let name = window.get_theme_name().to_string();
        let draft = self.draft_folder();
        let derived = based(&spec, creator::finish(&spec, colours_of(&editor.theme)));
        let folder_name = creator::slug(if name.trim().is_empty() {
            "my theme"
        } else {
            &name
        });
        if let Err(error) =
            creator::write_folder(&draft, &spec, &name, &folder_name, &derived, font_bytes)
        {
            self.app.state.notice = Some(format!("The theme couldn't be exported: {error}"));
            self.refresh(window);
            return;
        }
        // EXPORT HOOK: the draft is now a whole package folder (theme.json,
        // picture, fonts, LICENSE). When the shared custom-theme export
        // lands (branch themes-tools: `theme_tools::write_custom`, with a
        // custom theme's own `folder`), point that `folder` at
        // `themes/creator/<n>` and export through it instead of this copy.
        let weak = window.as_weak();
        self.dialog(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Export the theme as a package")
                .pick_folder()
                .await;
            let (Some(picked), Some(adapter)) = (picked, adapter_weak.upgrade()) else {
                return;
            };
            let target = picked.path().join(&folder_name);
            let copied = creator::copy_folder(&draft, &target);
            let mut adapter = adapter.borrow_mut();
            adapter.app.state.notice = Some(match copied {
                Ok(()) => format!(
                    "Exported to {}. Check it with `yapfast theme check`, and share it on GitHub.",
                    target.display()
                ),
                Err(error) => format!("The theme couldn't be exported: {error}"),
            });
            if let Some(window) = weak.upgrade() {
                adapter.refresh(&window);
            }
        });
    }
}

/// Makes a theme ready off the UI thread, then shows it through
/// `Creator.ready`.
fn preview(window: &YapfastWindow, id: &str) {
    let ready = SharedString::from(id);
    theme_look::prepare(window, id, move |window| {
        window.global::<Creator>().invoke_ready(ready);
    });
}

/// Sets the creator's controls from a spec.
fn show_spec(window: &YapfastWindow, spec: &Spec, derived: &creator::Derived) {
    let page = window.global::<Creator>();
    page.set_accent(colour(spec.accent));
    page.set_accent_hex(format!("#{:06x}", spec.accent & 0xff_ffff).into());
    page.set_accent_hue(creator::lch(spec.accent & 0xff_ffff)[2] as f32);
    page.set_tint(if neutral(spec.ground) {
        1
    } else if spec.ground & 0xff_ffff == spec.accent & 0xff_ffff {
        0
    } else {
        2
    });
    page.set_background(match spec.background {
        Background::Dark => 0,
        Background::Light => 1,
        Background::Picture => 2,
    });
    page.set_has_picture(spec.picture.is_some());
    page.set_picture_colours(ModelRc::new(VecModel::from(
        spec.picture
            .iter()
            .flat_map(|picture| picture.palette.iter().take(5))
            .map(|swatch| colour(*swatch))
            .collect::<Vec<_>>(),
    )));
    page.set_blur(i32::from(spec.blur));
    page.set_dim(i32::from(spec.dim));
    page.set_messages(match spec.messages {
        Messages::Flat => 0,
        Messages::Bubbles => 1,
        Messages::Cards => 2,
        Messages::Irc => 3,
    });
    page.set_corners(i32::from(spec.corners));
    let index = |key: &str| {
        creator::FONTS
            .iter()
            .position(|font| font.key == key)
            .or_else(|| {
                spec.fonts
                    .iter()
                    .position(|font| font.file == key)
                    .map(|place| place + creator::FONTS.len())
            })
            .map_or(0, |index| index as i32)
    };
    page.set_body(index(&spec.body));
    page.set_headings(spec.headings.as_deref().map_or(-1, index));
    page.set_density(match spec.density {
        Density::Compact => 0,
        Density::Cozy => 1,
        Density::Roomy => 2,
    });
    page.set_outline(i32::from(spec.outline.min(3)));
    page.set_glass(spec.glass);
    page.set_can_export(true);
    // The preview's picture: the thumbnail blurred and dimmed as the theme
    // will draw it.
    let picture = THUMBNAIL.with_borrow(|thumbnail| {
        let thumbnail = thumbnail.as_ref()?;
        let sigma = f32::from(spec.blur.min(100)) / 100.0 * 48.0 / 4.0;
        let mut shown = if sigma > 0.5 && !spec.glass {
            image::imageops::fast_blur(thumbnail, sigma)
        } else {
            thumbnail.clone()
        };
        let keep = 1.0 - derived.dim;
        for pixel in shown.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = (f32::from(*channel) * keep).round() as u8;
            }
        }
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            shown.as_raw(),
            shown.width(),
            shown.height(),
        );
        Some(Image::from_rgba8(buffer))
    });
    page.set_picture(picture.unwrap_or_default());
}

/// Drops what Undo would go back through.
fn forget(window: &YapfastWindow) {
    HISTORY.with_borrow_mut(Vec::clear);
    LAST.set(None);
    window.global::<Creator>().set_can_undo(false);
}

/// Says how the theme reads: its text's contrast on the conversation's
/// ground, and whether a colour was moved to keep it readable, the accent
/// where it's text or one the person set in Advanced.
fn show_readable(window: &YapfastWindow, derived: &creator::Derived, moved: bool) {
    let page = window.global::<Creator>();
    let (Some(text), Some(chat), Some(accent)) = (
        derived.colour("text"),
        derived.colour("chat"),
        derived.colour("accent"),
    ) else {
        page.set_readable("".into());
        return;
    };
    let opaque = |colour: u32| 0xff00_0000 | colour;
    let ratio = yapfast_core::themes::check::contrast(opaque(text), opaque(chat));
    let mut note = format!("Readable: text has {ratio:.1}:1 contrast on the background.");
    let accent_moved = derived
        .colour("accent-text")
        .is_some_and(|ink| ink & 0xff_ffff != accent & 0xff_ffff);
    if accent_moved {
        note.push_str(if derived.dark {
            " Where the accent is text, it's a shade lighter so it reads too."
        } else {
            " Where the accent is text, it's a shade darker so it reads too."
        });
    }
    if moved {
        note.push_str(" A text colour was adjusted to stay readable.");
    }
    page.set_readable(note.into());
    page.set_readable_moved(accent_moved || moved);
}

/// Offers up to five of the newest pictures in the person's Pictures folder
/// (and its Wallpapers), read on another thread.
fn suggest_pictures(window: &YapfastWindow) {
    SUGGESTED.set(Vec::new());
    window
        .global::<Creator>()
        .set_suggestions(ModelRc::default());
    let Some(pictures) = pictures_folder() else {
        return;
    };
    let weak = window.as_weak();
    let work = move || {
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for folder in [
            pictures.clone(),
            pictures.join("Wallpapers"),
            pictures.join("wallpapers"),
        ] {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let picture = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "jpg" | "jpeg" | "png" | "webp"
                        )
                    });
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                // Whole pictures, not screenshots of a whole drive.
                if picture && metadata.is_file() && metadata.len() <= 40 << 20 {
                    found.push((metadata.modified().unwrap_or(std::time::UNIX_EPOCH), path));
                }
            }
        }
        found.sort_by_key(|item| std::cmp::Reverse(item.0));
        let mut thumbnails = Vec::new();
        for (_, path) in found {
            if thumbnails.len() == 5 {
                break;
            }
            let Ok(reader) =
                image::ImageReader::open(&path).and_then(|reader| reader.with_guessed_format())
            else {
                continue;
            };
            let mut reader = reader;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(256 << 20);
            reader.limits(limits);
            if let Ok(picture) = reader.decode() {
                let small = picture.thumbnail(168, 104).to_rgba8();
                thumbnails.push((path, small));
            }
        }
        let _ = slint::invoke_from_event_loop(move || {
            let Some(window) = weak.upgrade() else { return };
            let images: Vec<Image> = thumbnails
                .iter()
                .map(|(_, small)| {
                    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                        small.as_raw(),
                        small.width(),
                        small.height(),
                    ))
                })
                .collect();
            SUGGESTED.set(thumbnails.into_iter().map(|(path, _)| path).collect());
            window
                .global::<Creator>()
                .set_suggestions(ModelRc::new(VecModel::from(images)));
        });
    };
    if std::thread::Builder::new()
        .name("pictures".into())
        .spawn(work)
        .is_err()
    {
        tracing::warn!("couldn't start a thread to find pictures");
    }
}

/// The person's Pictures folder: on Linux, the one `user-dirs.dirs` names,
/// as the desktop's file manager has it; elsewhere `Pictures` at home.
fn pictures_folder() -> Option<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)?;
    if cfg!(any(windows, target_os = "macos")) {
        return Some(home.join("Pictures"));
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let text = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    let line = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("XDG_PICTURES_DIR="))?;
    let path = line.trim().trim_matches('"');
    let path = match path.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    };
    (path.is_absolute() && path != home).then_some(path)
}

/// Reads a picture on another thread, then hands it over through
/// `Creator.arrived`.
fn read_picture(window: &YapfastWindow, path: PathBuf) {
    window.global::<Creator>().set_reading_picture(true);
    let weak = window.as_weak();
    let work = move || {
        let prepared = std::fs::read(&path)
            .map_err(|_| "That file isn't a picture Yapfast can use".to_owned())
            .and_then(|bytes| creator::prepare_picture(&bytes));
        let _ = slint::invoke_from_event_loop(move || {
            ARRIVED.set(Some(Arrival { prepared }));
            if let Some(window) = weak.upgrade() {
                window.global::<Creator>().invoke_arrived();
            }
        });
    };
    if std::thread::Builder::new()
        .name("picture".into())
        .spawn(work)
        .is_err()
    {
        window.global::<Creator>().set_reading_picture(false);
    }
}

/// Picture…: the system's file dialog for a picture, read once chosen.
fn pick_picture(adapter: &mut Adapter, window: &YapfastWindow) {
    let window = window.as_weak();
    adapter.dialog(async move {
        let picked = rfd::AsyncFileDialog::new()
            .set_title("Choose a picture for your theme")
            .add_filter("Pictures", &["jpg", "jpeg", "png", "webp", "bmp", "gif"])
            .pick_file()
            .await;
        if let (Some(picked), Some(window)) = (picked, window.upgrade()) {
            read_picture(&window, picked.path().to_owned());
        }
    });
}

/// Accent from a place on the rainbow: a lively colour of that hue.
fn accent_from_hue(degrees: f32) -> u32 {
    creator::from_lch([0.72, 0.16, f64::from(degrees)])
}

/// Binds the creator's controls.
pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let page = window.global::<Creator>();
    macro_rules! on {
        ($callback:ident($($arg:ident),*) => |$adapter:ident, $window:ident| $body:expr) => {{
            let (state, weak) = (adapter.clone(), window.as_weak());
            page.$callback(move |$($arg),*| {
                let (Some($window), Ok(mut $adapter)) = (weak.upgrade(), state.try_borrow_mut()) else { return };
                $body;
            });
        }};
    }
    on!(on_start_chosen(index) => |adapter, window| adapter.creator_start(&window, index));
    on!(on_accent_chosen(chosen) => |adapter, window| {
        let accent = rgb(chosen);
        adapter.creator_edit(&window, true, |spec| {
            spec.accent = accent;
            // The grounds follow the accent's hue, unless they're a picture's
            // or the person chose plain greys.
            if spec.background != Background::Picture && !neutral(spec.ground) {
                spec.ground = accent;
            }
        })
    });
    on!(on_hue_chosen(degrees) => |adapter, window| {
        let accent = accent_from_hue(degrees);
        adapter.creator_edit(&window, true, |spec| {
            spec.accent = accent;
            // The grounds follow the accent's hue, unless they're a picture's
            // or the person chose plain greys.
            if spec.background != Background::Picture && !neutral(spec.ground) {
                spec.ground = accent;
            }
        })
    });
    on!(on_hex_typed(text) => |adapter, window| {
        if let Some(chosen) = theme::parse(&text) {
            let accent = rgb(chosen);
            adapter.creator_edit(&window, true, |spec| {
                spec.accent = accent;
                if spec.background != Background::Picture && !neutral(spec.ground) {
                    spec.ground = accent;
                }
            });
        }
    });
    on!(on_background_chosen(index) => |adapter, window| {
        if index == 2 {
            let has_picture = adapter
                .theme_editor
                .as_ref()
                .and_then(|editor| editor.creator.as_ref())
                .is_some_and(|spec| spec.picture.is_some());
            if !has_picture {
                pick_picture(&mut adapter, &window);
                return;
            }
        }
        adapter.creator_edit(&window, true, |spec| {
            spec.background = match index {
                1 => Background::Light,
                2 => Background::Picture,
                _ => Background::Dark,
            };
        })
    });
    on!(on_blur_chosen(value) => |adapter, window| {
        adapter.creator_edit(&window, false, |spec| spec.blur = value.clamp(0, 100) as u8)
    });
    on!(on_dim_chosen(value) => |adapter, window| {
        adapter.creator_edit(&window, false, |spec| spec.dim = value.clamp(0, 100) as u8)
    });
    on!(on_messages_chosen(index) => |adapter, window| {
        adapter.creator_edit(&window, false, |spec| {
            spec.messages = match index {
                1 => Messages::Bubbles,
                2 => Messages::Cards,
                3 => Messages::Irc,
                _ => Messages::Flat,
            };
        })
    });
    on!(on_corners_chosen(value) => |adapter, window| {
        adapter.creator_edit(&window, false, |spec| spec.corners = value.clamp(0, 100) as u8)
    });
    on!(on_font_chosen(index, headings) => |adapter, window| {
        let key = usize::try_from(index).ok().and_then(|index| {
            window.global::<Creator>().get_fonts().row_data(index).map(|font| font.key.to_string())
        });
        adapter.creator_edit(&window, false, |spec| match (headings, key) {
            (true, key) => spec.headings = key,
            (false, Some(key)) => spec.body = key,
            (false, None) => {}
        })
    });
    on!(on_outline_chosen(width) => |adapter, window| {
        adapter.creator_edit(&window, false, |spec| spec.outline = width.clamp(0, 3) as u8)
    });
    on!(on_density_chosen(index) => |adapter, window| {
        adapter.creator_edit(&window, false, |spec| {
            spec.density = match index {
                0 => Density::Compact,
                2 => Density::Roomy,
                _ => Density::Cozy,
            };
        })
    });
    on!(on_glass_toggled(on) => |adapter, window| adapter.creator_edit(&window, false, |spec| spec.glass = on));
    on!(on_tint_chosen(index) => |adapter, window| {
        adapter.creator_edit(&window, true, |spec| {
            spec.ground = if index == 1 { NEUTRAL } else { spec.accent & 0xff_ffff };
        })
    });
    on!(on_undo() => |adapter, window| adapter.creator_undo(&window));
    on!(on_start_over() => |adapter, window| adapter.restart_theme(&window));
    on!(on_open_advanced() => |adapter, window| adapter.creator_advanced(&window));
    let (state, weak) = (adapter.clone(), window.as_weak());
    page.on_export(move || {
        let (Some(window), Ok(mut adapter)) = (weak.upgrade(), state.try_borrow_mut()) else {
            return;
        };
        adapter.creator_export(&window, Rc::downgrade(&state));
    });
    on!(on_ready(id) => |adapter, window| adapter.creator_ready(&window, &id));
    on!(on_arrived() => |adapter, window| adapter.creator_picture_arrived(&window));
    let weak = window.as_weak();
    page.on_suggestion_chosen(move |index| {
        let Some(window) = weak.upgrade() else { return };
        let path = usize::try_from(index)
            .ok()
            .and_then(|index| SUGGESTED.with_borrow(|suggested| suggested.get(index).cloned()));
        if let Some(path) = path {
            read_picture(&window, path);
        }
    });
    let (state, weak) = (adapter.clone(), window.as_weak());
    page.on_choose_picture(move || {
        let (Some(window), Ok(mut adapter)) = (weak.upgrade(), state.try_borrow_mut()) else {
            return;
        };
        pick_picture(&mut adapter, &window);
    });
    let (state, weak) = (adapter.clone(), window.as_weak());
    page.on_import_font(move || {
        let Ok(mut adapter) = state.try_borrow_mut() else {
            return;
        };
        let window = weak.clone();
        let again = Rc::downgrade(&state);
        adapter.dialog(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Import a font")
                .add_filter("Fonts", &["ttf", "otf"])
                .pick_file()
                .await;
            let (Some(picked), Some(window), Some(adapter)) =
                (picked, window.upgrade(), again.upgrade())
            else {
                return;
            };
            adapter
                .borrow_mut()
                .creator_font_imported(&window, picked.path());
        });
    });
}
