//! Settings → Themes' guided creator. The state here mirrors the Slint build's
//! `creator.slint` steps while all derivation, package writing and export
//! stay in `yapfast_core::themes::creator`.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use iced::{
    Alignment, Color, Element, Length, Task,
    advanced::image::Handle,
    widget::{Space, button, column, container, image, row, stack, text},
};
use yapfast_core::{
    theme::{self as palette, Colour, Custom},
    themes::creator::{self as core, Background, Density, Messages, Spec},
};
use yapfast_iced_kit::theme_page as kit;

use super::{Event as SettingsEvent, Page, Section, Settings, appearance};
use crate::{
    app::Message,
    controls::{Kind, action, slider},
    ctx::{Cx, Pref as Shown, Request},
    fonts::{BOLD, MEDIUM},
    form,
    pictures::Icon,
    themes,
};

const HISTORY: usize = 64;
pub(super) const TWO_COLUMNS: f32 = 520.0;
pub(super) const COLOUR_GAP: f32 = 24.0;
static GENERATION: AtomicU64 = AtomicU64::new(1);

/// One creator step, in the Slint build's order. Advanced fine-tunes every derived
/// colour without throwing away the simple choices behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    Start,
    Colours,
    Shape,
    Fonts,
    Advanced,
}

impl Step {
    const ALL: [Self; 5] = [
        Self::Start,
        Self::Colours,
        Self::Shape,
        Self::Fonts,
        Self::Advanced,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Colours => "Colours",
            Self::Shape => "Shape",
            Self::Fonts => "Fonts",
            Self::Advanced => "Advanced",
        }
    }
}

#[derive(Clone)]
struct Snapshot {
    spec: Spec,
    theme: palette::Theme,
    base: &'static str,
    assets: DraftAssets,
}

#[derive(Clone, Default)]
struct DraftAssets {
    picture: Option<Arc<[u8]>>,
    fonts: BTreeMap<String, Arc<[u8]>>,
}

/// The guided creator while open. Its theme is the fully derived palette
/// currently previewed; `fields` are Advanced's editable copies.
pub struct Creator {
    generation: u64,
    editing: Option<String>,
    pub step: Step,
    pub name: String,
    pub spec: Spec,
    pub theme: palette::Theme,
    pub base: &'static str,
    pub accent_hex: String,
    pub fields: Vec<(String, String, bool)>,
    pub note: Option<(String, bool)>,
    /// The theme started from, among `palette::starts`.
    pub start: Option<usize>,
    /// The accent's hue while the hue bar is dragged, in degrees.
    pub hue: Option<f32>,
    /// The font tiles set the headings' font, not the body's.
    pub headings_tab: bool,
    suggestions: Vec<Suggested>,
    confirming: bool,
    assets: DraftAssets,
    initial: Snapshot,
    history: Vec<Snapshot>,
}

/// The useful, cloneable part of a picture prepared off the UI thread.
#[derive(Clone, Debug)]
pub struct PictureArrival {
    jpeg: Vec<u8>,
    picture: core::Picture,
    accent: u32,
    ground: u32,
}

/// A newest picture found in the person's Pictures folders and made small
/// off the UI thread.
#[derive(Clone, Debug)]
pub struct Suggested {
    path: PathBuf,
    /// Made once, in the renderer's own pixels: a handle made in the view
    /// was a new picture to scale and copy on every frame.
    handle: Handle,
}

impl Suggested {
    pub(crate) fn read(path: PathBuf) -> Option<Self> {
        let mut reader = ::image::ImageReader::open(&path)
            .and_then(|reader| reader.with_guessed_format())
            .ok()?;
        let mut limits = ::image::Limits::default();
        limits.max_image_width = Some(12_000);
        limits.max_image_height = Some(12_000);
        limits.max_alloc = Some(256 << 20);
        reader.limits(limits);
        let small = reader.decode().ok()?.thumbnail(168, 104).to_rgba8();
        let (width, height) = small.dimensions();
        let mut pixels = small.into_raw();
        crate::paint::to_native(&mut pixels);
        Some(Self {
            path,
            handle: Handle::from_rgba(width, height, pixels),
        })
    }
}

impl PictureArrival {
    pub(crate) fn prepare(bytes: &[u8]) -> Result<Self, String> {
        let prepared = core::prepare_picture(bytes)?;
        Ok(Self {
            jpeg: prepared.jpeg,
            picture: prepared.picture,
            accent: prepared.accent,
            ground: prepared.ground,
        })
    }
}

impl Creator {
    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    fn new(
        name: String,
        spec: Spec,
        theme: palette::Theme,
        base: &'static str,
        assets: DraftAssets,
        editing: Option<String>,
    ) -> Self {
        let initial = Snapshot {
            spec: spec.clone(),
            theme,
            base,
            assets: assets.clone(),
        };
        let mut creator = Self {
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            editing,
            step: Step::Start,
            name,
            spec,
            theme,
            base,
            accent_hex: String::new(),
            fields: Vec::new(),
            note: None,
            start: None,
            hue: None,
            headings_tab: false,
            suggestions: Vec::new(),
            confirming: false,
            assets,
            initial,
            history: Vec::new(),
        };
        creator.fill();
        creator
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            spec: self.spec.clone(),
            theme: self.theme,
            base: self.base,
            assets: self.assets.clone(),
        }
    }

    fn remember(&mut self) {
        if self.history.len() == HISTORY {
            self.history.remove(0);
        }
        self.history.push(self.snapshot());
        self.note = None;
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.spec = snapshot.spec;
        self.theme = snapshot.theme;
        self.base = snapshot.base;
        self.assets = snapshot.assets;
        self.fill();
        self.note = None;
    }

    fn fill(&mut self) {
        self.accent_hex = format!("#{:06x}", self.spec.accent);
        self.fields = palette::Theme::NAMES
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                Some((
                    palette::token(name),
                    palette::hex(self.theme.colour(index)?),
                    false,
                ))
            })
            .collect();
    }

    fn valid(&self) -> bool {
        self.fields.iter().all(|(_, _, wrong)| !wrong)
    }

    fn derived(&self) -> core::Derived {
        based(
            &self.spec,
            core::finish(&self.spec, colours_of(&self.theme)),
        )
    }

    fn refresh(&mut self, recolour: bool) {
        let derived = if recolour {
            based(&self.spec, core::derive(&self.spec))
        } else {
            self.derived()
        };
        (self.base, self.theme) = palette_of(&derived);
        self.fill();
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Open,
    Step(Step),
    Name(String),
    Start(usize),
    Accent(String),
    AccentPreset(u32),
    /// The hue bar dragged to a hue, in degrees; chosen on release.
    HueDrag(f32),
    HueDone,
    Background(Background),
    PickPicture,
    PicturePicked(u64, Option<PathBuf>),
    PictureReady(u64, Result<PictureArrival, String>),
    Suggestions(u64, Vec<Suggested>),
    Suggested(usize),
    Blur(u8),
    Dim(u8),
    Neutral(bool),
    Messages(Messages),
    Corners(u8),
    Density(Density),
    Outline(u8),
    Glass(bool),
    Body(String),
    Headings(Option<String>),
    PickFont(bool),
    /// The font tiles set the headings' font (true) or the body's.
    FontRole(bool),
    Dark(bool),
    FontPicked(u64, bool, Option<PathBuf>),
    FontReady(u64, bool, Result<Vec<u8>, String>),
    AdvancedColour(usize, String),
    Undo,
    StartOver,
    Save,
    Export,
    ExportTo(u64, Option<PathBuf>),
    Delete,
    Cancel,
}

fn message(event: Event) -> Message {
    crate::app::settings(SettingsEvent::Theme(appearance::Event::Creator(event)))
}

fn pick_picture(generation: u64) -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Choose a picture for your theme")
                .add_filter("Pictures", &["jpg", "jpeg", "png", "webp", "bmp", "gif"])
                .pick_file()
                .await
                .map(|file| file.path().to_owned())
        },
        move |path| message(Event::PicturePicked(generation, path)),
    )
}

fn prepare_picture(generation: u64, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let bytes = std::fs::read(path)
                    .map_err(|_| "That file isn't a picture Yapfast can use".to_owned())?;
                PictureArrival::prepare(&bytes)
            })
            .await
            .unwrap_or_else(|_| Err("That picture couldn't be read".to_owned()))
        },
        move |arrival| message(Event::PictureReady(generation, arrival)),
    )
}

fn newest_pictures() -> Vec<Suggested> {
    let Some(pictures) = yapfast_core::pictures_directory() else {
        return Vec::new();
    };
    pictures_in(&pictures)
}

/// The five newest pictures in `pictures` and its wallpapers folder. On a
/// file system that ignores case, `Wallpapers` and `wallpapers` are one
/// folder, which is looked in once.
fn pictures_in(pictures: &std::path::Path) -> Vec<Suggested> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for folder in [
        pictures.to_path_buf(),
        pictures.join("Wallpapers"),
        pictures.join("wallpapers"),
    ] {
        let Ok(canonical) = folder.canonicalize() else {
            continue;
        };
        if !seen.insert(canonical) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(folder) else {
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
            if picture && metadata.is_file() && metadata.len() <= 40 << 20 {
                found.push((metadata.modified().unwrap_or(std::time::UNIX_EPOCH), path));
            }
        }
    }
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found
        .into_iter()
        .filter_map(|(_, path)| Suggested::read(path))
        .take(5)
        .collect()
}

fn suggest_pictures(generation: u64) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(newest_pictures)
                .await
                .unwrap_or_default()
        },
        move |pictures| message(Event::Suggestions(generation, pictures)),
    )
}

fn pick_font(generation: u64, headings: bool) -> Task<Message> {
    Task::perform(
        async move {
            (
                generation,
                headings,
                rfd::AsyncFileDialog::new()
                    .set_title("Import a font for your theme")
                    .add_filter("Fonts", &["ttf", "otf"])
                    .pick_file()
                    .await
                    .map(|file| file.path().to_owned()),
            )
        },
        |(generation, headings, path)| message(Event::FontPicked(generation, headings, path)),
    )
}

fn font_bytes(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "AtkinsonHyperlegibleNext-Regular.ttf" => {
            include_bytes!(
                "../../../../assets/fonts/atkinson-hyperlegible-next/AtkinsonHyperlegibleNext-Regular.ttf"
            )
        }
        "DisplayName-Tempo.ttf" => {
            include_bytes!("../../../../assets/fonts/display-names/DisplayName-Tempo.ttf")
        }
        "DisplayName-Modern.ttf" => {
            include_bytes!("../../../../assets/fonts/display-names/DisplayName-Modern.ttf")
        }
        "DisplayName-MonkeyBars.ttf" => {
            include_bytes!("../../../../assets/fonts/display-names/DisplayName-MonkeyBars.ttf")
        }
        "DisplayName-Journal.ttf" => {
            include_bytes!("../../../../assets/fonts/display-names/DisplayName-Journal.ttf")
        }
        "DisplayName-8Bit.ttf" => {
            include_bytes!("../../../../assets/fonts/display-names/DisplayName-8Bit.ttf")
        }
        "DisplayName-Mainframe.ttf" => {
            include_bytes!("../../../../assets/fonts/display-names/DisplayName-Mainframe.ttf")
        }
        _ => return None,
    })
}

fn colours_of(theme: &palette::Theme) -> Vec<(&'static str, u32)> {
    core::BASE_TOKENS
        .iter()
        .filter_map(|token| {
            let index = palette::token_index(token)?;
            Some((*token, theme.colour(index)?.argb()))
        })
        .collect()
}

fn based(spec: &Spec, mut derived: core::Derived) -> core::Derived {
    if let Some(base) = spec
        .base
        .as_deref()
        .and_then(palette::find)
        .filter(|base| base.theme.dark == derived.dark)
    {
        derived.base = base.id;
    }
    derived
}

fn palette_of(derived: &core::Derived) -> (&'static str, palette::Theme) {
    let base = palette::find(derived.base).unwrap_or(&palette::BUILT_IN[0]);
    let mut theme = base.theme;
    theme.dark = derived.dark;
    for (token, colour) in &derived.colours {
        if let Some(slot) = palette::token_index(token).and_then(|index| theme.colour_mut(index)) {
            *slot = Colour::from_argb(*colour);
        }
    }
    (base.id, theme)
}

fn spec_from(theme: palette::Theme, base: &str) -> Spec {
    Spec {
        base: Some(base.to_owned()),
        accent: theme.accent.rgb_part(),
        ground: theme.chat.rgb_part(),
        background: if theme.dark {
            Background::Dark
        } else {
            Background::Light
        },
        ..Spec::default()
    }
}

fn creator_relative(id: &str) -> String {
    format!(
        "themes/creator/{}",
        core::slug(id.strip_prefix("custom:").unwrap_or(id))
    )
}

impl Settings {
    pub(super) fn creator_event(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Open => {
                self.open_creator(cx);
                return self
                    .creator
                    .as_ref()
                    .map_or_else(Task::none, |creator| suggest_pictures(creator.generation));
            }
            Event::Step(step) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.step = step;
                    if step == Step::Start && creator.suggestions.is_empty() {
                        return suggest_pictures(creator.generation);
                    }
                }
            }
            Event::Name(name) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.name = name;
                }
            }
            Event::Start(index) => self.creator_start(index, cx),
            Event::Accent(text) => {
                if let Some(accent) = palette::parse(&text) {
                    self.creator_change(cx, true, |spec| {
                        let accent = accent.rgb_part();
                        spec.accent = accent;
                        if spec.background != Background::Picture && !neutral(spec.ground) {
                            spec.ground = accent;
                        }
                    });
                }
            }
            Event::AccentPreset(accent) => self.creator_change(cx, true, |spec| {
                spec.accent = accent;
                if spec.background != Background::Picture && !neutral(spec.ground) {
                    spec.ground = accent;
                }
            }),
            Event::HueDrag(degrees) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.hue = Some(degrees.clamp(0.0, 360.0));
                }
            }
            Event::HueDone => {
                let Some(degrees) = self.creator.as_mut().and_then(|creator| creator.hue.take())
                else {
                    return Task::none();
                };
                let accent = accent_from_hue(degrees);
                self.creator_change(cx, true, |spec| {
                    spec.accent = accent;
                    if spec.background != Background::Picture && !neutral(spec.ground) {
                        spec.ground = accent;
                    }
                });
            }
            Event::FontRole(headings) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.headings_tab = headings;
                }
            }
            Event::Dark(dark) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.remember();
                    creator.theme.dark = dark;
                    creator.fill();
                }
                self.show_creator(cx);
            }
            Event::Background(background) => {
                if background == Background::Picture
                    && self
                        .creator
                        .as_ref()
                        .is_none_or(|creator| creator.spec.picture.is_none())
                {
                    return self
                        .creator
                        .as_ref()
                        .map_or_else(Task::none, |creator| pick_picture(creator.generation));
                }
                self.creator_change(cx, true, |spec| spec.background = background);
            }
            Event::PickPicture => {
                return self
                    .creator
                    .as_ref()
                    .map_or_else(Task::none, |creator| pick_picture(creator.generation));
            }
            Event::PicturePicked(generation, path) => {
                if !self.creator_is(generation) {
                    return Task::none();
                }
                let Some(path) = path else {
                    return Task::none();
                };
                return prepare_picture(generation, path);
            }
            Event::PictureReady(generation, arrival) => {
                if self.creator_is(generation) {
                    self.creator_picture(arrival, cx);
                }
            }
            Event::Suggestions(generation, suggestions) => {
                if let Some(creator) = self
                    .creator
                    .as_mut()
                    .filter(|creator| creator.generation == generation)
                {
                    creator.suggestions = suggestions;
                }
            }
            Event::Suggested(index) => {
                let Some(creator) = self.creator.as_ref() else {
                    return Task::none();
                };
                let Some(suggestion) = creator.suggestions.get(index) else {
                    return Task::none();
                };
                return prepare_picture(creator.generation, suggestion.path.clone());
            }
            Event::Blur(blur) => self.creator_change(cx, false, |spec| spec.blur = blur.min(100)),
            Event::Dim(dim) => self.creator_change(cx, false, |spec| spec.dim = dim.min(100)),
            Event::Neutral(on) => self.creator_change(cx, true, |spec| {
                spec.ground = if on { 0x80_8080 } else { spec.accent };
            }),
            Event::Messages(messages) => {
                self.creator_change(cx, false, |spec| spec.messages = messages);
            }
            Event::Corners(corners) => {
                self.creator_change(cx, false, |spec| spec.corners = corners.min(100));
            }
            Event::Density(density) => {
                self.creator_change(cx, false, |spec| spec.density = density);
            }
            Event::Outline(outline) => {
                self.creator_change(cx, false, |spec| spec.outline = outline.min(4));
            }
            Event::Glass(glass) => self.creator_change(cx, false, |spec| spec.glass = glass),
            Event::Body(body) => self.creator_change(cx, false, |spec| spec.body = body),
            Event::Headings(headings) => {
                self.creator_change(cx, false, |spec| spec.headings = headings);
            }
            Event::PickFont(headings) => {
                return self.creator.as_ref().map_or_else(Task::none, |creator| {
                    pick_font(creator.generation, headings)
                });
            }
            Event::FontPicked(generation, headings, path) => {
                if !self.creator_is(generation) {
                    return Task::none();
                }
                let Some(path) = path else {
                    return Task::none();
                };
                return Task::perform(
                    async move { std::fs::read(path).map_err(|_| core::NOT_A_FONT.to_owned()) },
                    move |bytes| message(Event::FontReady(generation, headings, bytes)),
                );
            }
            Event::FontReady(generation, headings, bytes) => {
                if self.creator_is(generation) {
                    self.creator_font(headings, bytes, cx);
                }
            }
            Event::AdvancedColour(index, text) => self.creator_colour(index, text, cx),
            Event::Undo => {
                let restored = self
                    .creator
                    .as_mut()
                    .and_then(|creator| creator.history.pop().map(|before| (creator, before)));
                if let Some((creator, before)) = restored {
                    creator.restore(before);
                    if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
                        creator.note =
                            Some((format!("The draft couldn't be restored: {error}"), true));
                    }
                    self.show_creator(cx);
                }
            }
            Event::StartOver => {
                if let Some(creator) = self.creator.as_mut() {
                    let initial = creator.initial.clone();
                    creator.remember();
                    creator.restore(initial);
                    if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
                        creator.note =
                            Some((format!("The draft couldn't be restored: {error}"), true));
                    }
                    self.show_creator(cx);
                }
            }
            Event::Save => self.save_creator(cx),
            Event::Export => {
                if !self.prepare_creator_draft(cx) {
                    return Task::none();
                }
                let generation = self
                    .creator
                    .as_ref()
                    .map(|creator| creator.generation)
                    .unwrap_or_default();
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Export the theme as a package")
                            .pick_folder()
                            .await
                            .map(|folder| folder.path().to_owned())
                    },
                    move |path| message(Event::ExportTo(generation, path)),
                );
            }
            Event::ExportTo(generation, path) => {
                if self.creator_is(generation) && path.is_some() && self.prepare_creator_draft(cx) {
                    self.export_creator(path, cx);
                }
            }
            Event::Delete => self.delete_creator(cx),
            Event::Cancel => self.close_creator(cx),
        }
        Task::none()
    }

    fn open_creator(&mut self, cx: &mut Cx<'_>) {
        self.close_editor(cx);
        let chosen = cx.themes.chosen_id();
        let (base, theme, name, source) = if let Some(built_in) = palette::find(chosen) {
            (
                built_in.id,
                built_in.theme,
                format!("{} copy", built_in.name),
                None,
            )
        } else if let Some(custom) = cx.themes.custom.iter().find(|custom| custom.id == chosen) {
            (
                custom.base,
                custom.theme,
                format!("{} copy", custom.name),
                custom.folder.clone(),
            )
        } else {
            let built_in = &palette::BUILT_IN[0];
            (
                built_in.id,
                built_in.theme,
                format!("{} copy", built_in.name),
                None,
            )
        };
        let mut spec = cx
            .themes
            .custom
            .iter()
            .find(|custom| custom.id == chosen)
            .and_then(|custom| custom.creator.clone())
            .unwrap_or_else(|| spec_from(theme, base));
        let assets = Self::source_assets(cx.client.root(), source.as_deref(), &mut spec);
        let start = palette::starts(&cx.themes.custom).position(|start| start.id == chosen);
        self.creator = Some(Creator::new(name, spec, theme, base, assets, None));
        if let Some(creator) = self.creator.as_mut() {
            creator.start = start;
        }
        if let Some(creator) = self.creator.as_mut()
            && let Err(error) = Self::sync_draft(cx.client.root(), creator)
        {
            creator.note = Some((format!("The draft couldn't be opened: {error}"), true));
        }
        self.section = Some(Section::Themes);
        self.show_creator(cx);
    }

    pub(super) fn edit_creator(&mut self, id: &str, cx: &mut Cx<'_>) -> bool {
        let Some(custom) = cx
            .themes
            .custom
            .iter()
            .find(|custom| custom.id == id && custom.creator.is_some())
            .cloned()
        else {
            return false;
        };
        self.close_editor(cx);
        let mut spec = custom.creator.clone().expect("checked above");
        let assets = Self::source_assets(cx.client.root(), custom.folder.as_deref(), &mut spec);
        self.creator = Some(Creator::new(
            custom.name,
            spec,
            custom.theme,
            custom.base,
            assets,
            Some(custom.id),
        ));
        if let Some(creator) = self.creator.as_mut()
            && let Err(error) = Self::sync_draft(cx.client.root(), creator)
        {
            creator.note = Some((format!("The draft couldn't be opened: {error}"), true));
        }
        self.section = Some(Section::Themes);
        self.show_creator(cx);
        true
    }

    fn creator_start(&mut self, index: usize, cx: &mut Cx<'_>) {
        let Some(start) = palette::starts(&cx.themes.custom).nth(index) else {
            return;
        };
        let custom = cx.themes.custom.iter().find(|custom| custom.id == start.id);
        let mut spec = custom
            .and_then(|custom| custom.creator.clone())
            .unwrap_or_else(|| spec_from(start.theme, start.base));
        let source = custom.and_then(|custom| custom.folder.as_deref());
        let assets = Self::source_assets(cx.client.root(), source, &mut spec);
        let (name, theme, base) = (format!("{} copy", start.name), start.theme, start.base);
        if let Some(creator) = self.creator.as_mut() {
            creator.remember();
            creator.start = Some(index);
            creator.name = name;
            creator.spec = spec;
            creator.theme = theme;
            creator.base = base;
            creator.assets = assets;
            creator.fill();
            if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
                creator.note = Some((format!("The draft couldn't be opened: {error}"), true));
            }
        }
        self.show_creator(cx);
    }

    fn creator_change(&mut self, cx: &mut Cx<'_>, recolour: bool, change: impl FnOnce(&mut Spec)) {
        let Some(creator) = self.creator.as_mut() else {
            return;
        };
        creator.remember();
        change(&mut creator.spec);
        creator.refresh(recolour);
        self.show_creator(cx);
    }

    fn creator_colour(&mut self, index: usize, text: String, cx: &mut Cx<'_>) {
        let Some(creator) = self.creator.as_mut() else {
            return;
        };
        let parsed = palette::parse(&text);
        let Some(field) = creator.fields.get_mut(index) else {
            return;
        };
        (field.1, field.2) = (text, parsed.is_none());
        if let (Some(colour), Some(slot)) = (parsed, creator.theme.colour_mut(index))
            && *slot != colour
        {
            creator.remember();
            if let Some(slot) = creator.theme.colour_mut(index) {
                *slot = colour;
            }
            creator.fill();
            self.show_creator(cx);
        }
    }

    fn show_creator(&mut self, cx: &mut Cx<'_>) {
        let Some(creator) = self.creator.as_ref() else {
            return;
        };
        let fallback = themes::theme(themes::CREATOR_PREVIEW_ID, &creator.theme);
        let folder = Self::draft_folder(cx);
        let name = if creator.name.trim().is_empty() {
            "My theme"
        } else {
            creator.name.trim()
        };
        let derived = creator.derived();
        let ready = Self::sync_draft(cx.client.root(), creator)
            .map_err(|error| error.to_string())
            .and_then(|()| {
                core::write_folder(
                    &folder,
                    &creator.spec,
                    name,
                    themes::CREATOR_PREVIEW_ID,
                    &derived,
                    font_bytes,
                )
                .map_err(|error| error.to_string())
            });
        self.preview = Some(
            match ready.and_then(|()| cx.themes.preview_creator(&folder, cx.layout)) {
                Ok(preview) => preview,
                Err(error) => {
                    cx.themes.clear_creator_preview();
                    if let Some(creator) = self.creator.as_mut() {
                        creator.note =
                            Some((format!("The preview couldn't be shown: {error}"), true));
                    }
                    fallback
                }
            },
        );
        cx.request(Request::Changed(Shown::Theme));
    }

    pub(super) fn close_creator(&mut self, cx: &mut Cx<'_>) {
        if self.creator.take().is_some() {
            cx.themes.clear_creator_preview();
            std::fs::remove_dir_all(Self::draft_folder(cx)).ok();
            if self.preview.take().is_some() {
                cx.request(Request::Changed(Shown::Theme));
            }
        }
    }

    fn creator_is(&self, generation: u64) -> bool {
        self.creator
            .as_ref()
            .is_some_and(|creator| creator.generation == generation)
    }

    fn source_assets(root: &Path, source: Option<&str>, spec: &mut Spec) -> DraftAssets {
        let mut assets = DraftAssets::default();
        let source = source.map(|source| root.join(source));
        if spec.picture.is_some() {
            assets.picture = source
                .as_ref()
                .and_then(|source| std::fs::read(source.join(core::PICTURE)).ok())
                .map(Arc::from);
            if assets.picture.is_none() {
                spec.picture = None;
                if spec.background == Background::Picture {
                    spec.background = Background::Dark;
                }
            }
        }
        spec.fonts.retain(|font| {
            let safe = Path::new(&font.file)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
                && font.file.starts_with("fonts/");
            let bytes = safe
                .then_some(source.as_ref())
                .flatten()
                .and_then(|source| std::fs::read(source.join(&font.file)).ok());
            if let Some(bytes) = bytes {
                assets.fonts.insert(font.file.clone(), Arc::from(bytes));
                true
            } else {
                false
            }
        });
        let built_in = |key: &str| core::FONTS.iter().any(|font| font.key == key);
        if !built_in(&spec.body) && !assets.fonts.contains_key(&spec.body) {
            "default".clone_into(&mut spec.body);
        }
        if spec
            .headings
            .as_deref()
            .is_some_and(|key| !built_in(key) && !assets.fonts.contains_key(key))
        {
            spec.headings = None;
        }
        assets
    }

    fn sync_draft(root: &Path, creator: &Creator) -> std::io::Result<()> {
        let draft = root.join("themes/creator/draft");
        match std::fs::remove_dir_all(&draft) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Some(picture) = creator.assets.picture.as_deref() {
            core::store_picture(&draft, picture)?;
        }
        for (file, bytes) in &creator.assets.fonts {
            let path = draft.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
        }
        Ok(())
    }

    fn creator_picture(&mut self, arrival: Result<PictureArrival, String>, cx: &mut Cx<'_>) {
        let arrival = match arrival {
            Ok(arrival) => arrival,
            Err(reason) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.note = Some((reason, true));
                }
                return;
            }
        };
        let Some(creator) = self.creator.as_mut() else {
            return;
        };
        let first = creator.spec.picture.is_none();
        creator.remember();
        creator.assets.picture = Some(Arc::from(arrival.jpeg));
        creator.spec.picture = Some(arrival.picture);
        creator.spec.background = Background::Picture;
        creator.spec.accent = arrival.accent;
        creator.spec.ground = arrival.ground;
        if first {
            creator.spec.glass = true;
        }
        creator.refresh(true);
        creator.note = Some(("Added the picture and used its colours.".to_owned(), false));
        if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
            creator.note = Some((format!("The picture couldn't be kept: {error}"), true));
        }
        self.show_creator(cx);
    }

    fn creator_font(&mut self, headings: bool, bytes: Result<Vec<u8>, String>, cx: &mut Cx<'_>) {
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(reason) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.note = Some((reason, true));
                }
                return;
            }
        };
        let imported = match core::store_font(&Self::draft_folder(cx), &bytes) {
            Ok(imported) => imported,
            Err(reason) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.note = Some((reason, true));
                }
                return;
            }
        };
        let mut note = format!("Added {}.", imported.family);
        if imported.partial {
            note.push_str(" Some characters will use the default font.");
        }
        note.push_str(" Only share themes with fonts you're allowed to share.");
        let Some(creator) = self.creator.as_mut() else {
            return;
        };
        creator.remember();
        creator
            .assets
            .fonts
            .insert(imported.file.clone(), Arc::from(bytes));
        let file = imported.file.clone();
        creator.spec.fonts.retain(|font| font.file != imported.file);
        creator.spec.fonts.push(imported);
        if headings {
            creator.spec.headings = Some(file);
        } else {
            creator.spec.body = file;
        }
        creator.refresh(false);
        creator.note = Some((note, false));
        if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
            creator.note = Some((format!("The font couldn't be kept: {error}"), true));
        }
        self.show_creator(cx);
    }

    fn save_creator(&mut self, cx: &mut Cx<'_>) {
        let Some(creator) = self.creator.as_ref() else {
            return;
        };
        if !creator.valid() {
            return;
        }
        let editing = creator.editing.clone();
        let id = editing
            .clone()
            .unwrap_or_else(|| palette::new_id(&cx.themes.custom));
        // A stored Custom.folder can come from older layout data and is not
        // authority to overwrite that path. Creator saves always land in the
        // managed tree derived from the validated custom id.
        let relative = creator_relative(&id);
        let folder = cx.client.root().join(&relative);
        let name = if creator.name.trim().is_empty() {
            "My theme".to_owned()
        } else {
            creator.name.trim().to_owned()
        };
        let spec = creator.spec.clone();
        let derived = creator.derived();
        let (base, theme) = palette_of(&derived);
        let draft = Self::draft_folder(cx);
        if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
            if let Some(creator) = self.creator.as_mut() {
                creator.note = Some((format!("The theme couldn't be saved: {error}"), true));
            }
            return;
        }
        let written = core::write_folder(&draft, &spec, &name, &id, &derived, font_bytes)
            .and_then(|()| core::copy_folder(&draft, &folder));
        if let Err(error) = written {
            if let Some(creator) = self.creator.as_mut() {
                creator.note = Some((format!("The theme couldn't be saved: {error}"), true));
            }
            return;
        }
        let custom = Custom {
            id: id.clone(),
            name,
            base,
            theme,
            parts: Default::default(),
            creator: Some(spec),
            folder: Some(relative),
        };
        if let Some(saved) = editing
            .as_ref()
            .and_then(|id| cx.themes.custom.iter_mut().find(|custom| custom.id == *id))
        {
            *saved = custom;
        } else {
            cx.themes.custom.push(custom);
        }
        cx.layout.custom_themes.clone_from(&cx.themes.custom);
        cx.keep("custom_themes", &cx.themes.custom);
        cx.themes.refresh(cx.client.root(), cx.layout);
        cx.themes.clear_creator_preview();
        self.creator = None;
        self.preview = None;
        std::fs::remove_dir_all(draft).ok();
        cx.themes.choose(cx.client.root(), &id);
        cx.request(Request::Changed(Shown::Theme));
    }

    fn delete_creator(&mut self, cx: &mut Cx<'_>) {
        let Some(creator) = self.creator.as_mut() else {
            return;
        };
        let Some(id) = creator.editing.clone() else {
            return;
        };
        if !creator.confirming {
            creator.confirming = true;
            return;
        }
        let folder = cx
            .themes
            .custom
            .iter()
            .find(|custom| custom.id == id)
            .and_then(|custom| custom.folder.as_deref());
        if let Some(folder) = folder
            && let Err(error) = Self::remove_creator_folder(cx.client.root(), folder)
        {
            creator.note = Some((format!("The theme couldn't be deleted: {error}"), true));
            creator.confirming = false;
            return;
        }
        let changed = cx.themes.forget_custom(cx.client.root(), &id);
        cx.layout.custom_themes.clone_from(&cx.themes.custom);
        cx.keep("custom_themes", &cx.themes.custom);
        self.close_creator(cx);
        if changed {
            cx.request(Request::Changed(Shown::Theme));
        }
    }

    fn remove_creator_folder(root: &Path, named: &str) -> std::io::Result<()> {
        let owned = root.join("themes/creator");
        let target = root.join(named);
        let owned = std::fs::canonicalize(owned)?;
        let target = match std::fs::canonicalize(&target) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if target == owned || target.strip_prefix(&owned).is_err() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "its folder is not owned by the theme creator",
            ));
        }
        std::fs::remove_dir_all(target)
    }

    fn draft_folder(cx: &Cx<'_>) -> PathBuf {
        cx.client.root().join("themes/creator/draft")
    }

    fn prepare_creator_draft(&mut self, cx: &Cx<'_>) -> bool {
        let Some(creator) = self.creator.as_ref() else {
            return false;
        };
        let name = creator.name.trim();
        let derived = creator.derived();
        if let Err(error) = Self::sync_draft(cx.client.root(), creator) {
            if let Some(creator) = self.creator.as_mut() {
                creator.note = Some((format!("The theme couldn't be exported: {error}"), true));
            }
            return false;
        }
        match core::write_folder(
            &Self::draft_folder(cx),
            &creator.spec,
            name,
            "draft",
            &derived,
            font_bytes,
        ) {
            Ok(()) => true,
            Err(error) => {
                if let Some(creator) = self.creator.as_mut() {
                    creator.note = Some((format!("The theme couldn't be exported: {error}"), true));
                }
                false
            }
        }
    }

    fn export_creator(&mut self, parent: Option<PathBuf>, cx: &Cx<'_>) {
        let Some(parent) = parent else {
            return;
        };
        let Some(creator) = self.creator.as_ref() else {
            return;
        };
        let folder_name = core::slug(if creator.name.trim().is_empty() {
            "my theme"
        } else {
            creator.name.trim()
        });
        let target = parent.join(folder_name);
        let result = core::export_folder(&Self::draft_folder(cx), &target);
        if let Some(creator) = self.creator.as_mut() {
            creator.note = Some(match result {
                Ok(()) => (
                    format!("Exported the package to {}.", target.display()),
                    false,
                ),
                Err(error) => (format!("The theme couldn't be exported: {error}"), true),
            });
        }
    }
}

fn neutral(ground: u32) -> bool {
    core::lch(ground)[1] < 0.004
}

/// The window's width at which the preview stays beside the controls, as
/// the room the frame gives them; and the preview's column there.
const BESIDE: f32 = 722.0;
const ASIDE: f32 = 352.0;
const ASIDE_WIDE: f32 = 420.0;
const ASIDE_FROM: f32 = 1100.0;
/// The creator's sections, and the parts of one.
const SECTIONS: f32 = 24.0;

/// The accent a place on the hue bar gives: a lively colour of that hue.
pub(crate) fn accent_from_hue(degrees: f32) -> u32 {
    core::from_lch([0.72, 0.16, f64::from(degrees)])
}

fn rgb(colour: u32) -> Color {
    Color::from_rgb8((colour >> 16) as u8, (colour >> 8) as u8, colour as u8)
}

/// The iced face of a font the creator offers, by its key: a built-in's or
/// an imported file's, read into the font system once.
fn face(creator: &Creator, key: &str) -> iced::Font {
    use std::{collections::HashMap, sync::Mutex};
    static FACES: Mutex<Option<HashMap<String, iced::Font>>> = Mutex::new(None);
    let mut faces = FACES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let faces = faces.get_or_insert_with(HashMap::new);
    let imported = creator.assets.fonts.get(key);
    let memo = match imported {
        Some(bytes) => format!("{key}:{}", bytes.len()),
        None => key.to_owned(),
    };
    if let Some(font) = faces.get(&memo) {
        return *font;
    }
    let bytes: Option<Vec<u8>> = match imported {
        Some(bytes) => Some(bytes.to_vec()),
        None => core::FONTS
            .iter()
            .find(|font| font.key == key)
            .and_then(|font| font.file)
            .and_then(font_bytes)
            .map(<[u8]>::to_vec),
    };
    let font = bytes
        .and_then(|bytes| crate::fonts::load_theme_font(bytes).ok())
        .map_or(crate::fonts::REGULAR, |loaded| loaded.font);
    faces.insert(memo, font);
    font
}

/// The picture started from, for the tiles and the preview, decoded once.
fn picture(creator: &Creator) -> Option<Handle> {
    use std::sync::Mutex;
    static SHOWN: Mutex<Option<(usize, Handle)>> = Mutex::new(None);
    let bytes = creator.assets.picture.as_ref()?;
    let key = bytes.as_ptr() as usize ^ bytes.len();
    let mut shown = SHOWN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, handle)) = shown.as_ref()
        && *at == key
    {
        return Some(handle.clone());
    }
    let handle = Handle::from_bytes(bytes.to_vec());
    *shown = Some((key, handle.clone()));
    Some(handle)
}

/// A section of a step: its heading, the controls, and a word on it.
fn section<'a>(
    theme: &crate::theme::Theme,
    title: &'a str,
    controls: Vec<Element<'a, Message>>,
    hint: Option<String>,
) -> Element<'a, Message> {
    let mut parts = vec![form::heading(theme, title)];
    parts.extend(controls);
    if let Some(hint) = hint.filter(|hint| !hint.is_empty()) {
        parts.push(form::note(theme, hint));
    }
    column(parts).spacing(8).width(Length::Fill).into()
}

/// Tiles in as many columns as fit at `least` wide, `gap` apart.
fn grid<'a>(tiles: Vec<Element<'a, Message>>, columns: usize, gap: f32) -> Element<'a, Message> {
    let columns = columns.max(1);
    let mut rows = column![].spacing(gap);
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        let mut line = row![].spacing(gap);
        for _ in 0..columns {
            line = line.push(match tiles.next() {
                Some(tile) => container(tile).width(Length::FillPortion(1)),
                None => container(Space::new()).width(Length::FillPortion(1)),
            });
        }
        rows = rows.push(line);
    }
    rows.into()
}

/// A tile's art on the conversation's ground.
fn ground<'a>(chat: Color, inner: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| crate::style::fill(chat))
        .into()
}

/// How many `least` wide tiles fit in `width`, `gap` apart.
fn fits(width: f32, least: f32, gap: f32) -> usize {
    (((width + gap) / (least + gap)).floor() as usize).max(1)
}

/// A flat box of a colour, round by `radius`.
fn blob<'a>(
    width: impl Into<Length>,
    height: impl Into<Length>,
    colour: Color,
    radius: f32,
) -> Element<'a, Message> {
    container(Space::new())
        .width(width)
        .height(height)
        .style(move |_| crate::style::rounded(colour, radius))
        .into()
}

impl Creator {
    /// A radius at the creator's corners: stock at 50, square at 0,
    /// `round` at 100.
    fn radius(&self, stock: f32, round: f32) -> f32 {
        let corners = f32::from(self.spec.corners.min(100));
        if corners <= 50.0 {
            stock * corners / 50.0
        } else {
            stock + (round - stock) * (corners - 50.0) / 50.0
        }
    }

    fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    /// The text's contrast on the conversation, said gently, and whether a
    /// colour was moved to keep it readable.
    fn readable(&self) -> Option<(String, bool)> {
        let derived = self.derived();
        let (text, chat, accent) = (
            derived.colour("text")?,
            derived.colour("chat")?,
            derived.colour("accent")?,
        );
        let opaque = |colour: u32| 0xff00_0000 | colour;
        let ratio = yapfast_core::themes::check::contrast(opaque(text), opaque(chat));
        let mut note = format!("Readable: text has {ratio:.1}:1 contrast on the background.");
        let moved = derived
            .colour("accent-text")
            .is_some_and(|ink| ink & 0xff_ffff != accent & 0xff_ffff);
        if moved {
            note.push_str(if derived.dark {
                " Where the accent is text, it's a shade lighter so it reads too."
            } else {
                " Where the accent is text, it's a shade darker so it reads too."
            });
        }
        Some((note, moved))
    }
}

impl<'a> Page<'a> {
    /// The creator: the tabs, a bar with the name, Undo, Start over, Cancel
    /// and Save, the steps, and under them the step's controls scrolling
    /// beside a preview that stays in sight.
    pub(super) fn creator_view(self, creator: &'a Creator) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let steps = crate::controls::choices_within(
            theme,
            &Step::ALL.map(Step::name),
            creator.step.index(),
            520.0,
            |index| message(Event::Step(Step::ALL[index])),
        );
        let bar = self.editor_bar(
            &creator.name,
            |name| message(Event::Name(name)),
            Some(creator.can_undo()),
            creator.editing.is_some(),
            creator.valid().then(|| message(Event::Save)),
            message(Event::Cancel),
        );
        let controls = move |_: f32| -> Element<'a, Message> {
            match creator.step {
                Step::Start => self.creator_start_view(creator),
                Step::Colours => self.creator_colours_view(creator),
                Step::Shape => self.creator_shape_view(creator),
                Step::Fonts => self.creator_fonts_view(creator),
                Step::Advanced => self.creator_advanced_view(creator),
            }
        };
        let aside = move |width: f32| -> Element<'a, Message> {
            let mut parts = vec![
                self.creator_preview(creator, width),
                form::note(
                    theme,
                    "Yapfast itself wears the theme while you make it, behind these settings.",
                ),
            ];
            if let Some((note, moved)) = creator.readable() {
                parts.push(self.readable_line(note, moved));
            }
            if let Some((note, true)) = &creator.note {
                parts.push(form::note_in(note.clone(), theme.danger_text));
            }
            let mut buttons = row![
                action(theme, "Export", Kind::Plain, Some(message(Event::Export)))
                    .small()
                    .icon(
                        self.cx
                            .look
                            .icon(Icon::Download, 16, crate::theme::rgb(theme.text))
                    )
            ]
            .spacing(8);
            if creator.editing.is_some() {
                buttons = buttons.push(
                    action(
                        theme,
                        if creator.confirming {
                            "Delete it for good"
                        } else {
                            "Delete theme"
                        },
                        Kind::Danger,
                        Some(message(Event::Delete)),
                    )
                    .small()
                    .icon(self.cx.look.icon(
                        Icon::Trash,
                        16,
                        crate::theme::rgb(theme.on_danger),
                    )),
                );
            }
            parts.push(buttons.into());
            column(parts).spacing(12).into()
        };
        self.editor_frame(bar, Some(steps), controls, aside)
    }

    /// The editor's bar: the name, Undo and Start over (for a theme with
    /// steps), Cancel, and Save.
    pub(super) fn editor_bar(
        self,
        name: &'a str,
        on_name: impl Fn(String) -> Message + 'a,
        undo: Option<bool>,
        saved: bool,
        save: Option<Message>,
        cancel: Message,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let look = self.cx.look;
        let mut bar = row![
            form::field(theme, "creator-name", "Name your theme", name)
                .on_input(on_name)
                .on_submit_maybe(save.clone())
                .padding([8, 12])
                .width(Length::Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if let Some(can_undo) = undo {
            let ink = crate::theme::rgb(if can_undo {
                theme.secondary
            } else {
                theme.faint
            });
            bar = bar
                .push(
                    action(
                        theme,
                        "Undo",
                        Kind::Ghost,
                        can_undo.then(|| message(Event::Undo)),
                    )
                    .icon(look.icon(Icon::Reply, 18, ink)),
                )
                .push(
                    action(
                        theme,
                        "Start over",
                        Kind::Ghost,
                        can_undo.then(|| message(Event::StartOver)),
                    )
                    .icon(look.icon(Icon::Refresh, 18, ink)),
                );
        }
        bar.push(action(theme, "Cancel", Kind::Plain, Some(cancel)))
            .push(action(
                theme,
                if saved { "Save" } else { "Save theme" },
                Kind::Primary,
                save,
            ))
            .into()
    }

    /// What every editor is laid out in: the page's tabs, its bar, the
    /// steps, and the controls scrolling beside (or under, where the frame
    /// is narrow) what `aside` shows at the width it's given.
    pub(super) fn editor_frame(
        self,
        bar: Element<'a, Message>,
        steps: Option<Element<'a, Message>>,
        controls: impl Fn(f32) -> Element<'a, Message> + 'a,
        aside: impl Fn(f32) -> Element<'a, Message> + 'a,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let body = iced::widget::responsive(move |size| {
            let beside = size.width >= BESIDE;
            let wide = if size.width >= ASIDE_FROM {
                ASIDE_WIDE
            } else {
                ASIDE
            };
            if beside {
                let room = size.width - wide - SECTIONS - 12.0;
                let panel =
                    crate::widgets::scroll_view(container(controls(room)).padding(iced::Padding {
                        top: 6.0,
                        right: 12.0,
                        bottom: 24.0,
                        left: 6.0,
                    }))
                    .style(crate::style::scrollbar(theme))
                    .width(Length::Fill)
                    .height(Length::Fill);
                row![
                    panel,
                    container(aside(wide)).width(wide).padding(iced::Padding {
                        top: 6.0,
                        ..iced::Padding::ZERO
                    })
                ]
                .spacing(SECTIONS - 12.0)
                .into()
            } else {
                let room = size.width - 12.0;
                crate::widgets::scroll_view(
                    container(column![aside(room.min(560.0)), controls(room)].spacing(SECTIONS))
                        .padding(iced::Padding {
                            top: 6.0,
                            right: 12.0,
                            bottom: 24.0,
                            left: 6.0,
                        }),
                )
                .style(crate::style::scrollbar(theme))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        });
        let mut head = column![self.theme_tabs(), bar].spacing(16);
        if let Some(steps) = steps {
            head = head.push(steps);
        }
        column![head, body]
            .spacing(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// "Readable: text has 14.6:1 contrast…" with a tick, in the accent's
    /// ink when a colour was moved to keep it so.
    fn readable_line(self, note: String, moved: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        row![
            container(self.cx.look.glyph(
                Icon::Check,
                16,
                if moved {
                    theme.accent_text()
                } else {
                    theme.live
                }
            ))
            .padding(iced::Padding {
                top: 1.0,
                ..iced::Padding::ZERO
            }),
            text(note).size(13).color(theme.muted).width(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    /// A small Yapfast window in the theme being made: its servers, the
    /// channels and a conversation in the chosen message style, in the
    /// colours, corners, fonts and glass the window shows now.
    fn creator_preview(self, creator: &'a Creator, width: f32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let spec = &creator.spec;
        let height = (width * 0.75).max(236.0);
        let glass = spec.glass;
        let has_picture =
            spec.background == Background::Picture && creator.assets.picture.is_some();
        let gap = if glass { 6.0 } else { 0.0 };
        let pane = if glass { 0.45 } else { 1.0 };
        let row_height: f32 = match spec.density {
            Density::Compact => 26.0,
            Density::Roomy => 40.0,
            _ => 32.0,
        };
        let body = face(creator, &spec.body);
        let heading = face(creator, spec.headings.as_deref().unwrap_or(&spec.body));
        let bold = |font: iced::Font| iced::Font {
            weight: iced::font::Weight::Bold,
            ..font
        };
        let words = |words: &'a str, size: f32, font: iced::Font, ink: Color| {
            text(words)
                .size(size)
                .font(font)
                .color(ink)
                .wrapping(text::Wrapping::None)
        };
        // The servers.
        let mut rail = column![].spacing(6).padding([10, 8]);
        for index in 0..4 {
            let (ground, radius) = if index == 0 {
                (theme.accent, creator.radius(8.0, 12.0))
            } else {
                (
                    crate::style::faded(theme.raised, if glass { 0.6 } else { 1.0 }),
                    creator.radius(12.0, 12.0),
                )
            };
            rail = rail.push(blob(24.0, 24.0, ground, radius));
        }
        let pill = theme.text;
        let rail = stack![
            container(rail).width(40).height(Length::Fill),
            container(blob(3.0, 16.0, pill, 1.5)).padding(iced::Padding {
                top: 14.0,
                ..iced::Padding::ZERO
            })
        ];
        // The channels.
        let mut channels = column![
            words("Night Owls", 13.0, bold(heading), theme.text),
            Space::new().height(6)
        ]
        .spacing(2)
        .padding(10);
        for (index, name) in ["# general", "# ideas", "# lounge", "# weekend"]
            .into_iter()
            .enumerate()
        {
            let open = index == 0;
            let active = theme.active;
            let radius = creator.radius(4.0, 12.0).min((row_height - 8.0) / 2.0);
            channels = channels.push(
                container(words(
                    name,
                    12.0,
                    if open {
                        iced::Font {
                            weight: iced::font::Weight::Semibold,
                            ..body
                        }
                    } else {
                        body
                    },
                    if open { theme.text } else { theme.muted },
                ))
                .padding([0, 8])
                .width(Length::Fill)
                .height(row_height - 8.0)
                .align_y(Alignment::Center)
                .style(move |_| {
                    if open {
                        crate::style::rounded(active, radius)
                    } else {
                        container::Style::default()
                    }
                }),
            );
        }
        let side_ground = crate::style::faded(theme.sidebar, pane);
        let side_radius = if glass {
            creator.radius(8.0, 14.0)
        } else {
            0.0
        };
        let side = container(channels)
            .width((width * 0.3).min(132.0))
            .height(Length::Fill)
            .clip(true)
            .style(move |_| crate::style::rounded(side_ground, side_radius));
        // The conversation.
        let messages: [(&str, &str, &str); 3] = [
            ("Bob", "sunset at eight, roof?", "19:58"),
            ("Carol", "yes! I'll bring snacks", "19:59"),
            ("You", "on my way", "20:01"),
        ];
        let spacing = |compact: f32, cozy: f32, roomy: f32| match spec.density {
            Density::Compact => compact,
            Density::Roomy => roomy,
            _ => cozy,
        };
        let name_ink = |index: usize| {
            if index == 2 {
                theme.accent_text()
            } else {
                theme.text
            }
        };
        let conversation: Element<'a, Message> = match spec.messages {
            Messages::Bubbles => {
                let mut list = column![].spacing(spacing(6.0, 9.0, 12.0)).padding([0, 12]);
                for (index, (_, said, _)) in messages.into_iter().enumerate() {
                    let mine = index == 2;
                    // Others' bubbles a shade off the raised ground, so they
                    // show on a light conversation too.
                    let (ground, ink) = if mine {
                        (theme.accent, theme.on_accent)
                    } else {
                        (
                            crate::style::blend(theme.raised, theme.wash(0.07)),
                            theme.text,
                        )
                    };
                    let radius = creator.radius(14.0, 18.0);
                    let bubble = container(words(said, 12.0, body, ink))
                        .padding([6, 10])
                        .style(move |_| crate::style::rounded(ground, radius));
                    list = list.push(if mine {
                        Element::from(row![Space::new().width(Length::Fill), bubble])
                    } else {
                        bubble.into()
                    });
                }
                list.into()
            }
            Messages::Cards => {
                let mut list = column![].spacing(spacing(5.0, 7.0, 10.0)).padding([0, 10]);
                for (index, (name, said, _)) in messages.into_iter().enumerate() {
                    let (ground, radius) = (theme.raised, creator.radius(12.0, 22.0));
                    list = list.push(
                        container(
                            row![
                                words(name, 12.0, bold(body), name_ink(index)),
                                words(said, 12.0, body, theme.secondary)
                            ]
                            .spacing(6),
                        )
                        .padding(7)
                        .width(Length::Fill)
                        .clip(true)
                        .style(move |_| container::Style {
                            shadow: iced::Shadow {
                                color: Color::from_rgba8(0, 0, 0, 0.19),
                                offset: iced::Vector::new(0.0, 2.0),
                                blur_radius: 6.0,
                            },
                            ..crate::style::rounded(ground, radius)
                        }),
                    );
                }
                list.into()
            }
            Messages::Irc => {
                let mut list = column![].spacing(spacing(4.0, 6.0, 10.0)).padding([0, 12]);
                for (index, (name, said, time)) in messages.into_iter().enumerate() {
                    let lower = match index {
                        0 => "bob",
                        1 => "carol",
                        _ => "you",
                    };
                    let _ = name;
                    list = list.push(
                        row![
                            container(words(time, 12.0, body, theme.muted)).width(34),
                            container(words(lower, 12.0, bold(body), name_ink(index)))
                                .width(38)
                                .align_right(38),
                            words("│", 12.0, body, theme.faint),
                            words(said, 12.0, body, theme.secondary),
                        ]
                        .spacing(6),
                    );
                }
                list.into()
            }
            _ => {
                let mut list = column![].spacing(spacing(8.0, 12.0, 16.0)).padding([0, 12]);
                for (index, (name, said, _)) in messages.into_iter().enumerate() {
                    let avatar = match index {
                        0 => Color::from_rgb8(0x7a, 0x8c, 0xff),
                        1 => Color::from_rgb8(0x4c, 0xcf, 0x9e),
                        _ => theme.accent,
                    };
                    list = list.push(
                        row![
                            blob(28.0, 28.0, avatar, 14.0),
                            column![
                                words(name, 12.0, bold(body), name_ink(index)),
                                words(said, 12.0, body, theme.secondary)
                            ]
                            .spacing(1)
                        ]
                        .spacing(8),
                    );
                }
                list.into()
            }
        };
        let outline = f32::from(spec.outline.min(4));
        let line = crate::style::faded(theme.line, 0.7);
        let composer_ground = theme.composer;
        let composer_radius = creator.radius(10.0, 26.0);
        let send_radius = creator.radius(6.0, 9.0);
        let composer = container(
            row![
                container(words("Message #general", 12.0, body, theme.placeholder))
                    .width(Length::Fill)
                    .clip(true),
                blob(18.0, 18.0, theme.accent, send_radius)
            ]
            .align_y(Alignment::Center),
        )
        .padding([0, 6])
        .padding(iced::Padding {
            left: 10.0,
            right: 6.0,
            ..iced::Padding::ZERO
        })
        .height(28)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| crate::style::rounded(composer_ground, composer_radius));
        let chat_body = column![
            container(words("# general", 13.0, bold(heading), theme.text))
                .padding([8, 12])
                .height(30),
            blob(Length::Fill, outline, line, 0.0),
            Space::new().height(10),
            container(conversation).height(Length::Fill).clip(true),
            container(composer).padding(iced::Padding {
                left: 10.0,
                right: 10.0,
                bottom: 10.0,
                ..iced::Padding::ZERO
            }),
        ];
        let chat_ground = crate::style::faded(theme.chat, pane);
        let chat_radius = if glass {
            creator.radius(8.0, 14.0)
        } else {
            0.0
        };
        let shown_picture = picture(creator);
        let chat: Element<'a, Message> = match (&shown_picture, has_picture && !glass) {
            (Some(handle), true) => stack![
                iced::widget::image(handle.clone())
                    .content_fit(iced::ContentFit::Cover)
                    .width(Length::Fill)
                    .height(Length::Fill),
                chat_body
            ]
            .into(),
            _ => container(chat_body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::style::rounded(chat_ground, chat_radius))
                .into(),
        };
        let window = row![
            rail,
            container(row![side, container(chat).width(Length::Fill)].spacing(gap))
                .padding(iced::Padding {
                    top: gap,
                    right: gap,
                    bottom: gap,
                    left: 0.0,
                })
                .width(Length::Fill)
                .height(Length::Fill),
        ];
        let behind: Element<'a, Message> = match (&shown_picture, glass && has_picture) {
            (Some(handle), true) => iced::widget::image(handle.clone())
                .content_fit(iced::ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            _ if glass => {
                let glow = crate::style::faded(theme.accent, if theme.dark { 0.3 } else { 0.2 });
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| crate::style::fill(glow))
                    .into()
            }
            _ => Space::new().width(Length::Fill).height(Length::Fill).into(),
        };
        let (ground, edge, radius) = (theme.rail, theme.line, creator.radius(8.0, 16.0));
        container(stack![behind, window])
            .width(width)
            .height(height)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: radius.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Start: a theme to begin from, or a picture.
    fn creator_start_view(self, creator: &'a Creator) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let starts: Vec<_> = palette::starts(&self.cx.themes.custom).collect();
        let chips = iced::widget::responsive(move |size| {
            let starts: Vec<_> = palette::starts(&self.cx.themes.custom).collect();
            let chips = starts
                .iter()
                .enumerate()
                .map(|(index, start)| self.start_chip(index, start, creator.start == Some(index)))
                .collect();
            grid(chips, fits(size.width, 150.0, 8.0), 8.0)
        })
        .height(Length::Shrink);
        let _ = starts;
        let mut picture_controls: Vec<Element<'a, Message>> = vec![
            row![
                action(
                    theme,
                    if creator.spec.picture.is_some() {
                        "Choose another picture"
                    } else {
                        "Choose a picture"
                    },
                    Kind::Plain,
                    Some(message(Event::PickPicture)),
                )
                .icon(self.cx.look.icon(
                    Icon::Image,
                    18,
                    crate::theme::rgb(theme.text)
                ))
            ]
            .into(),
        ];
        if !creator.suggestions.is_empty() {
            let accent = theme.accent;
            let mut thumbnails = row![].spacing(8);
            for (index, suggestion) in creator.suggestions.iter().enumerate() {
                let thumbnail = suggestion.handle.clone();
                thumbnails = thumbnails.push(
                    button(
                        image(thumbnail)
                            .width(84)
                            .height(52)
                            .border_radius(6.0)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .padding(0)
                    .on_press(message(Event::Suggested(index)))
                    .style(move |_, status| button::Style {
                        border: iced::Border {
                            color: accent,
                            width: if crate::controls::lights(status) {
                                2.0
                            } else {
                                0.0
                            },
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }),
                );
            }
            picture_controls.push(thumbnails.wrap().into());
        }
        let bad = creator.note.as_ref().filter(|(_, bad)| *bad);
        if let Some((note, _)) = bad {
            picture_controls.push(form::note_in(note.clone(), theme.danger_text));
        }
        column![
            section(
                theme,
                "START FROM A THEME",
                vec![chips.into()],
                Some("Its colours carry over, and everything after is yours to change.".into()),
            ),
            section(
                theme,
                "OR FROM A PICTURE",
                picture_controls,
                bad.is_none().then(|| {
                    "The picture goes behind Yapfast, and its colours become the theme's.".into()
                }),
            ),
        ]
        .spacing(SECTIONS)
        .into()
    }

    /// A theme to start from: its colours as a window the size of a
    /// thumbnail, and its name.
    fn start_chip(
        self,
        index: usize,
        start: &palette::Start<'_>,
        chosen: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (ground, side, accent) = (
            themes::color(start.theme.chat),
            themes::color(start.theme.sidebar),
            themes::color(start.theme.accent),
        );
        let edge = theme.line;
        let window = container(
            row![
                blob(12.0, Length::Fill, side, 0.0),
                column![
                    blob(14.0, 3.0, accent, 1.5),
                    blob(10.0, 3.0, side, 1.5),
                    blob(14.0, 3.0, side, 1.5)
                ]
                .spacing(3)
                .padding(iced::Padding {
                    top: 6.0,
                    left: 4.0,
                    ..iced::Padding::ZERO
                })
            ]
            .height(Length::Fill),
        )
        .width(36)
        .height(26)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: iced::Border {
                color: edge,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });
        let (accent_ring, faint, line, hover) =
            (theme.accent, theme.faint, theme.line, theme.hover);
        button(
            row![
                window,
                text(start.name.to_owned())
                    .size(14)
                    .font(if chosen { BOLD } else { MEDIUM })
                    .color(if chosen { theme.text } else { theme.secondary })
                    .wrapping(text::Wrapping::None)
                    .width(Length::Fill)
            ]
            .spacing(10)
            .height(Length::Fill)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            left: 8.0,
            right: 10.0,
            ..iced::Padding::ZERO
        })
        .width(Length::Fill)
        .height(44)
        .on_press(message(Event::Start(index)))
        .style(move |_, status| {
            let lit = crate::controls::lights(status);
            button::Style {
                background: lit.then(|| hover.into()),
                border: iced::Border {
                    color: if chosen {
                        accent_ring
                    } else if lit {
                        faint
                    } else {
                        line
                    },
                    width: if chosen { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
    }

    /// Colours: the accent, the ground it sits on, and the tint.
    fn creator_colours_view(self, creator: &'a Creator) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let look = self.cx.look;
        let spec = &creator.spec;
        let mut swatches = row![].spacing(4).align_y(Alignment::Center);
        for accent in core::ACCENTS {
            swatches = swatches.push(kit::swatch(
                theme,
                rgb(accent),
                accent == spec.accent,
                message(Event::AccentPreset(accent)),
            ));
        }
        if let Some(picture) = &spec.picture {
            swatches = swatches.push(
                container(blob(1.0, 20.0, theme.line, 0.0))
                    .padding([0, 4])
                    .align_y(Alignment::Center),
            );
            for colour in picture.palette.iter().take(5) {
                swatches = swatches.push(kit::swatch(
                    theme,
                    rgb(*colour),
                    *colour == spec.accent,
                    message(Event::AccentPreset(*colour)),
                ));
            }
        }
        let hue = creator
            .hue
            .unwrap_or_else(|| core::lch(spec.accent & 0xff_ffff)[2] as f32);
        let stops: Vec<(f32, Color)> = (0..=12)
            .map(|step| {
                let place = step as f32 / 12.0;
                (place, rgb(accent_from_hue(place * 360.0)))
            })
            .collect();
        let thumb = if creator.hue.is_some() {
            rgb(accent_from_hue(hue))
        } else {
            rgb(spec.accent)
        };
        let hue_row = row![
            kit::hue_bar(
                theme,
                &stops,
                hue / 360.0,
                thumb,
                |at| message(Event::HueDrag(at * 360.0)),
                message(Event::HueDone),
            ),
            form::field(theme, "creator-accent", "#ffbf47", &creator.accent_hex)
                .on_input(|text| message(Event::Accent(text)))
                .size(14)
                .padding(8)
                .width(104),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let picture_shown = picture(creator);
        let ground_art = |ground: Color, ink: Color, muted: Color| -> Element<'a, Message> {
            container(
                column![
                    blob(Length::FillPortion(50), 6.0, ink, 3.0),
                    blob(Length::FillPortion(35), 5.0, muted, 2.5)
                ]
                .spacing(6)
                .padding([12, 8]),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(ground))
            .into()
        };
        let picture_art: Element<'a, Message> = match &picture_shown {
            Some(handle) => image(handle.clone())
                .content_fit(iced::ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => {
                let sunken = theme.sunken;
                container(look.glyph(Icon::Image, 22, theme.muted))
                    .center(Length::Fill)
                    .style(move |_| crate::style::fill(sunken))
                    .into()
            }
        };
        let backgrounds = row![
            kit::tile(
                look,
                "Dark",
                spec.background == Background::Dark,
                ground_art(
                    Color::from_rgb8(0x1e, 0x1f, 0x24),
                    Color::from_rgb8(0xee, 0xf0, 0xf4),
                    Color::from_rgb8(0x8d, 0x91, 0x9c)
                ),
                52.0,
                message(Event::Background(Background::Dark)),
            ),
            kit::tile(
                look,
                "Light",
                spec.background == Background::Light,
                ground_art(
                    Color::from_rgb8(0xf6, 0xf5, 0xf8),
                    Color::from_rgb8(0x2a, 0x2b, 0x33),
                    Color::from_rgb8(0x7a, 0x7d, 0x88)
                ),
                52.0,
                message(Event::Background(Background::Light)),
            ),
            kit::tile(
                look,
                "Picture",
                spec.background == Background::Picture,
                picture_art,
                52.0,
                message(Event::Background(Background::Picture)),
            ),
        ]
        .spacing(8);
        let mut background = vec![backgrounds.into()];
        if spec.background == Background::Picture && spec.picture.is_some() {
            background.push(
                container(
                    row![
                        container(
                            slider("Blur", u32::from(spec.blur), 100, |value| {
                                message(Event::Blur(value as u8))
                            })
                            .step(1)
                            .shown(if spec.blur == 0 {
                                "Sharp".to_owned()
                            } else {
                                format!("{}%", spec.blur)
                            })
                            .mark(false)
                            .view(theme)
                        )
                        .width(Length::Fill),
                        container(
                            slider("Dim", u32::from(spec.dim), 100, |value| {
                                message(Event::Dim(value as u8))
                            })
                            .step(1)
                            .shown(format!("{}%", spec.dim))
                            .mark(false)
                            .view(theme)
                        )
                        .width(Length::Fill),
                    ]
                    .spacing(24),
                )
                .padding(iced::Padding {
                    top: 4.0,
                    ..iced::Padding::ZERO
                })
                .into(),
            );
        }
        let mut sections = vec![
            section(
                theme,
                "ACCENT",
                vec![swatches.into(), hue_row.into()],
                Some("Buttons, links and what's selected. Pick one, slide to any hue, or type a colour.".into()),
            ),
            section(theme, "BACKGROUND", background, None),
        ];
        if spec.background != Background::Picture {
            let neutral = neutral(spec.ground);
            // The grounds follow the accent, are plain greys, or keep a
            // hue of their own from the theme or picture started from.
            let follows = !neutral && spec.ground & 0xff_ffff == spec.accent & 0xff_ffff;
            let grey = if theme.dark {
                Color::from_rgb8(0x8d, 0x91, 0x9c)
            } else {
                Color::from_rgb8(0x7a, 0x7d, 0x88)
            };
            let pill =
                |label: &'a str, chosen: bool, dot: Color, on: bool| -> Element<'a, Message> {
                    let (active, hover, accent, line) =
                        (theme.active, theme.hover, theme.accent, theme.line);
                    // Centred in its 32 px: on the button's own top, the
                    // label sat on the pill's upper edge.
                    button(
                        container(
                            row![
                                blob(16.0, 16.0, dot, 8.0),
                                text(label)
                                    .size(14)
                                    .font(if chosen { BOLD } else { MEDIUM })
                                    .color(if chosen { theme.text } else { theme.secondary })
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .center_y(Length::Fill),
                    )
                    .padding(iced::Padding {
                        left: 8.0,
                        right: 14.0,
                        ..iced::Padding::ZERO
                    })
                    .height(32)
                    .on_press(message(Event::Neutral(on)))
                    .style(move |_, status| button::Style {
                        background: if chosen {
                            Some(active.into())
                        } else {
                            crate::controls::lights(status).then(|| hover.into())
                        },
                        border: iced::Border {
                            color: if chosen { accent } else { line },
                            width: if chosen { 2.0 } else { 1.0 },
                            radius: 16.0.into(),
                        },
                        ..Default::default()
                    })
                    .into()
                };
            sections.push(section(
                theme,
                "TINT",
                vec![
                    row![
                        pill("Accent", follows, rgb(spec.accent), false),
                        pill("Neutral", neutral, grey, true)
                    ]
                    .spacing(8)
                    .into(),
                ],
                Some(
                    if neutral {
                        "The backgrounds are plain greys, and the accent stands alone."
                    } else if follows {
                        "The backgrounds take a little of the accent's hue."
                    } else {
                        "The backgrounds keep the hue of the theme you started from."
                    }
                    .into(),
                ),
            ));
        }
        column(sections).spacing(SECTIONS).into()
    }

    /// Shape: how messages are laid out, corners, spacing, outlines and
    /// glass.
    fn creator_shape_view(self, creator: &'a Creator) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let look = self.cx.look;
        let spec = &creator.spec;
        let chat = theme.chat;
        let (accent, ink, muted, raised, faint) = (
            theme.accent,
            theme.text,
            theme.muted,
            theme.raised,
            theme.faint,
        );
        let flat = ground(
            chat,
            column((0..2).map(|index| {
                row![
                    blob(
                        14.0,
                        14.0,
                        if index == 0 {
                            Color::from_rgb8(0x7a, 0x8c, 0xff)
                        } else {
                            accent
                        },
                        7.0
                    ),
                    column![
                        blob(26.0, 4.0, ink, 2.0),
                        blob(Length::Fill, 4.0, muted, 2.0)
                    ]
                    .spacing(4)
                ]
                .spacing(6)
                .into()
            }))
            .spacing(8)
            .padding([8, 8]),
        );
        let bubbles = ground(
            chat,
            column![
                row![
                    blob(Length::FillPortion(55), 15.0, raised, 7.5),
                    Space::new().width(Length::FillPortion(45))
                ],
                row![
                    Space::new().width(Length::FillPortion(55)),
                    blob(Length::FillPortion(45), 15.0, accent, 7.5)
                ],
            ]
            .spacing(6)
            .padding([8, 8]),
        );
        let cards = ground(
            chat,
            column((0..2).map(|index| {
                container(
                    row![
                        blob(18.0, 4.0, if index == 1 { accent } else { ink }, 2.0),
                        blob(Length::Fill, 4.0, muted, 2.0)
                    ]
                    .spacing(4),
                )
                .padding([7, 6])
                .width(Length::Fill)
                .style(move |_| crate::style::rounded(raised, 4))
                .into()
            }))
            .spacing(4)
            .padding([6, 7]),
        );
        let irc = ground(
            chat,
            column((0..4).map(|index| {
                row![
                    blob(10.0, 4.0, faint, 0.0),
                    blob(12.0, 4.0, if index == 2 { accent } else { ink }, 0.0),
                    blob(
                        Length::FillPortion(if index % 2 == 1 { 70 } else { 100 }),
                        4.0,
                        muted,
                        0.0
                    ),
                    Space::new().width(Length::FillPortion(if index % 2 == 1 { 30 } else { 1 }))
                ]
                .spacing(4)
                .into()
            }))
            .spacing(6)
            .padding([7, 6]),
        );
        let message_tiles = row![
            kit::tile(
                look,
                "Flat",
                spec.messages == Messages::Flat,
                flat,
                52.0,
                message(Event::Messages(Messages::Flat))
            ),
            kit::tile(
                look,
                "Bubbles",
                spec.messages == Messages::Bubbles,
                bubbles,
                52.0,
                message(Event::Messages(Messages::Bubbles))
            ),
            kit::tile(
                look,
                "Cards",
                spec.messages == Messages::Cards,
                cards,
                52.0,
                message(Event::Messages(Messages::Cards))
            ),
            kit::tile(
                look,
                "IRC",
                spec.messages == Messages::Irc,
                irc,
                52.0,
                message(Event::Messages(Messages::Irc))
            ),
        ]
        .spacing(8);
        let corners_word = match spec.corners {
            0 => "Square",
            50 => "Yapfast's own",
            100 => "Round",
            under if under < 50 => "Sharper",
            _ => "Rounder",
        };
        let corner_radius = creator.radius(8.0, 16.0);
        let corners = row![
            blob(44.0, 32.0, accent, corner_radius),
            container(
                slider("Square to round", u32::from(spec.corners), 100, |value| {
                    message(Event::Corners(value as u8))
                })
                .step(1)
                .shown(corners_word.to_owned())
                .mark(false)
                .view(theme)
            )
            .width(Length::Fill)
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        let densities = [Density::Compact, Density::Cozy, Density::Roomy];
        let density_tiles = row(densities.into_iter().enumerate().map(|(index, density)| {
            let (lines, step) = match index {
                0 => (5, 3.0),
                1 => (4, 5.0),
                _ => (3, 8.0),
            };
            let art = ground(
                chat,
                column((0..lines).map(|line| {
                    row![
                        blob(Length::FillPortion(100), 4.0, muted, 2.0),
                        Space::new().width(Length::FillPortion(if line % 2 == 1 { 16 } else { 1 }))
                    ]
                    .into()
                }))
                .spacing(step)
                .padding([5, 8]),
            );
            kit::tile(
                look,
                ["Compact", "Cozy", "Roomy"][index],
                spec.density == density,
                art,
                40.0,
                message(Event::Density(density)),
            )
        }))
        .spacing(8);
        let line = theme.line;
        let outline_radius = creator.radius(4.0, 10.0);
        let outline_tiles = row((0..4u8).map(|width| {
            let art = ground(
                chat,
                container(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(raised.into()),
                            border: iced::Border {
                                color: line,
                                width: f32::from(width),
                                radius: outline_radius.into(),
                            },
                            ..Default::default()
                        }),
                )
                .padding([8, 10]),
            );
            kit::tile(
                look,
                ["None", "Thin", "Medium", "Bold"][usize::from(width)],
                spec.outline == width,
                art,
                40.0,
                message(Event::Outline(width)),
            )
        }))
        .spacing(8);
        column![
            section(theme, "MESSAGES", vec![message_tiles.into()], None),
            section(theme, "CORNERS", vec![corners.into()], None),
            section(theme, "DENSITY", vec![density_tiles.into()], None),
            section(theme, "OUTLINES", vec![outline_tiles.into()], None),
            section(
                theme,
                "GLASS",
                vec![
                    crate::form::switch(
                        "Frosted glass panes",
                        spec.glass,
                        Some(message(Event::Glass(!spec.glass))),
                    )
                    .description(
                        "The sidebar and the conversation show the background through them, frosted.",
                    )
                    .view(look)
                ],
                None,
            ),
        ]
        .spacing(SECTIONS)
        .into()
    }

    /// Fonts: one for reading, and another for headings if wanted.
    fn creator_fonts_view(self, creator: &'a Creator) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let look = self.cx.look;
        let spec = &creator.spec;
        let headings = creator.headings_tab;
        let mut keys: Vec<(String, String)> = core::FONTS
            .iter()
            .map(|font| (font.key.to_owned(), font.label.to_owned()))
            .collect();
        keys.extend(
            spec.fonts
                .iter()
                .map(|font| (font.file.clone(), font.family.clone())),
        );
        let sunken = theme.sunken;
        // Each word on its own: some fonts have no space of their own.
        let sample = move |words: &'a str,
                           font: iced::Font,
                           ink: Color,
                           size: f32|
              -> Element<'a, Message> {
            let line = row(words.split(' ').map(|word| {
                text(word)
                    .size(size)
                    .font(font)
                    .color(ink)
                    .wrapping(text::Wrapping::None)
                    .into()
            }))
            .spacing(size * 0.28);
            container(line)
                .center(Length::Fill)
                .clip(true)
                .style(move |_| crate::style::fill(sunken))
                .into()
        };
        let tiles = iced::widget::responsive(move |size| {
            let mut tiles: Vec<Element<'a, Message>> = Vec::new();
            if headings {
                tiles.push(kit::tile(
                    look,
                    "Same as body",
                    spec.headings.is_none(),
                    sample("= Aa", crate::fonts::REGULAR, theme.muted, 18.0),
                    44.0,
                    message(Event::Headings(None)),
                ));
            }
            for (key, label) in &keys {
                let chosen = if headings {
                    spec.headings.as_deref() == Some(key.as_str())
                } else {
                    spec.body == *key
                };
                let font = face(creator, key);
                let font = core::FONTS
                    .iter()
                    .find(|built_in| built_in.key == key)
                    .filter(|built_in| built_in.file.is_none())
                    .map_or(font, |_| crate::fonts::MEDIUM);
                tiles.push(kit::tile(
                    look,
                    label.clone(),
                    chosen,
                    sample("Aa Hello", font, theme.text, 17.0),
                    44.0,
                    message(if headings {
                        Event::Headings(Some(key.clone()))
                    } else {
                        Event::Body(key.clone())
                    }),
                ));
            }
            grid(tiles, fits(size.width, 112.0, 8.0), 8.0)
        })
        .height(Length::Shrink);
        let roles = row![
            crate::controls::choices_within(
                theme,
                &["Body", "Headings"],
                usize::from(headings),
                196.0,
                |index| message(Event::FontRole(index == 1)),
            ),
            Space::new().width(Length::Fill),
            action(
                theme,
                "Import a font",
                Kind::Ghost,
                Some(message(Event::PickFont(headings))),
            )
            .small()
            .icon(look.icon(Icon::Plus, 16, crate::theme::rgb(theme.secondary))),
        ]
        .align_y(Alignment::Center);
        let good = creator
            .note
            .as_ref()
            .filter(|(_, bad)| !*bad)
            .map(|(note, _)| note.clone());
        let mut controls = vec![roles.into(), tiles.into()];
        if let Some((note, true)) = &creator.note {
            controls.push(form::note_in(note.clone(), theme.danger_text));
        }
        section(theme, "FONT", controls, good)
    }

    /// Advanced: every colour, worked out from the steps, to fine-tune.
    fn creator_advanced_view(self, creator: &'a Creator) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut parts = Vec::new();
        if creator.editing.is_none() {
            let names: Vec<String> = palette::starts(&self.cx.themes.custom)
                .map(|start| start.name.to_owned())
                .collect();
            parts.push(section(
                theme,
                "START FROM",
                vec![
                    form::dropdown(
                        names,
                        creator.start,
                        self.s.open == Some(super::Drop::Start),
                        crate::app::settings(SettingsEvent::Drop(super::Drop::Start)),
                        |index| message(Event::Start(index)),
                    )
                    .view(self.cx.look),
                ],
                None,
            ));
        }
        parts.push(
            crate::form::switch(
                "Dark",
                creator.theme.dark,
                Some(message(Event::Dark(!creator.theme.dark))),
            )
            .description("Drop-down lists and scroll bars draw dark to match.")
            .view(self.cx.look),
        );
        parts.push(section(
            theme,
            "EVERY COLOUR",
            vec![
                form::note(
                    theme,
                    "Each as #rgb, #rrggbb or #rrggbbaa. The window shows a change as you type it.",
                ),
                colour_rows(theme, &creator.fields, &creator.theme, |index, text| {
                    message(Event::AdvancedColour(index, text))
                }),
            ],
            None,
        ));
        column(parts).spacing(SECTIONS).into()
    }
}

/// Every colour's row, in two columns read down when they fit.
pub(super) fn colour_rows<'a>(
    theme: &'a crate::theme::Theme,
    fields: &'a [(String, String, bool)],
    palette: &'a palette::Theme,
    on_input: impl Fn(usize, String) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    iced::widget::responsive(move |size| {
        let columns = if size.width >= TWO_COLUMNS { 2 } else { 1 };
        let per = fields.len().div_ceil(columns);
        let mut line = row![].spacing(COLOUR_GAP);
        for column_index in 0..columns {
            let mut cells = column![];
            for (index, (name, hex, wrong)) in
                fields.iter().enumerate().skip(column_index * per).take(per)
            {
                let shown = palette
                    .colour(index)
                    .map_or(Color::TRANSPARENT, themes::color);
                cells = cells.push(form::colour_row(
                    theme,
                    name,
                    shown,
                    hex,
                    *wrong,
                    move |text| on_input(index, text),
                ));
            }
            line = line.push(container(cells).width(Length::FillPortion(1)));
        }
        line.into()
    })
    .height(Length::Shrink)
    .into()
}

#[cfg(test)]
mod tests {
    /// Where `wallpapers` is `Wallpapers` (a file system that ignores case,
    /// here a link), each picture is suggested once.
    #[cfg(unix)]
    #[test]
    fn one_wallpapers_folder_under_two_names_is_looked_in_once() {
        let pictures = tempfile::tempdir().unwrap();
        let wallpapers = pictures.path().join("Wallpapers");
        std::fs::create_dir(&wallpapers).unwrap();
        std::os::unix::fs::symlink(&wallpapers, pictures.path().join("wallpapers")).unwrap();
        ::image::RgbaImage::from_pixel(4, 4, ::image::Rgba([200, 40, 80, 255]))
            .save(wallpapers.join("dunes.png"))
            .unwrap();
        assert_eq!(super::pictures_in(pictures.path()).len(), 1);
    }
}
