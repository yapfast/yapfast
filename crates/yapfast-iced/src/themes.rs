//! The themes the window shows: one of the core's colour themes, or an
//! installed theme package resolved over one of them, made the kit's
//! [`Theme`]. See docs/themes.md.
//!
//! The theme is the one `layout.json` names: a built-in id or one of the
//! person's own (`custom_themes`), Bonfire when it names none, Yap for an
//! id Yapfast doesn't know. The window-control command `theme ID` chooses
//! another and saves it, as Settings → Appearance and Themes do; their iced
//! views share `settings/appearance.rs`.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use iced::{Color, Size};
use yapfast_core::themes::package::art::{self, ArtSurface, Source};
use yapfast_core::{
    ring,
    sounds::{self, Sound, ThemedSound},
    theme::{self as palette, Colour, Custom},
    themes::{
        Mode, Performance, Resolved, check, check::Strictness, chrome, content, install,
        layout as theme_layout, media, options, package,
    },
};

use crate::{
    layout::{self, Layout},
    theme::Theme,
};

/// The theme shown when `layout.json` names none.
const DEFAULT: &str = "bonfire";
/// Installed package ids are kept apart from built-in and custom ids.
pub const INSTALLED: &str = "installed:";

/// An installed package as Settings → Themes and Appearance need it.
pub struct Installed {
    /// Its id as `layout.json` keeps it, including [`INSTALLED`].
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: String,
    pub modes: Vec<Mode>,
    pub notes: Vec<String>,
    pub options: Vec<options::ThemeOption>,
    pub choices: options::Choices,
    /// The revision retained by core for Roll back, when there is one.
    pub previous: Option<String>,
    /// What this theme remixes, if named in `theme.json`.
    pub based_on: Option<String>,
    /// Core's cost summary line for Settings → Themes.
    pub cost: String,
    /// Decoded thumbnail of `package.preview[0]`, if present and readable.
    pub preview: Option<iced::advanced::image::Handle>,
    /// Resolved palette for miniature colour previews when no preview image exists.
    pub palette_preview: palette::Theme,
    folder: PathBuf,
    package: package::Package,
    /// Whether its fonts, icons, sounds and art are loaded: only while it
    /// shows (chosen, on trial, in the creator's preview). The others
    /// keep what Settings → Themes lists them with, so six installed
    /// themes don't hold six themes' pictures and sounds.
    live: bool,
    shown: Theme,
    metrics: yapfast_iced_kit::theme::Metrics,
    motion: package::Motion,
    animations: Vec<package::Animation>,
    icons: HashMap<String, yapfast_iced_kit::icons::Replacement>,
    sounds: ThemeSounds,
    sound_prefs: SoundPrefs,
    chrome: chrome::Chrome,
    content: content::Content,
    art: ThemeArt,
    layout: theme_layout::Layout,
}

/// How fast theme motion may run given window visibility, focus, Settings
/// coverage, Performance and `motion.unfocused`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pace {
    #[default]
    Full,
    Slow,
    Still,
}

impl Pace {
    pub const SLOW_FPS: u64 = 15;

    pub fn runs(self) -> bool {
        self != Self::Still
    }

    pub fn cap_fps(self, fps: u64) -> u64 {
        match self {
            Self::Full => fps,
            Self::Slow => fps.min(Self::SLOW_FPS),
            Self::Still => 0,
        }
    }
}

/// The moment every theme motion counts its frames from, so motions asked
/// for apart still share frames.
fn motion_epoch(now: Instant) -> Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(|| now)
}

/// The first moment after `now` on a beat of `period`, counted from the
/// one epoch every theme motion shares, so redraws asked for together
/// coincide.
pub(crate) fn on_beat(now: Instant, period: Duration) -> Instant {
    let epoch = motion_epoch(now);
    let period = period.as_nanos().max(1);
    let since = now.saturating_duration_since(epoch).as_nanos();
    let next = (since / period + 1) * period;
    epoch + Duration::from_nanos(u64::try_from(next).unwrap_or(u64::MAX))
}

/// When a theme's motion that waits `wait_ms` between frames next draws:
/// the first multiple of `wait_ms` after `now` on the shared grid. Asked
/// from `now` each, several animations at 15 frames a second (under
/// Settings) drew 30 or 45 frames between them, each a little after the
/// last; on one grid they share their frames.
pub fn next_tick(now: Instant, wait_ms: u64) -> Instant {
    on_beat(now, Duration::from_millis(wait_ms.max(1)))
}

/// When a decoration waiting `wait` for its next change wakes the window:
/// a smooth one on the next 60 Hz beat; a stepped one (a wait of 50 ms or
/// more) on the next tenth of a second after its step, the beat ambient
/// particles keep, so the steps of every slot, sheet and overlay, and the
/// particles, share frames rather than each waking the window at its own
/// moment (Eyeliner's twenty-odd loops woke it 27 times a second). Both
/// beats are on [`next_tick`]'s grid.
pub(crate) fn wake_at(now: Instant, wait: Duration) -> Instant {
    let beat = if wait >= Duration::from_millis(50) {
        STEP_BEAT
    } else {
        Duration::from_millis(1000 / 60)
    };
    on_beat(now + wait, beat)
}

/// The beat stepped decorations and ambient particles keep.
pub(crate) const STEP_BEAT: Duration = Duration::from_millis(100);

pub fn pace_of(
    shown: bool,
    minimized: bool,
    focused: bool,
    covered: bool,
    performance: Performance,
    reduced: bool,
    unfocused: package::Unfocused,
) -> Pace {
    if !shown || minimized || reduced || performance == Performance::Light {
        return Pace::Still;
    }
    if focused && covered {
        return Pace::Slow;
    }
    if focused {
        return Pace::Full;
    }
    match (performance, unfocused) {
        (Performance::Balanced | Performance::Light, _) | (_, package::Unfocused::Still) => {
            Pace::Still
        }
        (Performance::Full, package::Unfocused::Slow) => Pace::Slow,
        (Performance::Full, package::Unfocused::Full) => Pace::Full,
    }
}

/// A person's choices for one installed theme's sound replacements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SoundPrefs {
    pub on: bool,
    pub interface: bool,
    pub volume: u8,
}

impl Default for SoundPrefs {
    fn default() -> Self {
        Self {
            on: true,
            interface: false,
            volume: 50,
        }
    }
}

/// An entry in the installed-theme lock that could not safely be opened.
/// It remains visible in Settings → Themes with the core's exact reason
/// instead of disappearing from the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Default)]
pub struct ThemeArt {
    surfaces: HashMap<ArtSurface, Vec<ArtLayer>>,
    overlays: Vec<ArtLayer>,
    chat_overlays: Vec<ArtLayer>,
    procedural_overlays: Vec<art::Overlay>,
    chat_procedural_overlays: Vec<art::Overlay>,
    reserves: HashMap<ArtSurface, art::Reserve>,
    performance: Performance,
    reduced_motion: bool,
    glass: Option<GlassPane>,
    particles: Vec<ThemeParticles>,
    particle_events: Arc<Mutex<Vec<ParticleEvent>>>,
    list_art: HashMap<String, TileArtImage>,
    /// A card's top-edge picture with masking tape painted on it (Zine's
    /// torn tops), by its key: the tape, cut out to lie across the torn
    /// edge, half on the board ([`split_card_tape`]).
    card_tapes: HashMap<String, yapfast_core::themes::lists::TileArt>,
    footer_height: Option<f32>,
    text: BTreeMap<art::TextRole, art::TextLook>,
    events: BTreeMap<art::EventSlot, art::Event>,
    foil: Option<art::Foil>,
    ransom: Option<Ransom>,
    /// Pictures on your own bubbles and on others', over their grounds.
    bubble_art: [Vec<ArtLayer>; 2],
    /// The channel-change transition of the variant shown.
    transition: Option<art::Transition>,
    /// Keys under the composer's buttons, by button.
    keys: BTreeMap<art::KeySlot, ComposerKey>,
}

/// A key under a composer button: its picture at rest and held down, its
/// logical size, and how far the glyph on it goes down while pressed.
#[derive(Clone, Debug)]
pub struct ComposerKey {
    pub up: iced::advanced::image::Handle,
    pub down: Option<iced::advanced::image::Handle>,
    pub size: Size,
    pub travel: f32,
}

/// People's names cut as a ransom note: the faces the letters cycle
/// through, and the scraps' grounds and inks.
#[derive(Clone, Debug)]
pub struct Ransom {
    pub fonts: Vec<iced::Font>,
    /// By `fonts`: the face draws small letters as capitals.
    pub capitals: Vec<bool>,
    /// The tallest face's line in ems: every scrap of a name stands this
    /// tall, as Slint's row of letters takes its tallest letter's height.
    pub line: f32,
    pub papers: Vec<(iced::Color, iced::Color)>,
}

/// The scraps when a theme names none: newsprint, ink and a pink, as Slint.
const RANSOM_PAPERS: [(u32, u32); 3] = [
    (0xfff5_f1e8, 0xff14_1414),
    (0xff14_1414, 0xfff5_f1e8),
    (0xffff_5fa2, 0xff14_1414),
];

fn ransom(folder: &Path, resolved: &Resolved) -> Option<Ransom> {
    let names = resolved.layout.names.as_ref()?;
    let mut capitals = Vec::new();
    let mut line: f32 = 1.2;
    let mut fonts: Vec<iced::Font> = names
        .fonts
        .iter()
        .filter_map(|file| std::fs::read(folder.join(file)).ok())
        .filter_map(|bytes| {
            let caps = theme_layout::draws_small_as_capitals(&bytes);
            if let Some(ems) = theme_layout::line_ems(&bytes) {
                line = line.max(ems.min(3.0));
            }
            let font = yapfast_iced_kit::fonts::load_theme_font(bytes).ok()?;
            capitals.push(caps);
            Some(font.font)
        })
        .collect();
    if fonts.is_empty() {
        // The names and body faces when the theme lists none.
        for role in [package::Role::Names, package::Role::Body] {
            if let Some(file) = resolved
                .types
                .get(&role)
                .and_then(|kind| kind.font.as_ref())
                && let Ok(bytes) = std::fs::read(folder.join(file))
                && let Ok(font) = yapfast_iced_kit::fonts::load_theme_font(bytes)
            {
                fonts.push(font.font);
                capitals.push(false);
            }
        }
    }
    let papers = if names.papers.is_empty() {
        RANSOM_PAPERS.to_vec()
    } else {
        names.papers.clone()
    };
    Some(Ransom {
        fonts,
        capitals,
        line,
        papers: papers
            .into_iter()
            .map(|(ground, ink)| (argb(ground), argb(ink)))
            .collect(),
    })
}

#[derive(Clone)]
pub struct ThemeParticles {
    pub spec: art::Particles,
    pub sprite: Option<iced::advanced::image::Handle>,
}

#[derive(Clone, Copy)]
pub struct ParticleEvent {
    pub trigger: art::Trigger,
    pub started: Instant,
    pub origin: Option<(f32, f32)>,
}

pub type ParticleView<'a> = (
    &'a [ThemeParticles],
    &'a Arc<Mutex<Vec<ParticleEvent>>>,
    Performance,
    bool,
);

#[derive(Clone)]
pub struct TileArtImage {
    pub handle: iced::advanced::image::Handle,
    pub art: yapfast_core::themes::lists::TileArt,
    pub pieces: Option<Vec<Option<iced::advanced::image::Handle>>>,
    pub frames: Vec<(
        iced::advanced::image::Handle,
        Option<Vec<Option<iced::advanced::image::Handle>>>,
    )>,
}

#[derive(Clone)]
pub struct ArtLayer {
    pub handle: iced::advanced::image::Handle,
    /// Each frame its own picture: only for the tooltip's art, which the
    /// kit draws (see `sheet` for every other surface's).
    pub frames: Vec<iced::advanced::image::Handle>,
    /// A sprite sheet whole, with its columns, rows and frame count: one
    /// picture uploaded once, its frame shown by clipping. Switching
    /// pictures each frame made the GPU take a new one each time (Hurt's
    /// breathing room cost a fifth of a core while anything moved).
    pub sheet: Option<(iced::advanced::image::Handle, u32, u32, u32)>,
    pub width: u32,
    pub height: u32,
    pub layer: art::Layer,
    /// Masking tape at the top of a card, dialog or popout (a `contain`
    /// picture at the top whose file is named `tape…`): where the tape's
    /// middle is down the picture, 0 to 1. It's drawn so the surface's top
    /// edge runs through that middle, half on what's behind, pinning the
    /// surface down, as asked of Zine's tape.
    pub pin: Option<f32>,
    /// A pinning tape picture with two strips (a dialog's): each strip cut
    /// out, its size in the picture's pixels and the angle it lies at, so
    /// each goes across a top corner, as real corner tape.
    pub corners: Vec<(iced::advanced::image::Handle, Size, f32)>,
    /// A frame round the surface: stretched over it, its middle clear and
    /// its edges inked (Bubblegum's plum rim). It's drawn over what's in
    /// the surface, as its border is, so scrolled content passes under it.
    pub rim: bool,
}

#[derive(Clone)]
pub struct GlassPane {
    /// The reduced, preblurred window art.
    pub blurred: yapfast_iced_kit::widgets::BlurredWindow,
    pub style: art::Glass,
    grounds: HashMap<ArtSurface, u32>,
    /// The window art decoded, blurred, blended and dimmed once, which a
    /// resize bakes again from without reading or decoding anything.
    source: Arc<GlassSource>,
}

/// What glass is baked from at any window size ([`bake_glass`]).
pub struct GlassSource {
    ground: [u8; 3],
    blur: f32,
    layers: Vec<GlassLayer>,
}

/// One window art layer, ready to lay on the reduced canvas.
enum GlassLayer {
    /// At its own size, already reduced, placed by `position`.
    Placed(image::RgbaImage, (f32, f32)),
    /// A tile, already reduced.
    Tile(image::RgbaImage),
    /// Fitted to the canvas, kept no larger than any canvas needs.
    Fit(art::Fit, image::RgbaImage),
}

/// The glass a window resize asks for, baked off the window's thread.
pub struct GlassJob {
    id: String,
    source: Arc<GlassSource>,
    window: (u32, u32),
}

/// Baked glass for [`Themes::glass_baked`], by the generation that asked.
#[derive(Clone)]
pub struct GlassBaked {
    generation: u64,
    panes: Vec<(String, yapfast_iced_kit::widgets::BlurredWindow)>,
}

impl std::fmt::Debug for GlassBaked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlassBaked")
            .field("generation", &self.generation)
            .field("panes", &self.panes.len())
            .finish()
    }
}

/// How long a resize rests before its glass is baked: dragging a window's
/// edge bakes once when it stops, not for every size on the way.
const GLASS_REST: std::time::Duration = std::time::Duration::from_millis(150);

/// Bakes `jobs` once the window has rested at their size, off the window's
/// thread; nothing if a later resize asked again meanwhile.
pub async fn bake_glass_later(
    jobs: Vec<GlassJob>,
    generation: u64,
    latest: Arc<std::sync::atomic::AtomicU64>,
) -> Option<GlassBaked> {
    tokio::time::sleep(GLASS_REST).await;
    if latest.load(Ordering::Relaxed) != generation {
        return None;
    }
    let panes = tokio::task::spawn_blocking(move || {
        jobs.into_iter()
            .map(|job| (job.id, bake_glass(&job.source, job.window)))
            .collect()
    })
    .await
    .ok()?;
    Some(GlassBaked { generation, panes })
}

impl GlassPane {
    pub fn ground(&self, surface: ArtSurface) -> u32 {
        self.grounds.get(&surface).copied().unwrap_or(0xff00_0000)
    }
}

impl ThemeArt {
    fn decorations(&self) -> std::collections::HashSet<String> {
        let mut surfaces = self
            .surfaces
            .keys()
            .map(|surface| surface.name().to_owned())
            .collect::<std::collections::HashSet<_>>();
        if let Some(glass) = &self.glass {
            surfaces.extend(
                glass
                    .style
                    .surfaces
                    .iter()
                    .map(|surface| surface.name().to_owned()),
            );
        }
        surfaces
    }
}

#[derive(Clone, Default)]
struct ThemeSounds {
    app: HashMap<Sound, ThemedSound>,
    rings: HashMap<media::SoundSlot, Arc<[i16]>>,
    cues: HashMap<media::SoundSlot, SoundCue>,
}

#[derive(Clone)]
struct SoundCue {
    samples: Vec<Arc<[i16]>>,
    volume: f32,
}

static NEXT_CUE: AtomicUsize = AtomicUsize::new(0);

/// `--default-theme`: this run starts in the default look.
static DEFAULT_THIS_RUN: AtomicBool = AtomicBool::new(false);

/// Starts this run in the default look, keeping the theme chosen in
/// `layout.json` (`--default-theme`).
pub fn default_this_run() {
    DEFAULT_THIS_RUN.store(true, Ordering::Relaxed);
}

/// The themes this data folder offers and the one it chose.
pub struct Themes {
    pub custom: Vec<Custom>,
    pub installed: Vec<Installed>,
    /// Package companions of creator-made custom themes. They use their
    /// `custom:` layout ids but do not appear a second time in Installed.
    companions: Vec<Installed>,
    pub refused: Vec<Refused>,
    pub chosen: Option<String>,
    previous: Option<String>,
    trial: Option<ThemeTrial>,
    creator_preview: Option<Installed>,
    /// `--theme-dev`'s folder as last read, shown over the theme chosen,
    /// and the newest change in it then.
    dev: Option<(Installed, Option<std::time::SystemTime>)>,
    /// The newest change in `--theme-dev`'s folder when it was last read,
    /// whether that read passed or not, so a folder that fails is read
    /// again only once it's saved again.
    dev_seen: Option<Option<std::time::SystemTime>>,
    /// Ctrl+Shift+T turned `--theme-dev` off for the rest of this run.
    dev_off: bool,
    /// `--theme-dev`'s folder.
    dev_folder: Option<PathBuf>,
    /// Moves on with each resize, so glass baked for an older size is let go.
    glass_generation: Arc<std::sync::atomic::AtomicU64>,
    system_mode: Mode,
    window: (u32, u32),
}

const TRIAL_ID: &str = "trial";
/// The start of a reviewed trial's temporary folder's name.
const TRIAL_FOLDER: &str = ".yapfast-theme-trial-";

/// Removes trial folders in `temp` older than `age`, left by a Yapfast that
/// stopped during a trial.
fn sweep_trials(temp: &Path, age: Duration) {
    let Ok(entries) = std::fs::read_dir(temp) else {
        return;
    };
    for entry in entries.flatten() {
        let named = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(TRIAL_FOLDER));
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|elapsed| elapsed > age);
        if named && old && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            std::fs::remove_dir_all(entry.path()).ok();
        }
    }
}
const TRIAL_SECONDS: u64 = 30;
pub const CREATOR_PREVIEW_ID: &str = "creator-preview";
/// `--theme-dev`'s theme's id.
pub const DEV_ID: &str = "dev";

/// `--theme-dev FOLDER`: a theme maker's folder, shown over the theme
/// chosen and shown again each time a file in it is saved.
static DEV: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Shows `folder` this run (`--theme-dev`).
pub fn develop(folder: PathBuf) {
    DEV.set(folder).ok();
}

/// When `--theme-dev`'s folder last changed, looked at off the window's
/// thread every half second; it says so only when that changes.
pub fn dev_saves(folder: PathBuf) -> impl iced::futures::Stream<Item = ()> + Send + 'static {
    iced::futures::stream::unfold((folder, None), |(folder, mut last)| async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let looked = folder.clone();
            let Ok(stamp) = tokio::task::spawn_blocking(move || newest(&looked)).await else {
                continue;
            };
            if last != Some(stamp) {
                last = Some(stamp);
                return Some(((), (folder, last)));
            }
        }
    })
}

/// The newest change in a folder, two levels deep, as Slint's watch reads.
fn newest(folder: &Path) -> Option<std::time::SystemTime> {
    fn walk(folder: &Path, depth: usize, newest: &mut Option<std::time::SystemTime>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if let Ok(modified) = metadata.modified() {
                *newest = (*newest).max(Some(modified));
            }
            if metadata.is_dir() && depth > 0 {
                walk(&entry.path(), depth - 1, newest);
            }
        }
    }
    let mut found = None;
    walk(folder, 2, &mut found);
    found
}

struct ThemeTrial {
    id: String,
    name: String,
    back_to: Option<String>,
    until: Instant,
    reviewed: Option<ReviewedTrial>,
}

struct ReviewedTrial {
    fetched: install::Fetched,
    _folder: tempfile::TempDir,
    package: Installed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrialInfo {
    pub name: String,
    pub seconds: u64,
}

struct ResolveSource {
    id: String,
    folder: PathBuf,
    package: package::Package,
    previous: Option<String>,
    cost: String,
    /// Its gallery thumbnail when already read (from `<data>/theme-thumbs/`),
    /// else read from the package's first preview.
    thumbnail: Option<Option<iced::advanced::image::Handle>>,
}

fn resolve_package(
    source: ResolveSource,
    preferred: Mode,
    layout: &Layout,
    window: (u32, u32),
    live: bool,
) -> Result<Installed, String> {
    let ResolveSource {
        id,
        folder,
        package,
        previous,
        cost,
        thumbnail,
    } = source;
    // Until the mode option is exposed, prefer dark, as the old themes page
    // did when the desktop did not report a light preference.
    let choices = layout.theme_options.get(&id).cloned().unwrap_or_default();
    let mode = selected_mode(&package, &choices, preferred);
    let resolved = package
        .resolve_with(mode, &choices)
        .map_err(|error| error.to_string())?;
    let palette = resolved_palette(&resolved);
    let shown = theme(&id, &palette);
    let mut metrics = metrics(&resolved, layout.theme_performance, layout.reduce_motion);
    metrics.readable_text = layout.readable_text;
    let Loaded {
        types,
        icons,
        sounds,
        art,
    } = load(
        live,
        &folder,
        &resolved,
        &palette,
        &package.options,
        &choices,
        (layout.theme_performance, layout.reduce_motion),
        window,
    );
    metrics.types = types;
    metrics.decorations = art.decorations();
    metrics.tooltip = tooltip_surface(&art);
    let sound_prefs = layout.theme_sounds.get(&id).copied().unwrap_or_default();
    let cues_on = sound_prefs.interface && sound_prefs.volume > 0;
    metrics.hover_cue = cues_on && sounds.cues.contains_key(&media::SoundSlot::Hover);
    metrics.press_cue = cues_on && sounds.cues.contains_key(&media::SoundSlot::Press);
    shown.register_metrics(metrics.clone());
    let based_on = package.based_on.clone();
    let preview = thumbnail.unwrap_or_else(|| installed_preview(&folder, &package.preview));
    Ok(Installed {
        id,
        name: package.name.clone(),
        version: package.version.clone(),
        authors: package.authors.clone(),
        description: package.description.clone(),
        modes: package.modes(),
        notes: package.notes.clone(),
        options: package.options.clone(),
        choices,
        previous,
        based_on,
        cost,
        preview,
        palette_preview: palette,
        folder,
        package,
        live,
        shown,
        metrics,
        motion: resolved.motion,
        animations: resolved.animations,
        icons,
        sounds,
        sound_prefs,
        chrome: resolved.chrome,
        content: resolved.content,
        art,
        layout: resolved.layout,
    })
}

/// What a showing theme loads from its files: its fonts, icons, sounds
/// and art.
struct Loaded {
    types: HashMap<String, yapfast_iced_kit::theme::TypeStyle>,
    icons: HashMap<String, yapfast_iced_kit::icons::Replacement>,
    sounds: ThemeSounds,
    art: ThemeArt,
}

/// A theme's files loaded while it shows (`live`), else nothing read: its
/// colours, shapes and words come from `resolved` alone.
#[allow(
    clippy::too_many_arguments,
    reason = "each part it draws is its own input"
)]
fn load(
    live: bool,
    folder: &Path,
    resolved: &Resolved,
    palette: &palette::Theme,
    options: &[options::ThemeOption],
    choices: &options::Choices,
    (performance, reduced_motion): (Performance, bool),
    window: (u32, u32),
) -> Loaded {
    if !live {
        return Loaded {
            types: HashMap::new(),
            icons: HashMap::new(),
            sounds: ThemeSounds::default(),
            art: ThemeArt {
                performance,
                reduced_motion,
                ..ThemeArt::default()
            },
        };
    }
    Loaded {
        types: typography(folder, resolved),
        icons: icons(folder, resolved, palette),
        sounds: sounds(folder, resolved, options, choices),
        art: theme_art(folder, resolved, performance, reduced_motion, window),
    }
}

fn creator_font_bytes(file: &str) -> Option<&'static [u8]> {
    match file {
        "AtkinsonHyperlegibleNext-Regular.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/AtkinsonHyperlegibleNext-Regular.ttf"
        )),
        "DisplayName-Tempo.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/DisplayName-Tempo.ttf"
        )),
        "DisplayName-Modern.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/DisplayName-Modern.ttf"
        )),
        "DisplayName-MonkeyBars.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/DisplayName-MonkeyBars.ttf"
        )),
        "DisplayName-Journal.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/DisplayName-Journal.ttf"
        )),
        "DisplayName-8Bit.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/DisplayName-8Bit.ttf"
        )),
        "DisplayName-Mainframe.ttf" => Some(include_bytes!(
            "../../../crates/yapfast-iced-kit/fonts/DisplayName-Mainframe.ttf"
        )),
        _ => None,
    }
}

fn regenerate_managed_creator(root: &Path, custom: &Custom, folder: &Path) -> Result<(), String> {
    let Some(spec) = custom.creator.as_ref() else {
        return Ok(());
    };
    let Some(named) = custom.folder.as_deref() else {
        return Ok(());
    };
    let expected = PathBuf::from("themes/creator").join(yapfast_core::themes::creator::slug(
        custom.id.strip_prefix("custom:").unwrap_or(&custom.id),
    ));
    if Path::new(named) != expected || folder != root.join(&expected) || folder.exists() {
        return Ok(());
    }
    // Imported fonts and picture backdrops cannot be recreated from layout
    // metadata alone. Leave those missing folders refused instead of
    // manufacturing a partial package.
    if !spec.fonts.is_empty() || spec.picture.is_some() {
        return Ok(());
    }
    let derived = yapfast_core::themes::creator::derive(spec);
    yapfast_core::themes::creator::write_folder(
        folder,
        spec,
        &custom.name,
        &custom.id,
        &derived,
        creator_font_bytes,
    )
    .map_err(|error| error.to_string())
}

impl Themes {
    /// The channel-change transition of `id` as it's resolved and shown:
    /// its variant (Auto follows the desktop) and options. Nothing is read
    /// from disk.
    pub fn transition(&self, id: &str) -> Option<art::Transition> {
        self.package(id)?.art.transition
    }

    fn package(&self, id: &str) -> Option<&Installed> {
        if id == DEV_ID
            && let Some((dev, _)) = &self.dev
        {
            return Some(dev);
        }
        if id == CREATOR_PREVIEW_ID
            && let Some(package) = &self.creator_preview
        {
            return Some(package);
        }
        if id == TRIAL_ID
            && let Some(package) = self
                .trial
                .as_ref()
                .and_then(|trial| trial.reviewed.as_ref())
                .map(|reviewed| &reviewed.package)
        {
            return Some(package);
        }
        self.installed
            .iter()
            .chain(&self.companions)
            .find(|package| package.id == id)
    }

    fn package_mut(&mut self, id: &str) -> Option<&mut Installed> {
        if id == DEV_ID
            && let Some((dev, _)) = &mut self.dev
        {
            return Some(dev);
        }
        if id == CREATOR_PREVIEW_ID
            && let Some(package) = &mut self.creator_preview
        {
            return Some(package);
        }
        if id == TRIAL_ID
            && let Some(package) = self
                .trial
                .as_mut()
                .and_then(|trial| trial.reviewed.as_mut())
                .map(|reviewed| &mut reviewed.package)
        {
            return Some(package);
        }
        self.installed
            .iter_mut()
            .chain(&mut self.companions)
            .find(|package| package.id == id)
    }

    fn packages_mut(&mut self) -> impl Iterator<Item = &mut Installed> {
        self.installed
            .iter_mut()
            .chain(&mut self.companions)
            .chain(self.trial.iter_mut().filter_map(|trial| {
                trial
                    .reviewed
                    .as_mut()
                    .map(|reviewed| &mut reviewed.package)
            }))
            .chain(self.creator_preview.iter_mut())
            .chain(self.dev.iter_mut().map(|(dev, _)| dev))
    }

    /// The themes `layout` keeps and the packages installed in `root`.
    #[cfg(test)]
    pub fn read(root: &Path, layout: &Layout) -> Self {
        Self::read_live(root, layout, (1024, 768), layout.theme.as_deref())
    }

    /// The themes at the start: [`Self::read_live`], showing the default
    /// look this run when `--default-theme` asked, with the theme chosen
    /// offered by Back and kept in `layout.json`.
    pub fn at_start(root: &Path, layout: &Layout, window: (u32, u32)) -> Self {
        // A trial lasts 30 seconds; its folder outlives it only when a
        // Yapfast stopped without cleaning up.
        sweep_trials(&std::env::temp_dir(), Duration::from_secs(60 * 60));
        Self::start_with(
            root,
            layout,
            window,
            DEFAULT_THIS_RUN.load(Ordering::Relaxed),
            DEV.get().cloned(),
        )
    }

    /// [`Self::at_start`] with `--default-theme` and `--theme-dev` given.
    fn start_with(
        root: &Path,
        layout: &Layout,
        window: (u32, u32),
        default_this_run: bool,
        dev_folder: Option<PathBuf>,
    ) -> Self {
        // The theme `--default-theme` escapes isn't loaded at all: a theme
        // that hangs, crashes or breaks text while loading can't do so now.
        let live = if default_this_run {
            None
        } else {
            layout.theme.as_deref()
        };
        let mut themes = Self::read_live(root, layout, window, live);
        themes.dev_folder = dev_folder;
        if themes.dev_folder.is_some()
            && let Err(reason) = themes.read_dev(layout)
        {
            tracing::warn!("{reason}");
        }
        if default_this_run && let Some(chosen) = themes.chosen.take() {
            themes.previous = Some(chosen);
            themes.settle_live();
        }
        themes
    }

    /// The themes this folder offers, with only the theme `live` names loaded: its
    /// art, sounds, icons and faces. The others are read and checked only.
    fn read_live(root: &Path, layout: &Layout, window: (u32, u32), live: Option<&str>) -> Self {
        let mut custom = layout.custom_themes.clone();
        palette::tidy(&mut custom);
        let (read, refused) = install::Themes::open(root).installed_and_refused();
        let thumbnails = thumbnails(
            root,
            &read
                .iter()
                .map(|installed| {
                    (
                        installed.folder.as_path(),
                        installed.package.preview.as_slice(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let mut refused: Vec<_> = refused
            .into_iter()
            .map(|(id, reason)| Refused { id, reason })
            .collect();
        let installed = read
            .into_iter()
            .zip(thumbnails)
            .filter_map(|(installed, thumbnail)| {
                let id = format!("{INSTALLED}{}", installed.id);
                let live = live == Some(id.as_str());
                let previous = installed
                    .entry
                    .previous
                    .as_ref()
                    .map(|previous| earlier_version(&installed.folder, previous));
                let cost = if !installed.entry.cost.is_empty() {
                    installed.entry.cost.clone()
                } else {
                    check::Files::read(&installed.folder)
                        .ok()
                        .map(|files| check::check(&files, Strictness::Install).cost.text())
                        .unwrap_or_default()
                };
                match resolve_package(
                    ResolveSource {
                        id,
                        folder: installed.folder,
                        package: installed.package,
                        previous,
                        cost,
                        thumbnail: Some(thumbnail),
                    },
                    Mode::Dark,
                    layout,
                    window,
                    live,
                ) {
                    Ok(installed) => Some(installed),
                    Err(error) => {
                        tracing::warn!(
                            id = %installed.id,
                            reason = %error,
                            "an installed theme could not be resolved"
                        );
                        refused.push(Refused {
                            id: installed.id,
                            reason: error,
                        });
                        None
                    }
                }
            })
            .collect();
        let companions = custom
            .iter()
            .filter_map(|custom| {
                let named = custom.folder.as_deref()?;
                let named = PathBuf::from(named);
                let folder = if named.is_absolute() {
                    named
                } else {
                    root.join(named)
                };
                if custom.creator.is_some()
                    && let Err(reason) = regenerate_managed_creator(root, custom, &folder)
                {
                    tracing::warn!(id = %custom.id, %reason, "a missing managed creator theme could not be regenerated");
                }
                let result = check::Files::read(&folder).and_then(|mut files| {
                    // Creator folders already are the complete package
                    // derived from their saved Spec. Older custom folders
                    // are layers only: preserve their type/art/media, but
                    // merge the palette currently edited in layout.json on
                    // top, exactly as the Slint build's export/apply path does.
                    if custom.creator.is_none() {
                        let colours: Vec<_> = palette::Theme::TOKENS
                            .iter()
                            .enumerate()
                            .filter_map(|(index, token)| {
                                Some((*token, custom.theme.colour(index)?.argb()))
                            })
                            .collect();
                        files.manifest = package::export_with(
                            &custom.name,
                            "",
                            custom.base,
                            custom.theme.dark,
                            &colours,
                            Some(&files.manifest),
                        )?;
                    }
                    let report = check::check(&files, Strictness::Install);
                    if let Some(reason) = report.reason() {
                        return Err(reason.to_owned());
                    }
                    let package = report.package.expect("a passed check has its package");
                    resolve_package(
                        ResolveSource {
                            id: custom.id.clone(),
                            folder,
                            package,
                            previous: None,
                            cost: String::new(),
                            thumbnail: None,
                        },
                        Mode::Dark,
                        layout,
                        window,
                        live == Some(custom.id.as_str()),
                    )
                });
                match result {
                    Ok(companion) => Some(companion),
                    Err(reason) => {
                        tracing::warn!(id = %custom.id, %reason, "a custom theme package could not be resolved");
                        refused.push(Refused {
                            id: custom.id.clone(),
                            reason,
                        });
                        None
                    }
                }
            })
            .collect();
        Self {
            custom,
            installed,
            companions,
            refused,
            chosen: layout.theme.clone(),
            previous: layout.previous_theme.clone(),
            trial: None,
            creator_preview: None,
            dev: None,
            dev_seen: None,
            dev_off: false,
            dev_folder: None,
            glass_generation: Arc::default(),
            system_mode: Mode::Dark,
            window,
        }
    }

    /// Reads installed packages and resolves them again after their store
    /// changed. The live choice and Back target are deliberately retained:
    /// [`Self::choose`] keeps those directly as well as queuing their
    /// `layout.json` writes, so the `Layout` borrowed by the caller can
    /// briefly still contain the values from before that choice.
    pub fn refresh(&mut self, root: &Path, layout: &Layout) {
        let chosen = self.chosen.clone();
        let previous = self.previous.clone();
        let trial = self.trial.take();
        let creator_preview = self.creator_preview.take();
        let dev = self.dev.take();
        let (dev_seen, dev_off) = (self.dev_seen, self.dev_off);
        let dev_folder = self.dev_folder.take();
        let system_mode = self.system_mode;
        let mut refreshed = Self::read_live(root, layout, self.window, chosen.as_deref());
        refreshed.chosen = chosen;
        refreshed.previous = previous;
        refreshed.trial = trial;
        refreshed.creator_preview = creator_preview;
        refreshed.dev = dev;
        refreshed.dev_seen = dev_seen;
        refreshed.dev_off = dev_off;
        refreshed.dev_folder = dev_folder;
        refreshed.glass_generation = self.glass_generation.clone();
        *self = refreshed;
        self.system_mode_changed(system_mode);
        self.settle_live();
    }

    /// Loads the files of the installed and own themes showing (the chosen
    /// one, and one on trial) and lets every other's go, after what shows
    /// changed.
    fn settle_live(&mut self) {
        let mut showing: Vec<String> = self.chosen.iter().cloned().collect();
        if let Some(trial) = &self.trial
            && trial.reviewed.is_none()
        {
            showing.push(trial.id.clone());
        }
        let (window, system_mode) = (self.window, self.system_mode);
        for installed in self.installed.iter_mut().chain(&mut self.companions) {
            let live = showing.contains(&installed.id);
            if installed.live == live {
                continue;
            }
            installed.live = live;
            let choices = installed.choices.clone();
            let mode = selected_mode(&installed.package, &choices, system_mode);
            let Ok(resolved) = installed.package.resolve_with(mode, &choices) else {
                continue;
            };
            let (performance, reduced) = (installed.art.performance, installed.art.reduced_motion);
            apply_resolution(installed, resolved, choices, performance, reduced, window);
        }
    }

    /// Re-reads installed packages after their store changes. Creator saves
    /// should call [`Self::refresh`] directly because it also rebuilds custom
    /// package companions.
    pub fn refresh_installed(&mut self, root: &Path, layout: &Layout) {
        self.refresh(root, layout);
    }

    /// Installs checked repository bytes, then re-reads and resolves every
    /// installed package. Choosing the new theme remains the review UI's
    /// explicit next action, as in the Slint build.
    pub fn install_fetched(
        &mut self,
        root: &Path,
        layout: &Layout,
        fetched: &install::Fetched,
    ) -> Result<String, String> {
        install::Themes::open(root)
            .install(fetched)
            .map_err(|error| format!("{error:#}"))?;
        self.refresh_installed(root, layout);
        Ok(format!("{INSTALLED}{}", fetched.package().id))
    }

    /// Rolls an installed package back to the version retained by core,
    /// preserving its choice, options, content permission and sound
    /// preferences while resolving that earlier version again.
    ///
    /// `id` is the package id in `theme.json`, without [`INSTALLED`].
    pub fn roll_back(&mut self, root: &Path, layout: &Layout, id: &str) -> Result<(), String> {
        install::Themes::open(root)
            .roll_back(id)
            .map_err(|error| format!("{error:#}"))?;
        self.refresh_installed(root, layout);
        Ok(())
    }

    /// Removes an installed package. Its per-package permissions and
    /// choices go only after core has safely updated its lock. If it was
    /// showing, the Slint build's previous-theme rule is applied: restore a
    /// different Back target, or show the stock default with no target.
    ///
    /// Returns whether what shows changed: it was chosen or on trial. `id`
    /// is the package id in `theme.json`, without [`INSTALLED`].
    pub fn uninstall(
        &mut self,
        root: &Path,
        layout: &mut Layout,
        id: &str,
    ) -> Result<bool, String> {
        install::Themes::open(root)
            .uninstall(id)
            .map_err(|error| format!("{error:#}"))?;

        let layout_id = format!("{INSTALLED}{id}");
        // A theme on trial that goes ends its trial: what showed before it
        // shows again, as when the trial is reverted.
        let was_on_trial =
            self.trial.as_ref().map(|trial| trial.id.as_str()) == Some(layout_id.as_str());
        if was_on_trial {
            self.trial = None;
            layout.theme_sounds.remove("trial");
            layout.theme_content.remove("trial");
        }
        let was_chosen = self.chosen.as_deref() == Some(layout_id.as_str());
        let mut back = None;
        if was_chosen {
            back = self
                .previous
                .clone()
                .filter(|previous| previous != &layout_id);
            // Validate after the package list is refreshed: the old in-memory
            // list still contains the package core just removed.
            self.chosen = None;
            self.previous = None;
        }

        if layout.theme_options.remove(&layout_id).is_some() {
            layout::store(root, "theme_options", &layout.theme_options);
        }
        if layout.theme_sounds.remove(&layout_id).is_some() {
            layout::store(root, "theme_sounds", &layout.theme_sounds);
        }
        if layout.theme_content.remove(&layout_id) {
            layout::store(root, "theme_content", &layout.theme_content);
        }
        self.refresh_installed(root, layout);
        if was_chosen {
            back = back.filter(|id| self.find(id).is_some());
            self.chosen.clone_from(&back);
            layout.theme = back;
            layout.previous_theme = None;
            layout::store(root, "theme", &layout.theme);
            layout::store(root, "previous_theme", &layout.previous_theme);
        }
        if was_chosen || was_on_trial {
            self.settle_live();
        }
        Ok(was_chosen || was_on_trial)
    }

    /// The theme to show.
    pub fn shown(&self) -> Theme {
        if let Some((dev, _)) = &self.dev {
            return dev.shown.clone();
        }
        if let Some(theme) = self
            .trial
            .as_ref()
            .and_then(|trial| self.package(&trial.id))
            .map(|package| package.shown.clone())
        {
            return theme;
        }
        if let Some(theme) = self
            .chosen
            .as_deref()
            .and_then(|id| self.package(id).map(|package| package.shown.clone()))
        {
            return theme;
        }
        let (id, found) = palette::chosen(self.chosen.as_deref().or(Some(DEFAULT)), &self.custom);
        theme(id, &found)
    }

    /// The chosen theme's id, as Appearance and Themes mark it.
    pub fn chosen_id(&self) -> &str {
        if let Some(trial) = &self.trial {
            return &trial.id;
        }
        if let Some(installed) = self.chosen.as_deref().and_then(|id| self.package(id)) {
            return &installed.id;
        }
        palette::chosen(self.chosen.as_deref().or(Some(DEFAULT)), &self.custom).0
    }

    /// The theme Settings → Themes' Back action would restore, and its
    /// display name. A missing package or deleted custom theme is not
    /// offered, nor is the theme already shown.
    pub fn previous(&self) -> Option<(&str, &str)> {
        let id = self
            .previous
            .as_deref()
            .filter(|previous| self.chosen.as_deref() != Some(*previous))?;
        let name = self
            .installed
            .iter()
            .find(|installed| installed.id == id)
            .map(|installed| installed.name.as_str())
            .or_else(|| palette::find(id).map(|theme| theme.name))
            .or_else(|| {
                self.custom
                    .iter()
                    .find(|custom| custom.id == id)
                    .map(|custom| custom.name.as_str())
            })?;
        Some((id, name))
    }

    /// `id`'s theme, if it's one Yapfast has or the person made.
    pub fn find(&self, id: &str) -> Option<Theme> {
        self.package(id)
            .map(|package| package.shown.clone())
            .or_else(|| palette::lookup(id, &self.custom).map(|(id, found)| theme(id, &found)))
    }

    /// Resolves a checked creator draft into the ordinary package registry
    /// without choosing or persisting it. All package consumers can read it
    /// through [`CREATOR_PREVIEW_ID`] until [`Self::clear_creator_preview`].
    pub fn preview_creator(&mut self, folder: &Path, layout: &Layout) -> Result<Theme, String> {
        self.clear_creator_preview();
        let files = check::Files::read(folder)?;
        let report = check::check(&files, Strictness::Install);
        if let Some(reason) = report.reason() {
            return Err(reason.to_owned());
        }
        let package = report.package.expect("a passed check has its package");
        let resolved = resolve_package(
            ResolveSource {
                id: CREATOR_PREVIEW_ID.to_owned(),
                folder: folder.to_owned(),
                package,
                previous: None,
                cost: report.cost.text(),
                thumbnail: None,
            },
            self.system_mode,
            layout,
            self.window,
            true,
        )?;
        let shown = resolved.shown.clone();
        self.creator_preview = Some(resolved);
        Ok(shown)
    }

    /// Whether `--theme-dev` watches a folder: it was given and Ctrl+Shift+T
    /// hasn't turned it off.
    #[cfg(test)]
    pub fn developing(&self) -> bool {
        self.dev_folder().is_some()
    }

    /// `--theme-dev`'s folder while it's watched.
    pub fn dev_folder(&self) -> Option<&Path> {
        self.dev_folder.as_deref().filter(|_| !self.dev_off)
    }

    /// Reads `--theme-dev`'s folder again if a file in it changed since,
    /// checked as an install checks; whether the look changed. What fails
    /// the checks is printed and the look before stays.
    pub fn dev_saved(&mut self, layout: &Layout) -> bool {
        let Some(folder) = self.dev_folder() else {
            return false;
        };
        let stamp = newest(folder);
        if self.dev_seen == Some(stamp) {
            return false;
        }
        match self.read_dev(layout) {
            Ok(()) => true,
            Err(reason) => {
                tracing::warn!("{reason}");
                false
            }
        }
    }

    fn read_dev(&mut self, layout: &Layout) -> Result<(), String> {
        let folder = self.dev_folder().ok_or("no --theme-dev folder")?.to_owned();
        let stamp = newest(&folder);
        // Not read again until the next save, whether it passes or not.
        self.dev_seen = Some(stamp);
        let files = check::Files::read(&folder)?;
        let report = check::check(&files, Strictness::Install);
        if !report.passed() {
            return Err(format!(
                "--theme-dev: {} doesn't pass its checks, so the look before stays:\n{}",
                folder.display(),
                report.text()
            ));
        }
        let package = report.package.expect("a passed check has its package");
        let resolved = resolve_package(
            ResolveSource {
                id: DEV_ID.to_owned(),
                folder: folder.clone(),
                package,
                previous: None,
                cost: report.cost.text(),
                thumbnail: None,
            },
            self.system_mode,
            layout,
            self.window,
            true,
        )?;
        tracing::info!("--theme-dev: showing {} as saved", folder.display());
        self.dev = Some((resolved, stamp));
        Ok(())
    }

    pub fn clear_creator_preview(&mut self) {
        self.creator_preview = None;
        Theme::unregister_metrics(CREATOR_PREVIEW_ID);
    }

    /// Removes one custom theme from live theme state and repairs choice
    /// history before its id can be reused. An active deletion restores a
    /// still-valid Back target, or Yap when there is none; the deleted id is
    /// never retained as `previous_theme`.
    pub fn forget_custom(&mut self, root: &Path, id: &str) -> bool {
        self.custom.retain(|custom| custom.id != id);
        self.companions.retain(|companion| companion.id != id);
        Theme::unregister_metrics(id);
        if self.trial.as_ref().is_some_and(|trial| trial.id == id) {
            self.trial = None;
        }

        let active = self.chosen.as_deref() == Some(id);
        if active {
            let fallback = self
                .previous
                .take()
                .filter(|previous| previous != id && self.find(previous).is_some())
                .unwrap_or_else(|| "yap".to_owned());
            self.chosen = Some(fallback);
            layout::store(root, "theme", &self.chosen);
            layout::store(root, "previous_theme", Option::<String>::None);
        } else if self.previous.as_deref() == Some(id) {
            self.previous = None;
            layout::store(root, "previous_theme", Option::<String>::None);
        }
        self.settle_live();
        active
    }

    /// Keeps `id` as the chosen theme in `layout.json`, leaving the rest of
    /// what's kept there as it is.
    pub fn choose(&mut self, root: &Path, id: &str) {
        self.trial = None;
        // Re-choosing the theme already shown must not lose the useful Back
        // target. Every actual change remembers what it replaced, exactly
        // as the Slint build did.
        if self.chosen.as_deref() != Some(id) {
            self.previous = self.chosen.take();
            layout::store(root, "previous_theme", &self.previous);
        }
        self.chosen = Some(id.to_owned());
        layout::store(root, "theme", id);
        self.settle_live();
    }

    /// Turns a package theme off. With no theme kept, the stock default is
    /// shown at once and at the next start; Back can restore the theme that
    /// was showing.
    pub fn turn_off(&mut self, root: &Path) {
        self.trial = None;
        // The emergency key is a way back from `--theme-dev`'s theme too: it
        // stops showing and watching the folder for the rest of this run.
        if self.dev_folder.is_some() {
            self.dev = None;
            self.dev_off = true;
            Theme::unregister_metrics(DEV_ID);
        }
        if let Some(shown) = self.chosen.take() {
            self.previous = Some(shown);
            layout::store(root, "previous_theme", &self.previous);
        }
        layout::store(root, "theme", Option::<String>::None);
        self.settle_live();
    }

    /// Restores Settings → Themes' Back target. Going back between two
    /// chosen themes swaps them; restoring after Turn off clears Back, as
    /// in the Slint build.
    pub fn revert(&mut self, root: &Path) -> bool {
        self.trial = None;
        let Some(previous) = self.previous().map(|(id, _)| id.to_owned()) else {
            return false;
        };
        self.choose(root, &previous);
        true
    }

    /// Shows an installed package for 30 seconds without changing
    /// `layout.json`.
    pub fn try_installed(&mut self, id: &str) -> Result<(), String> {
        let package = self
            .installed
            .iter()
            .find(|package| package.id == id)
            .ok_or_else(|| "that installed theme isn't available".to_owned())?;
        let name = package.name.clone();
        let back_to = self
            .trial
            .take()
            .and_then(|trial| trial.back_to)
            .or_else(|| self.chosen.clone());
        self.trial = Some(ThemeTrial {
            id: id.to_owned(),
            name,
            back_to,
            until: Instant::now() + Duration::from_secs(TRIAL_SECONDS),
            reviewed: None,
        });
        self.settle_live();
        Ok(())
    }

    /// Shows checked review bytes for 30 seconds without installing them.
    /// Their files live only in a private temporary folder held by the
    /// trial, so fonts and art remain available until Keep or Revert.
    pub fn try_reviewed(
        &mut self,
        fetched: &install::Fetched,
        layout: &Layout,
    ) -> Result<(), String> {
        let folder = tempfile::Builder::new()
            .prefix(TRIAL_FOLDER)
            .tempdir()
            .map_err(|error| error.to_string())?;
        std::fs::write(
            folder.path().join(yapfast_core::themes::MANIFEST),
            &fetched.files.manifest,
        )
        .map_err(|error| error.to_string())?;
        for (relative, bytes) in &fetched.files.files {
            let target = folder.path().join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(target, bytes).map_err(|error| error.to_string())?;
        }
        if let Some((relative, bytes)) = &fetched.files.licence {
            std::fs::write(folder.path().join(relative), bytes)
                .map_err(|error| error.to_string())?;
        }

        let name = fetched.package().name.clone();
        let package = resolve_package(
            ResolveSource {
                id: TRIAL_ID.to_owned(),
                folder: folder.path().to_owned(),
                package: fetched.package().clone(),
                previous: None,
                cost: fetched.report.cost.text(),
                thumbnail: None,
            },
            self.system_mode,
            layout,
            self.window,
            true,
        )?;
        let back_to = self
            .trial
            .take()
            .and_then(|trial| trial.back_to)
            .or_else(|| self.chosen.clone());
        self.trial = Some(ThemeTrial {
            id: TRIAL_ID.to_owned(),
            name,
            back_to,
            until: Instant::now() + Duration::from_secs(TRIAL_SECONDS),
            reviewed: Some(ReviewedTrial {
                fetched: fetched.clone(),
                _folder: folder,
                package,
            }),
        });
        Ok(())
    }

    /// Whether the theme on trial came from a review, not installed.
    pub fn trial_reviewed(&self) -> bool {
        self.trial
            .as_ref()
            .is_some_and(|trial| trial.reviewed.is_some())
    }

    pub fn trial(&self) -> Option<TrialInfo> {
        let trial = self.trial.as_ref()?;
        Some(TrialInfo {
            name: trial.name.clone(),
            seconds: trial
                .until
                .saturating_duration_since(Instant::now())
                .as_secs_f32()
                .ceil() as u64,
        })
    }

    /// Keeps the current trial. Reviewed bytes are installed only here;
    /// callers then choose the returned layout id through the normal path.
    pub fn keep_trial(&mut self, root: &Path, layout: &Layout) -> Result<Option<String>, String> {
        let Some(trial) = self.trial.take() else {
            return Ok(None);
        };
        if let Some(reviewed) = &trial.reviewed {
            match self.install_fetched(root, layout, &reviewed.fetched) {
                Ok(id) => Ok(Some(id)),
                Err(error) => {
                    self.trial = Some(trial);
                    Err(error)
                }
            }
        } else {
            Ok(Some(trial.id))
        }
    }

    /// Reverts the current trial and returns the layout id that was showing
    /// before it. `None` inside `Some` means the stock default.
    pub fn revert_trial(&mut self) -> Option<Option<String>> {
        let back_to = self.trial.take().map(|trial| trial.back_to);
        self.settle_live();
        back_to
    }

    /// Reverts an expired trial. Settings calls this from its countdown
    /// tick; accepting `now` keeps expiry deterministic in tests.
    pub fn expire_trial(&mut self, now: Instant) -> Option<Option<String>> {
        if self.trial.as_ref().is_some_and(|trial| now >= trial.until) {
            self.revert_trial()
        } else {
            None
        }
    }

    /// Re-resolves live dual-variant packages whose Light or dark choice is
    /// Auto. Explicit Dark/Light choices and the persisted theme id remain
    /// untouched.
    pub fn system_mode_changed(&mut self, mode: Mode) -> bool {
        if self.system_mode == mode {
            return false;
        }
        self.system_mode = mode;
        let window = self.window;
        let mut changed = false;
        for installed in self.packages_mut() {
            if installed.modes.len() < 2
                || matches!(
                    installed
                        .choices
                        .get(options::MODE)
                        .and_then(serde_json::Value::as_str),
                    Some("dark" | "light")
                )
            {
                continue;
            }
            let choices = installed.choices.clone();
            let performance = installed.art.performance;
            let reduced = installed.art.reduced_motion;
            let selected = installed.package.mode_for(mode);
            let Ok(resolved) = installed.package.resolve_with(selected, &choices) else {
                continue;
            };
            apply_resolution(installed, resolved, choices, performance, reduced, window);
            changed = true;
        }
        changed
    }

    /// Rebakes fake-glass backings for the current window pixels. Ordinary
    /// package art is size-independent and is left intact.
    pub fn resized(&mut self, window: (u32, u32)) -> bool {
        if self.window == window {
            return false;
        }
        self.window = window;
        let mut changed = false;
        for installed in self.packages_mut() {
            let Some(pane) = &mut installed.art.glass else {
                continue;
            };
            // The blur there is shows stretched until the new one is baked.
            pane.blurred = pane.blurred.stretched_to(glass_window(window));
            installed.metrics.tooltip = tooltip_surface(&installed.art);
            installed.shown.register_metrics(installed.metrics.clone());
            changed = true;
        }
        if changed {
            self.glass_generation.fetch_add(1, Ordering::Relaxed);
        }
        changed
    }

    /// What the last resize asks baked: each loaded glass theme's art at
    /// the window's size, with the generation that asks and the counter a
    /// later resize moves on. Nothing is read from disk.
    pub fn glass_jobs(
        &mut self,
    ) -> Option<(Vec<GlassJob>, u64, Arc<std::sync::atomic::AtomicU64>)> {
        let window = self.window;
        let jobs: Vec<GlassJob> = self
            .packages_mut()
            .filter_map(|installed| {
                let pane = installed.art.glass.as_ref()?;
                Some(GlassJob {
                    id: installed.id.clone(),
                    source: pane.source.clone(),
                    window,
                })
            })
            .collect();
        if jobs.is_empty() {
            return None;
        }
        Some((
            jobs,
            self.glass_generation.load(Ordering::Relaxed),
            self.glass_generation.clone(),
        ))
    }

    /// Takes glass a resize baked, unless a later resize has asked again.
    /// Whether anything changed.
    pub fn glass_baked(&mut self, baked: GlassBaked) -> bool {
        if baked.generation != self.glass_generation.load(Ordering::Relaxed) {
            return false;
        }
        let mut changed = false;
        for (id, blurred) in baked.panes {
            let Some(installed) = self.packages_mut().find(|installed| installed.id == id) else {
                continue;
            };
            let Some(pane) = &mut installed.art.glass else {
                continue;
            };
            pane.blurred = blurred;
            installed.metrics.tooltip = tooltip_surface(&installed.art);
            installed.shown.register_metrics(installed.metrics.clone());
            changed = true;
        }
        changed
    }

    /// Reworks each installed theme's popup motion after Themes changes
    /// Performance or Reduce motion, without touching the persisted choice.
    pub fn motion_changed(&mut self, performance: Performance, reduced: bool) {
        for installed in self.packages_mut() {
            installed.metrics.motion = motion(&installed.motion, performance, reduced);
            installed.shown.register_metrics(installed.metrics.clone());
            installed.art.performance = performance;
            installed.art.reduced_motion = reduced;
        }
    }

    /// Applies Appearance's Readable text choice to every package's
    /// companion typography without re-resolving unrelated theme data.
    pub fn readable_changed(&mut self, readable: bool) {
        for installed in self.packages_mut() {
            installed.metrics.readable_text = readable;
            installed.shown.register_metrics(installed.metrics.clone());
        }
    }

    /// The active theme's checked replacement SVGs. Built-in and custom
    /// colour themes have no icon pack.
    pub fn icons(&self, id: &str) -> HashMap<String, yapfast_iced_kit::icons::Replacement> {
        self.package(id)
            .map_or_else(HashMap::new, |installed| installed.icons.clone())
    }

    /// Makes the active package's replacements the ones the core plays.
    /// Built-in and custom colour themes restore Yapfast's own sounds.
    pub fn apply_sounds(&self, id: &str) {
        let installed = self.package(id);
        let sounds = installed.map(|installed| &installed.sounds);
        let notifications = installed.is_some_and(|installed| installed.sound_prefs.on);
        sounds::set_themed(if notifications {
            sounds.map_or_else(HashMap::new, |sounds| sounds.app.clone())
        } else {
            HashMap::new()
        });
        ring::set_themed(
            ring::Tone::Incoming,
            notifications
                .then(|| {
                    sounds.and_then(|sounds| sounds.rings.get(&media::SoundSlot::Ring).cloned())
                })
                .flatten(),
        );
        ring::set_themed(
            ring::Tone::Outgoing,
            notifications
                .then(|| {
                    sounds.and_then(|sounds| sounds.rings.get(&media::SoundSlot::Ringback).cloned())
                })
                .flatten(),
        );
        self.update_cue_sounds(id);
    }

    pub fn sound_prefs(&self, id: &str) -> Option<SoundPrefs> {
        self.package(id).map(|installed| installed.sound_prefs)
    }

    pub fn set_sound_prefs(&mut self, id: &str, prefs: SoundPrefs) -> bool {
        let Some(installed) = self.package_mut(id) else {
            return false;
        };
        installed.sound_prefs = prefs;
        let cues_on = prefs.interface && prefs.volume > 0;
        installed.metrics.hover_cue =
            cues_on && installed.sounds.cues.contains_key(&media::SoundSlot::Hover);
        installed.metrics.press_cue =
            cues_on && installed.sounds.cues.contains_key(&media::SoundSlot::Press);
        installed.shown.register_metrics(installed.metrics.clone());
        true
    }

    pub fn has_sounds(&self, id: &str) -> Option<(bool, bool)> {
        let sounds = &self.package(id)?.sounds;
        Some((
            !sounds.app.is_empty() || !sounds.rings.is_empty(),
            !sounds.cues.is_empty(),
        ))
    }

    /// One active package sound for an interface event. The caller decides
    /// whether the window is focused and which speakers to use.
    pub fn cue(&self, id: &str, slot: media::SoundSlot) -> Option<(Arc<[i16]>, f32)> {
        let cue = self.package(id)?;
        if !cue.sound_prefs.interface {
            return None;
        }
        let volume = cue.sound_prefs.volume.min(100) as f32 / 100.0;
        let cue = cue.sounds.cues.get(&slot)?;
        let index = NEXT_CUE.fetch_add(1, Ordering::Relaxed) % cue.samples.len();
        Some((cue.samples[index].clone(), cue.volume * volume))
    }

    fn update_cue_sounds(&self, id: &str) {
        let installed = self.package(id);
        let (hover, press) =
            match installed.filter(|p| p.sound_prefs.interface && p.sound_prefs.volume > 0) {
                Some(p) => {
                    let scale = p.sound_prefs.volume.min(100) as f32 / 100.0;
                    let pick = |slot| {
                        p.sounds.cues.get(&slot).map(|cue| SoundCue {
                            samples: cue.samples.clone(),
                            volume: cue.volume * scale,
                        })
                    };
                    (pick(media::SoundSlot::Hover), pick(media::SoundSlot::Press))
                }
                None => (None, None),
            };
        if let Ok(mut rt) = cue_runtime().lock() {
            rt.hover = hover;
            rt.press = press;
        }
    }

    /// Brings the interface cues in line with the shown theme, the focus
    /// and Do Not Disturb, and the speakers. It runs after every message, so
    /// it touches nothing unless one of those changed.
    pub fn sync_cue_runtime(&self, id: &str, allowed: bool, speaker: Option<&str>) {
        let prefs = self.package(id).map(|installed| installed.sound_prefs);
        {
            let Ok(mut rt) = cue_runtime().lock() else {
                return;
            };
            rt.allowed = allowed;
            if rt.speaker.as_deref() != speaker {
                rt.speaker = speaker.map(str::to_owned);
            }
            if rt
                .synced
                .as_ref()
                .is_some_and(|(was, was_prefs)| was == id && *was_prefs == prefs)
            {
                return;
            }
            rt.synced = Some((id.to_owned(), prefs));
        }
        self.update_cue_sounds(id);
    }

    pub fn animations(&self, id: &str) -> &[package::Animation] {
        self.package(id)
            .map_or(&[], |installed| installed.animations.as_slice())
    }

    pub fn unfocused(&self, id: &str) -> package::Unfocused {
        self.package(id)
            .and_then(|installed| installed.motion.unfocused)
            .unwrap_or_default()
    }

    pub fn chrome(&self, id: &str) -> Option<&chrome::Chrome> {
        self.package(id).map(|installed| &installed.chrome)
    }

    /// A relabelled piece of chrome, or Wired's exact words when the
    /// active package does not name it.
    pub fn label(
        &self,
        id: &str,
        label: chrome::Label,
        stock: impl Into<String>,
        values: &[(&str, &str)],
    ) -> String {
        self.chrome(id)
            .and_then(|chrome| chrome.labels.get(&label))
            .map_or_else(|| stock.into(), |template| chrome::fill(template, values))
    }

    pub fn marked(&self, id: &str, place: chrome::Place, stock: &str) -> String {
        let Some(chrome) = self.chrome(id) else {
            return stock.to_owned();
        };
        format!(
            "{}{}{}",
            chrome.prefix.get(&place).map_or("", String::as_str),
            stock,
            chrome.suffix.get(&place).map_or("", String::as_str)
        )
    }

    pub fn marked_time(&self, id: &str, stock: &str) -> String {
        self.chrome(id)
            .and_then(|chrome| chrome.time.as_ref())
            .map_or_else(
                || stock.to_owned(),
                |(before, after)| format!("{before}{stock}{after}"),
            )
    }

    pub fn content(&self, id: &str, allowed: bool) -> Option<&content::Content> {
        allowed
            .then(|| self.package(id).map(|package| &package.content))
            .flatten()
    }

    pub fn content_phrase(&self, id: &str) -> Option<String> {
        self.package(id)?.content.phrase()
    }

    /// The pictures on a bubble, yours (`own`) or someone else's.
    pub fn bubble_art(&self, id: &str, own: bool) -> &[ArtLayer] {
        self.package(id).map_or(&[], |installed| {
            installed.art.bubble_art[usize::from(!own)].as_slice()
        })
    }

    pub fn art(&self, id: &str, surface: ArtSurface) -> &[ArtLayer] {
        self.package(id)
            .and_then(|installed| installed.art.surfaces.get(&surface))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether an installed theme places art or glass on this surface.
    /// Callers use this only to make their stock opaque fill transparent so
    /// the shared surface renderer can actually be seen underneath.
    /// Whether the composer draws keys under its buttons (Bubblegum's
    /// candy keys): keys of their own, or a composer layer that sits at the
    /// right.
    pub fn composer_keys(&self, id: &str) -> bool {
        self.package(id).is_some_and(|installed| {
            !installed.art.keys.is_empty()
                || installed
                    .art
                    .surfaces
                    .get(&ArtSurface::Composer)
                    .is_some_and(|layers| layers.iter().any(|layer| layer.layer.position.0 >= 1.0))
        })
    }

    /// The key a theme draws under one of the composer's buttons.
    pub fn composer_key(&self, id: &str, slot: art::KeySlot) -> Option<&ComposerKey> {
        self.package(id)?.art.keys.get(&slot)
    }

    pub fn decorates(&self, id: &str, surface: ArtSurface) -> bool {
        self.package(id).is_some_and(|installed| {
            installed
                .art
                .surfaces
                .get(&surface)
                .is_some_and(|layers| !layers.is_empty())
                || installed
                    .art
                    .glass
                    .as_ref()
                    .is_some_and(|glass| glass.style.surfaces.contains(&surface))
        })
    }

    pub fn overlays(&self, id: &str, chat: bool) -> &[ArtLayer] {
        self.package(id).map_or(&[], |installed| {
            if chat {
                installed.art.chat_overlays.as_slice()
            } else {
                installed.art.overlays.as_slice()
            }
        })
    }

    pub fn procedural_overlays(&self, id: &str, chat: bool) -> &[art::Overlay] {
        self.package(id).map_or(&[], |installed| {
            if chat {
                installed.art.chat_procedural_overlays.as_slice()
            } else {
                installed.art.procedural_overlays.as_slice()
            }
        })
    }

    pub fn art_reserve(&self, id: &str, surface: ArtSurface) -> Option<art::Reserve> {
        self.package(id)?.art.reserves.get(&surface).copied()
    }

    /// Height reserved at the member list's foot for the package's Footer
    /// art. A footer with pictures and no explicit height uses the Slint build's
    /// 64 px default.
    pub fn footer_height(&self, id: &str) -> Option<f32> {
        self.package(id)?.art.footer_height
    }

    pub fn art_motion(&self, id: &str) -> (Performance, bool) {
        self.package(id)
            .map_or((Performance::Light, false), |installed| {
                (installed.art.performance, installed.art.reduced_motion)
            })
    }

    /// The active package's ransom-note names, when it cuts them.
    pub fn ransom(&self, id: &str) -> Option<&Ransom> {
        self.package(id)?.art.ransom.as_ref()
    }

    /// The active package's glass, whichever surfaces it covers.
    pub fn glass_style(&self, id: &str) -> Option<&art::Glass> {
        Some(&self.package(id)?.art.glass.as_ref()?.style)
    }

    /// The room between floating glass panes, 0 when they don't float.
    pub fn glass_gap(&self, id: &str) -> f32 {
        self.glass_style(id).map_or(0.0, |style| style.gap.max(0.0))
    }

    pub fn glass(&self, id: &str, surface: ArtSurface) -> Option<&GlassPane> {
        let pane = self.package(id)?.art.glass.as_ref()?;
        pane.style.surfaces.contains(&surface).then_some(pane)
    }

    pub fn text_look(&self, id: &str, role: art::TextRole) -> Option<&art::TextLook> {
        self.package(id)?.art.text.get(&role)
    }

    pub fn event_effect(&self, id: &str, slot: art::EventSlot) -> Option<art::Event> {
        self.package(id)?.art.events.get(&slot).copied()
    }

    pub fn foil(&self, id: &str) -> Option<&art::Foil> {
        self.package(id)?.art.foil.as_ref()
    }

    pub fn particles(&self, id: &str) -> Option<ParticleView<'_>> {
        let art = &self.package(id)?.art;
        (!art.particles.is_empty()).then_some((
            art.particles.as_slice(),
            &art.particle_events,
            art.performance,
            art.reduced_motion,
        ))
    }

    pub fn particle_event(&self, id: &str, trigger: art::Trigger, origin: Option<(f32, f32)>) {
        let Some((sets, events, performance, reduced)) = self.particles(id) else {
            return;
        };
        if reduced
            || performance == Performance::Light
            || !sets.iter().any(|set| {
                set.spec.on == trigger && (performance == Performance::Full || !set.spec.heavy)
            })
        {
            return;
        }
        if let Ok(mut events) = events.lock() {
            let now = Instant::now();
            events.retain(|event| now.duration_since(event.started) < Duration::from_secs(12));
            events.push(ParticleEvent {
                trigger,
                started: now,
                origin,
            });
        }
    }

    pub fn list_art(
        &self,
        id: &str,
        tile: &yapfast_core::themes::lists::TileArt,
        surface: ArtSurface,
    ) -> Option<&TileArtImage> {
        self.package(id)?
            .art
            .list_art
            .get(&tile_art_key(tile, surface))
    }

    /// The tape cut out of a card's top-edge picture, to draw over the
    /// card across its torn edge ([`card_tape`]).
    pub fn card_tape(
        &self,
        id: &str,
        tile: &yapfast_core::themes::lists::TileArt,
    ) -> Option<&yapfast_core::themes::lists::TileArt> {
        self.package(id)?
            .art
            .card_tapes
            .get(&tile_art_key(tile, ArtSurface::Chat))
    }

    pub fn package_layout(&self, id: &str) -> Option<&theme_layout::Layout> {
        self.package(id).map(|installed| &installed.layout)
    }

    pub fn channel_rows(&self, id: &str) -> Option<&yapfast_core::themes::lists::ChannelRows> {
        self.package_layout(id)?.channel_rows.as_ref()
    }

    pub fn rail_tiles(&self, id: &str) -> Option<&yapfast_core::themes::lists::RailTiles> {
        self.package_layout(id)?.rail_tiles.as_ref()
    }

    pub fn member_rows(&self, id: &str) -> Option<&yapfast_core::themes::lists::MemberRows> {
        self.package_layout(id)?.member_rows.as_ref()
    }

    /// Changes one installed theme option and resolves the package again.
    /// The normalized choices are returned for the caller to keep in
    /// `layout.json`; an invalid id or value changes nothing.
    pub fn set_option(
        &mut self,
        id: &str,
        option_id: &str,
        value: serde_json::Value,
        performance: Performance,
        reduced: bool,
    ) -> Option<options::Choices> {
        let window = self.window;
        let system_mode = self.system_mode;
        let installed = self.package_mut(id)?;
        let option = installed
            .options
            .iter()
            .find(|option| option.id == option_id)?;
        let mut choices = installed.choices.clone();
        choices.insert(option_id.to_owned(), value);
        let value = option.value(&choices);
        if value == option.default_value() {
            choices.remove(option_id);
        } else {
            choices.insert(option_id.to_owned(), value);
        }
        let mode = selected_mode(&installed.package, &choices, system_mode);
        let resolved = installed.package.resolve_with(mode, &choices).ok()?;
        apply_resolution(
            installed,
            resolved,
            choices.clone(),
            performance,
            reduced,
            window,
        );
        Some(choices)
    }
}

fn apply_resolution(
    installed: &mut Installed,
    resolved: Resolved,
    choices: options::Choices,
    performance: Performance,
    reduced: bool,
    window: (u32, u32),
) {
    let palette = resolved_palette(&resolved);
    let shown = theme(&installed.id, &palette);
    let readable = installed.metrics.readable_text;
    let mut metrics = metrics(&resolved, performance, reduced);
    metrics.readable_text = readable;
    let Loaded {
        types,
        icons,
        sounds,
        art,
    } = load(
        installed.live,
        &installed.folder,
        &resolved,
        &palette,
        &installed.package.options,
        &choices,
        (performance, reduced),
        window,
    );
    metrics.types = types;
    metrics.decorations = art.decorations();
    metrics.tooltip = tooltip_surface(&art);
    let cues_on = installed.sound_prefs.interface && installed.sound_prefs.volume > 0;
    metrics.hover_cue = cues_on && sounds.cues.contains_key(&media::SoundSlot::Hover);
    metrics.press_cue = cues_on && sounds.cues.contains_key(&media::SoundSlot::Press);
    shown.register_metrics(metrics.clone());
    installed.shown = shown;
    installed.metrics = metrics;
    installed.palette_preview = palette;
    installed.motion = resolved.motion;
    installed.animations = resolved.animations;
    installed.icons = icons;
    installed.sounds = sounds;
    installed.chrome = resolved.chrome;
    installed.content = resolved.content;
    installed.art = art;
    installed.layout = resolved.layout;
    installed.choices = choices;
}

fn theme_art(
    folder: &Path,
    resolved: &Resolved,
    performance: Performance,
    reduced_motion: bool,
    window: (u32, u32),
) -> ThemeArt {
    let palette = resolved_palette(resolved);
    let mut out = ThemeArt {
        performance,
        reduced_motion,
        text: resolved.art.text.clone(),
        events: resolved.art.events.clone(),
        foil: resolved.art.foil.clone(),
        ransom: ransom(folder, resolved),
        transition: resolved.art.transition,
        ..ThemeArt::default()
    };
    for (index, layers) in [
        &resolved.layout.bubbles.own_art,
        &resolved.layout.bubbles.other_art,
    ]
    .into_iter()
    .enumerate()
    {
        out.bubble_art[index] = layers
            .iter()
            .filter_map(|layer| {
                art_layer(
                    folder,
                    layer,
                    surface_ground(resolved, &palette, ArtSurface::Chat),
                    false,
                )
            })
            .collect();
    }
    for (&slot, key) in &resolved.art.keys {
        if let Some(key) = composer_key(folder, key) {
            out.keys.insert(slot, key);
        }
    }
    for (&surface, layers) in &resolved.art.surfaces {
        let loaded: Vec<_> = layers
            .iter()
            .filter_map(|layer| {
                art_layer(
                    folder,
                    layer,
                    surface_ground(resolved, &palette, surface),
                    surface == ArtSurface::Tooltip,
                )
            })
            .collect();
        if !loaded.is_empty() {
            out.surfaces.insert(surface, loaded);
            if surface == ArtSurface::Footer {
                out.footer_height = Some(resolved.art.footer_height.unwrap_or(64.0));
            }
            if let Some(reserve) = resolved.art.reserve(surface) {
                out.reserves.insert(surface, reserve);
            }
        }
    }
    for overlay in &resolved.art.overlays {
        if overlay.heavy && performance == Performance::Light {
            continue;
        }
        if overlay.kind == art::OverlayKind::Image {
            if let Some(layer) = overlay.layer.as_ref()
                && let Some(layer) = art_layer(
                    folder,
                    layer,
                    surface_ground(resolved, &palette, ArtSurface::Window),
                    false,
                )
            {
                if overlay.chat {
                    out.chat_overlays.push(layer);
                } else {
                    out.overlays.push(layer);
                }
            }
        } else if overlay.chat {
            out.chat_procedural_overlays.push(overlay.clone());
        } else {
            out.procedural_overlays.push(overlay.clone());
        }
    }
    out.glass = resolved
        .art
        .glass
        .as_ref()
        .map(|glass| glass_pane(folder, resolved, &palette, glass, window));
    out.particles = resolved
        .art
        .particles
        .iter()
        .map(|spec| ThemeParticles {
            sprite: spec.sprite.as_ref().and_then(|file| {
                let picture = yapfast_core::chat::decode_picture_at(&folder.join(file), 256, 256)?;
                Some(rgba_handle(picture_rgba(&picture)))
            }),
            spec: spec.clone(),
        })
        .collect();
    for (surface, tiles) in [
        (
            ArtSurface::Rail,
            yapfast_core::themes::lists::files(resolved.layout.rail_tiles.as_ref(), None, None),
        ),
        (
            ArtSurface::Sidebar,
            yapfast_core::themes::lists::files(None, resolved.layout.channel_rows.as_ref(), None),
        ),
        (
            ArtSurface::Members,
            yapfast_core::themes::lists::files(None, None, resolved.layout.member_rows.as_ref()),
        ),
    ] {
        let ground = surface_ground(resolved, &palette, surface);
        for tile in tiles {
            if let Some(image) = tile_art_image(folder, tile, ground) {
                out.list_art.insert(tile_art_key(tile, surface), image);
            }
        }
    }
    let card_ground = resolved
        .layout
        .cards
        .ground
        .unwrap_or_else(|| surface_ground(resolved, &palette, ArtSurface::Card));
    for (bottom, tiles) in card_tiles(&resolved.layout.cards) {
        for tile in tiles {
            let key = tile_art_key(&tile, ArtSurface::Chat);
            if let Some(split) = (!bottom)
                .then(|| card_tape(folder, &tile, card_ground))
                .flatten()
            {
                out.list_art.insert(key.clone(), split.untaped);
                out.list_art
                    .insert(tile_art_key(&split.tape.art, ArtSurface::Chat), split.tape);
                out.card_tapes.insert(key, split.tape_art);
                continue;
            }
            if let Some(image) = tile_art_image(folder, &tile, card_ground) {
                out.list_art.insert(key, image);
            }
        }
    }
    out
}

/// A card's top-edge picture and the tape cut out of it.
struct CardTape {
    untaped: TileArtImage,
    tape: TileArtImage,
    tape_art: yapfast_core::themes::lists::TileArt,
}

/// The tape painted on a card's top-edge picture (Zine's torn tops), cut
/// out to lie across the torn edge: half on the board above the card and
/// half on the card, pinning it down, as asked; the picture keeps
/// the torn edge, with paper where the tape covered it.
fn card_tape(
    folder: &Path,
    tile: &yapfast_core::themes::lists::TileArt,
    ground: u32,
) -> Option<CardTape> {
    let (width, height) = tile.size?;
    if !width.is_finite() || tile.layer.frames.is_some_and(|sheet| sheet.count > 1) {
        return None;
    }
    let picture = baked_art_image(folder, &tile.layer, ground)?;
    let split = split_card_tape(&picture)?;
    let (scale_x, scale_y) = (
        width / picture.width().max(1) as f32,
        height / picture.height().max(1) as f32,
    );
    let [x, y, tape_width, tape_height] = split.bounds;
    let size = (tape_width as f32 * scale_x, tape_height as f32 * scale_y);
    let across = tile.layer.position.0;
    let mut tape_art = tile.clone();
    tape_art.size = Some(size);
    // Where the tape lay in the picture, raised by `lift`, from the
    // picture's own place: x = (card − tape) · across + offset.
    tape_art.offset = (
        tile.offset.0 + x as f32 * scale_x - (width - size.0) * across,
        tile.offset.1 + (y as f32 - split.lift) * scale_y,
    );
    Some(CardTape {
        untaped: tile_art_from(split.untaped, tile),
        tape: tile_art_from(split.tape, &tape_art),
        tape_art,
    })
}

fn tile_art_from(
    picture: image::RgbaImage,
    tile: &yapfast_core::themes::lists::TileArt,
) -> TileArtImage {
    let (handle, pieces) = tile_art_handles(picture, tile.slice);
    TileArtImage {
        handle,
        art: tile.clone(),
        pieces,
        frames: Vec::new(),
    }
}

struct SplitTape {
    untaped: image::RgbaImage,
    tape: image::RgbaImage,
    /// The tape's box in the picture: x, y, width, height.
    bounds: [u32; 4],
    /// Picture pixels the tape goes up so the torn edge runs through its
    /// middle.
    lift: f32,
}

/// Masking tape's pixels: a pale yellow, well warmer than the paper.
fn is_tape(pixel: &image::Rgba<u8>) -> bool {
    let [red, green, blue, alpha] = pixel.0;
    alpha > 40
        && red > 120
        && i32::from(red) - i32::from(blue) > 35
        && u32::from(green) * 10 > u32::from(red) * 7
}

fn split_card_tape(picture: &image::RgbaImage) -> Option<SplitTape> {
    let (width, height) = picture.dimensions();
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    let mut count = 0u32;
    for (x, y, pixel) in picture.enumerate_pixels() {
        if is_tape(pixel) {
            count += 1;
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
    }
    if count < 200 || right <= left || bottom <= top {
        return None;
    }
    let (tape_width, tape_height) = (right - left + 1, bottom - top + 1);
    // The paper under the tape's opaque part: the commonest opaque colour
    // that isn't tape.
    let mut papers: HashMap<[u8; 3], u32> = HashMap::new();
    for pixel in picture.pixels() {
        if pixel.0[3] == 255 && !is_tape(pixel) {
            *papers
                .entry([pixel.0[0], pixel.0[1], pixel.0[2]])
                .or_default() += 1;
        }
    }
    let paper = papers
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or([255, 253, 247], |(colour, _)| colour);
    // The tape's own see-through look: what it is where it lies on
    // nothing (on the card, below the torn paper), its colour and strength.
    let mean = |filter: &dyn Fn(&image::Rgba<u8>) -> bool| {
        let (mut sum, mut n) = ([0.0f32; 4], 0u32);
        for pixel in picture.pixels().filter(|pixel| filter(pixel)) {
            for (total, value) in sum.iter_mut().zip(pixel.0) {
                *total += f32::from(value);
            }
            n += 1;
        }
        (n > 0).then(|| sum.map(|total| total / n as f32))
    };
    let clear = mean(&|pixel| is_tape(pixel) && pixel.0[3] < 250);
    let solid = mean(&|pixel| is_tape(pixel) && pixel.0[3] == 255);
    let strength = clear.map_or(0.6, |clear| (clear[3] / 255.0).clamp(0.3, 0.95));
    let mut untaped = picture.clone();
    let mut tape = image::RgbaImage::new(tape_width, tape_height);
    for y in top..=bottom {
        for x in left..=right {
            let pixel = *picture.get_pixel(x, y);
            if !is_tape(&pixel) {
                continue;
            }
            let [red, green, blue, alpha] = pixel.0;
            let out = if alpha == 255 {
                // Tape over paper: the paper goes back, and the tape takes
                // the see-through look it has on the card, keeping this
                // part's own crinkle.
                untaped.put_pixel(x, y, image::Rgba([paper[0], paper[1], paper[2], 255]));
                let shade = |at: usize, value: u8| {
                    let (clear, solid) = (
                        clear.map_or(f32::from(value), |c| c[at]),
                        solid.map_or(f32::from(value), |c| c[at]),
                    );
                    (clear + f32::from(value) - solid).round().clamp(0.0, 255.0) as u8
                };
                image::Rgba([
                    shade(0, red),
                    shade(1, green),
                    shade(2, blue),
                    (strength * 255.0).round() as u8,
                ])
            } else {
                untaped.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
                pixel
            };
            tape.put_pixel(x - left, y - top, out);
        }
    }
    // The torn edge beside the tape: the first solid row just past its
    // ends, either side.
    let edge_at = |x: u32| {
        (0..height)
            .find(|&y| picture.get_pixel(x, y).0[3] > 128 && !is_tape(picture.get_pixel(x, y)))
    };
    let mut edges = Vec::new();
    for x in [left.saturating_sub(4), (right + 4).min(width - 1)] {
        if let Some(y) = edge_at(x) {
            edges.push(y as f32);
        }
    }
    let edge = if edges.is_empty() {
        top as f32
    } else {
        edges.iter().sum::<f32>() / edges.len() as f32
    };
    let lift = (top as f32 + tape_height as f32 / 2.0 - edge).max(0.0);
    Some(SplitTape {
        untaped,
        tape,
        bounds: [left, top, tape_width, tape_height],
        lift,
    })
}

/// An id's seed as Slint's lists see it: FNV-1a ([`lists::seed`]),
/// carried in a Slint `int`, which holds it as a 32-bit float, so the low
/// bits round away. Picking a frame or a lean from this one gives the same
/// pick as the Slint client (Zine's channel tapes, rail prints, card lean).
///
/// [`lists::seed`]: yapfast_core::themes::lists::seed
pub fn slint_seed(id: &str) -> u64 {
    yapfast_core::themes::lists::seed(id) as f32 as u64
}

/// A lean within `tilt` degrees from a seed, as Slint's `lean`.
pub fn seeded_lean(seed: u64, tilt: f32) -> f32 {
    tilt * ((seed % 201) as f32 - 100.0) / 100.0
}

/// A message card's pictures as list art the chat's tiler draws: each
/// place on the card's top (or, `bottom`, its bottom) edge, with its
/// choices, sized and moved so it straddles the edge as in Slint (half
/// above, half below, then its own offset). An infinite width spans the
/// card (`full`).
pub fn card_tiles(
    cards: &theme_layout::Cards,
) -> Vec<(bool, Vec<yapfast_core::themes::lists::TileArt>)> {
    cards
        .art
        .iter()
        .map(|art| {
            let (width, height) = art.size.unwrap_or((96.0, 24.0));
            let width = if art.full { f32::INFINITY } else { width };
            let dy = if art.bottom {
                height / 2.0 + art.offset.1
            } else {
                -height / 2.0 + art.offset.1
            };
            let tile = |source: Source| {
                let mut layer = art.layer.clone();
                layer.source = source;
                layer.position = (art.across, if art.bottom { 1.0 } else { 0.0 });
                yapfast_core::themes::lists::TileArt {
                    layer,
                    size: Some((width, height)),
                    offset: (art.offset.0, dy),
                    rotation: 0.0,
                    slice: None,
                    window: None,
                    density: 1.0,
                    pixelated: false,
                    window_radius: 0.0,
                }
            };
            let mut tiles = vec![tile(art.layer.source.clone())];
            tiles.extend(
                art.choices
                    .iter()
                    .filter(
                        |choice| !matches!(&art.layer.source, Source::Image(own) if own == *choice),
                    )
                    .map(|choice| tile(Source::Image(choice.clone()))),
            );
            (art.bottom, tiles)
        })
        .collect()
}

fn tile_art_key(tile: &yapfast_core::themes::lists::TileArt, surface: ArtSurface) -> String {
    format!("{}:{tile:?}", surface.name())
}

fn tile_art_image(
    folder: &Path,
    tile: &yapfast_core::themes::lists::TileArt,
    ground: u32,
) -> Option<TileArtImage> {
    let picture = baked_art_image(folder, &tile.layer, ground)?;
    let mut frames = Vec::new();
    if let Some(sheet) = tile.layer.frames.filter(|sheet| sheet.count > 1) {
        let columns = sheet.columns.max(1).min(sheet.count);
        let rows = sheet.count.div_ceil(columns);
        let width = (picture.width() / columns).max(1);
        let height = (picture.height() / rows).max(1);
        for index in 0..sheet.count {
            let frame = image::imageops::crop_imm(
                &picture,
                index % columns * width,
                index / columns * height,
                width,
                height,
            )
            .to_image();
            frames.push(tile_art_handles(frame, tile.slice));
        }
    }
    let (handle, pieces) = frames
        .first()
        .cloned()
        .unwrap_or_else(|| tile_art_handles(picture, tile.slice));
    Some(TileArtImage {
        handle,
        art: tile.clone(),
        pieces,
        frames,
    })
}

fn tile_art_handles(
    picture: image::RgbaImage,
    slice: Option<[f32; 4]>,
) -> (
    iced::advanced::image::Handle,
    Option<Vec<Option<iced::advanced::image::Handle>>>,
) {
    let (width, height) = picture.dimensions();
    let pieces = slice.map(|[top, right, bottom, left]| {
        let (left, right, top, bottom) = (
            left.round().clamp(0.0, width as f32) as u32,
            right.round().clamp(0.0, width as f32) as u32,
            top.round().clamp(0.0, height as f32) as u32,
            bottom.round().clamp(0.0, height as f32) as u32,
        );
        let xs = [0, left, width.saturating_sub(right), width];
        let ys = [0, top, height.saturating_sub(bottom), height];
        let mut pieces = vec![None; 9];
        for row in 0..3 {
            for column in 0..3 {
                let piece_width = xs[column + 1].saturating_sub(xs[column]);
                let piece_height = ys[row + 1].saturating_sub(ys[row]);
                if piece_width > 0 && piece_height > 0 {
                    pieces[row * 3 + column] = Some(rgba_handle(
                        image::imageops::crop_imm(
                            &picture,
                            xs[column],
                            ys[row],
                            piece_width,
                            piece_height,
                        )
                        .to_image(),
                    ));
                }
            }
        }
        pieces
    });
    (rgba_handle(picture), pieces)
}

/// A composer key's pictures, decoded; none when its resting picture
/// can't be, and no pressed one when that can't be.
fn composer_key(folder: &Path, key: &art::Key) -> Option<ComposerKey> {
    let decode = |file: &Path| {
        let picture = yapfast_core::chat::decode_picture_at(&folder.join(file), 1024, 1024)
            .or_else(|| {
                tracing::warn!(art = %file.display(), "a theme's composer key could not be decoded");
                None
            })?;
        Some(picture_rgba(&picture))
    };
    let up = decode(&key.image)?;
    let size = key.size.map_or_else(
        || Size::new(up.width() as f32 / 2.0, up.height() as f32 / 2.0),
        |(width, height)| Size::new(width, height),
    );
    Some(ComposerKey {
        up: rgba_handle(up),
        down: key.pressed.as_deref().and_then(decode).map(rgba_handle),
        size,
        travel: key.travel,
    })
}

fn art_layer(
    folder: &Path,
    layer: &art::Layer,
    ground: u32,
    frame_pictures: bool,
) -> Option<ArtLayer> {
    let picture = baked_art_image(folder, layer, ground)?;
    let (mut width, mut height) = picture.dimensions();
    let mut frames = Vec::new();
    let mut sheet = None;
    let mut first = None;
    if let Some(frames_of) = layer.frames.filter(|sheet| sheet.count > 1) {
        let columns = frames_of.columns.max(1).min(frames_of.count);
        let rows = frames_of.count.div_ceil(columns);
        width = (width / columns).max(1);
        height = (height / rows).max(1);
        first = Some(rgba_handle(
            image::imageops::crop_imm(&picture, 0, 0, width, height).to_image(),
        ));
        if frame_pictures {
            for index in 0..frames_of.count {
                let x = index % columns * width;
                let y = index / columns * height;
                frames.push(rgba_handle(
                    image::imageops::crop_imm(&picture, x, y, width, height).to_image(),
                ));
            }
        } else {
            sheet = Some((columns, rows, frames_of.count));
        }
    }
    // A tiny tile (a 3 px LCD grid) repeats into a larger one, so the
    // tiler's 128 tiles a side still cover a window, and draws far fewer.
    let picture = if first.is_none() && layer.fit == art::Fit::Tile && (width < 64 || height < 64) {
        let across = 64u32.div_ceil(width.max(1));
        let down = 64u32.div_ceil(height.max(1));
        let mut tiled = image::RgbaImage::new(width * across, height * down);
        image::imageops::tile(&mut tiled, &picture);
        width = tiled.width();
        height = tiled.height();
        tiled
    } else {
        picture
    };
    let pin = pinned_tape(layer, &picture);
    let rim = layer.fit == art::Fit::Stretch && {
        let (w, h) = picture.dimensions();
        let alpha = |x: u32, y: u32| picture.get_pixel(x.min(w - 1), y.min(h - 1)).0[3];
        w > 8 && h > 8 && alpha(w / 2, h / 2) == 0 && alpha(0, h / 2) > 200 && alpha(w / 2, 0) > 200
    };
    let corners = if pin.is_some() {
        tape_strips(&picture)
    } else {
        Vec::new()
    };
    let (handle, sheet) = match (sheet, first) {
        (Some((columns, rows, count)), Some(first)) => {
            (first, Some((rgba_handle(picture), columns, rows, count)))
        }
        (_, first) => (
            frames
                .first()
                .cloned()
                .or(first)
                .unwrap_or_else(|| rgba_handle(picture)),
            None,
        ),
    };
    Some(ArtLayer {
        handle,
        frames,
        sheet,
        width,
        height,
        layer: layer.clone(),
        pin,
        corners,
        rim,
    })
}

/// See [`ArtLayer::pin`].
fn pinned_tape(layer: &art::Layer, picture: &image::RgbaImage) -> Option<f32> {
    let art::Source::Image(path) = &layer.source else {
        return None;
    };
    let named = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.starts_with("tape"));
    if !named || layer.fit != art::Fit::Contain || layer.position.1 > 0.01 {
        return None;
    }
    let (width, height) = picture.dimensions();
    let inked = |y: u32| (0..width).any(|x| picture.get_pixel(x, y).0[3] > 64);
    let top = (0..height).find(|&y| inked(y))?;
    let bottom = (0..height).rev().find(|&y| inked(y))?;
    Some((top + bottom + 1) as f32 / 2.0 / height.max(1) as f32)
}

/// The separate strips of a tape picture, when there are exactly two: each
/// cut to its box, with its size and the angle its length lies at (from
/// its pixels' spread), in degrees, clockwise.
fn tape_strips(picture: &image::RgbaImage) -> Vec<(iced::advanced::image::Handle, Size, f32)> {
    let (width, height) = picture.dimensions();
    let inked = |x: u32| (0..height).any(|y| picture.get_pixel(x, y).0[3] > 64);
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut start = None;
    for x in 0..=width {
        match (x < width && inked(x), start) {
            (true, None) => start = Some(x),
            (false, Some(from)) => {
                runs.push((from, x));
                start = None;
            }
            _ => {}
        }
    }
    runs.retain(|(from, to)| to - from > 8);
    if runs.len() != 2 {
        return Vec::new();
    }
    runs.into_iter()
        .filter_map(|(from, to)| {
            let rows: Vec<u32> = (0..height)
                .filter(|&y| (from..to).any(|x| picture.get_pixel(x, y).0[3] > 64))
                .collect();
            let (top, bottom) = (*rows.first()?, *rows.last()? + 1);
            let strip =
                image::imageops::crop_imm(picture, from, top, to - from, bottom - top).to_image();
            // The strip's length: its inked pixels' main direction.
            let (mut n, mut mx, mut my) = (0.0f32, 0.0f32, 0.0f32);
            for (x, y, pixel) in strip.enumerate_pixels() {
                if pixel.0[3] > 64 {
                    n += 1.0;
                    mx += x as f32;
                    my += y as f32;
                }
            }
            let (mx, my) = (mx / n.max(1.0), my / n.max(1.0));
            let (mut xx, mut yy, mut xy) = (0.0f32, 0.0f32, 0.0f32);
            for (x, y, pixel) in strip.enumerate_pixels() {
                if pixel.0[3] > 64 {
                    let (dx, dy) = (x as f32 - mx, y as f32 - my);
                    xx += dx * dx;
                    yy += dy * dy;
                    xy += dx * dy;
                }
            }
            let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
            let size = Size::new(strip.width() as f32, strip.height() as f32);
            Some((rgba_handle(strip), size, angle.to_degrees()))
        })
        .collect()
}

fn surface_ground(resolved: &Resolved, palette: &palette::Theme, surface: ArtSurface) -> u32 {
    if let Some(fixed) = surface.fixed_ground() {
        return fixed;
    }
    // A surface's ground is often one of the package's extra colours
    // (`composer`, `menu`, `account`, `members`...), which the palette
    // lacks: the theme's own, else the colour it follows, as Slint's
    // `Theme.<token>` resolves.
    let mut token = surface.ground();
    for _ in 0..4 {
        if let Some((_, paint)) = resolved.colours.iter().find(|(name, _)| *name == token) {
            return 0xff00_0000 | (paint.solid() & 0x00ff_ffff);
        }
        if let Some(colour) = palette::token_index(token).and_then(|index| palette.colour(index)) {
            return 0xff00_0000 | colour.rgb_part();
        }
        match package::follows(token) {
            Some(next) => token = next,
            None => break,
        }
    }
    0xff00_0000
}

fn decoded_art(folder: &Path, layer: &art::Layer) -> Option<image::RgbaImage> {
    match &layer.source {
        Source::Image(file) => {
            let path = folder.join(file);
            let picture = yapfast_core::chat::decode_picture_at(&path, 4096, 4096).or_else(|| {
                tracing::warn!(art = %file.display(), "an installed theme art file could not be decoded");
                None
            })?;
            Some(picture_rgba(&picture))
        }
        Source::Texture(texture) => Some(texture_image(*texture, layer.colour)),
    }
}

fn picture_rgba(picture: &yapfast_core::chat::Picture) -> image::RgbaImage {
    use yapfast_core::dominant_colour::Order;
    let mut rgba = picture.rgba.to_vec();
    if crate::pictures::byte_order(picture) == Order::Bgra {
        for pixel in rgba.chunks_exact_mut(4) {
            let [b, g, r, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let straight = |value: u8| match a {
                0 => 0,
                255 => value,
                _ => ((u16::from(value) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8,
            };
            pixel.copy_from_slice(&[straight(r), straight(g), straight(b), a]);
        }
    }
    image::RgbaImage::from_raw(picture.width, picture.height, rgba)
        .expect("decoded art has its declared dimensions")
}

/// Bakes blur, blend and dim once, as the Slint renderer does.
fn baked_art_image(folder: &Path, layer: &art::Layer, ground: u32) -> Option<image::RgbaImage> {
    let mut image = decoded_art(folder, layer)?;
    if layer.fit == art::Fit::Tile && (layer.scale - 1.0).abs() > 0.01 {
        image = image::imageops::resize(
            &image,
            ((image.width() as f32 * layer.scale).round() as u32).clamp(1, 2048),
            ((image.height() as f32 * layer.scale).round() as u32).clamp(1, 2048),
            image::imageops::FilterType::Triangle,
        );
    }
    if layer.blur > 0.0 {
        image = image::imageops::fast_blur(&image, layer.blur / 2.0);
    }
    let base = [
        ((ground >> 16) & 0xff) as u8,
        ((ground >> 8) & 0xff) as u8,
        (ground & 0xff) as u8,
    ];
    for pixel in image.as_mut().chunks_exact_mut(4) {
        for channel in 0..3 {
            let source = f32::from(pixel[channel]) / 255.0;
            let below = f32::from(base[channel]) / 255.0;
            let mixed = match layer.blend {
                art::Blend::Normal => source,
                art::Blend::Multiply => source * below,
                art::Blend::Screen => 1.0 - (1.0 - source) * (1.0 - below),
                art::Blend::Overlay => {
                    if below < 0.5 {
                        2.0 * source * below
                    } else {
                        1.0 - 2.0 * (1.0 - source) * (1.0 - below)
                    }
                }
            };
            pixel[channel] = (mixed * (1.0 - layer.dim) * 255.0).round() as u8;
        }
    }
    Some(image)
}

fn rgba_handle(image: image::RgbaImage) -> iced::advanced::image::Handle {
    let (width, height) = image.dimensions();
    let mut rgba = image.into_raw();
    crate::paint::to_native(&mut rgba);
    iced::advanced::image::Handle::from_rgba(width, height, rgba)
}

fn texture_image(texture: art::Texture, colour: u32) -> image::RgbaImage {
    const SIDE: u32 = 64;
    let [alpha, red, green, blue] = colour.to_be_bytes();
    image::RgbaImage::from_fn(SIDE, SIDE, |x, y| {
        let hash = x
            .wrapping_mul(1_103_515_245)
            .wrapping_add(y.wrapping_mul(12_345))
            .rotate_left((x + y) & 15);
        let coverage = match texture {
            art::Texture::Grain => f32::from((hash & 31) as u8) / 90.0,
            art::Texture::Noise => f32::from((hash & 63) as u8) / 70.0,
            art::Texture::Paper => {
                if y % 13 == 0 || hash % 97 == 0 {
                    0.28
                } else {
                    0.035
                }
            }
            art::Texture::Kraft => {
                if (x + y * 3) % 29 == 0 {
                    0.34
                } else {
                    f32::from((hash & 15) as u8) / 120.0
                }
            }
            art::Texture::Halftone => {
                let (dx, dy) = ((x % 8) as i32 - 4, (y % 8) as i32 - 4);
                if dx * dx + dy * dy <= 4 { 0.65 } else { 0.0 }
            }
        };
        image::Rgba([
            red,
            green,
            blue,
            (coverage.clamp(0.0, 1.0) * f32::from(alpha)).round() as u8,
        ])
    })
}

/// The window size glass is laid out for: never smaller than this.
fn glass_window(window: (u32, u32)) -> (u32, u32) {
    (window.0.max(256), window.1.max(192))
}

/// The Slint build's fake glass: one reduced, preblurred copy of the window art,
/// aligned under every pane. It intentionally never samples other widgets.
fn glass_pane(
    folder: &Path,
    resolved: &Resolved,
    palette: &palette::Theme,
    style: &art::Glass,
    window: (u32, u32),
) -> GlassPane {
    let source = Arc::new(glass_source(folder, resolved, palette, style));
    let grounds = style
        .surfaces
        .iter()
        .copied()
        .map(|surface| (surface, surface_ground(resolved, palette, surface)))
        .collect();
    GlassPane {
        blurred: bake_glass(&source, window),
        style: style.clone(),
        grounds,
        source,
    }
}

/// Reads, decodes, blurs, blends and dims the window art once, and reduces
/// each layer as far as any window size allows.
fn glass_source(
    folder: &Path,
    resolved: &Resolved,
    palette: &palette::Theme,
    style: &art::Glass,
) -> GlassSource {
    const SHRINK: u32 = yapfast_core::themes::check::art::GLASS_SHRINK;
    // A reduced canvas for a 4K window is 960 wide; a fitted layer kept at
    // this size serves any window without decoding again.
    const FIT_MOST: u32 = 1024;
    let ground = surface_ground(resolved, palette, ArtSurface::Window);
    let [_, red, green, blue] = ground.to_be_bytes();
    let filter = image::imageops::FilterType::Triangle;
    let mut layers = Vec::new();
    for layer in resolved
        .art
        .surfaces
        .get(&ArtSurface::Window)
        .into_iter()
        .flatten()
    {
        let Some(mut picture) = baked_art_image(folder, layer, ground) else {
            continue;
        };
        if let Some(sheet) = layer.frames.filter(|sheet| sheet.count > 1) {
            let columns = sheet.columns.max(1).min(sheet.count);
            let rows = sheet.count.div_ceil(columns);
            picture = image::imageops::crop_imm(
                &picture,
                0,
                0,
                (picture.width() / columns).max(1),
                (picture.height() / rows).max(1),
            )
            .to_image();
        }
        if layer.opacity < 1.0 {
            for pixel in picture.pixels_mut() {
                pixel.0[3] = (f32::from(pixel.0[3]) * layer.opacity).round() as u8;
            }
        }
        if let Some((width, height)) = layer.size {
            let fitted = image::imageops::resize(
                &picture,
                ((width / SHRINK as f32).round() as u32).max(1),
                ((height / SHRINK as f32).round() as u32).max(1),
                filter,
            );
            layers.push(GlassLayer::Placed(fitted, layer.position));
            continue;
        }
        layers.push(match layer.fit {
            art::Fit::Tile => GlassLayer::Tile(image::imageops::resize(
                &picture,
                (picture.width() / SHRINK).max(1),
                (picture.height() / SHRINK).max(1),
                filter,
            )),
            fit => {
                if picture.width() > FIT_MOST || picture.height() > FIT_MOST {
                    picture = image::DynamicImage::ImageRgba8(picture)
                        .resize(FIT_MOST, FIT_MOST, filter)
                        .to_rgba8();
                }
                GlassLayer::Fit(fit, picture)
            }
        });
    }
    GlassSource {
        ground: [red, green, blue],
        blur: style.blur,
        layers,
    }
}

/// Lays `source` out for a window of `window` and blurs it: a small canvas,
/// cheap enough for a resize, and run off the window's thread then.
fn bake_glass(
    source: &GlassSource,
    window: (u32, u32),
) -> yapfast_iced_kit::widgets::BlurredWindow {
    const SHRINK: u32 = yapfast_core::themes::check::art::GLASS_SHRINK;
    let window = glass_window(window);
    let size = ((window.0 / SHRINK).max(64), (window.1 / SHRINK).max(48));
    let [red, green, blue] = source.ground;
    let mut canvas =
        image::RgbaImage::from_pixel(size.0, size.1, image::Rgba([red, green, blue, 255]));
    let filter = image::imageops::FilterType::Triangle;
    for layer in &source.layers {
        match layer {
            GlassLayer::Placed(fitted, position) => {
                let x = (size.0.saturating_sub(fitted.width()) as f32 * position.0) as i64;
                let y = (size.1.saturating_sub(fitted.height()) as f32 * position.1) as i64;
                image::imageops::overlay(&mut canvas, fitted, x, y);
            }
            GlassLayer::Tile(tile) => image::imageops::tile(&mut canvas, tile),
            GlassLayer::Fit(art::Fit::Contain, picture) => {
                let fitted = image::DynamicImage::ImageRgba8(picture.clone())
                    .resize(size.0, size.1, filter)
                    .to_rgba8();
                image::imageops::overlay(
                    &mut canvas,
                    &fitted,
                    i64::from((size.0 - fitted.width()) / 2),
                    i64::from((size.1 - fitted.height()) / 2),
                );
            }
            GlassLayer::Fit(art::Fit::Stretch, picture) => {
                let fitted = image::imageops::resize(picture, size.0, size.1, filter);
                image::imageops::overlay(&mut canvas, &fitted, 0, 0);
            }
            GlassLayer::Fit(_, picture) => {
                let fitted = image::DynamicImage::ImageRgba8(picture.clone())
                    .resize_to_fill(size.0, size.1, filter)
                    .to_rgba8();
                image::imageops::overlay(&mut canvas, &fitted, 0, 0);
            }
        }
    }
    let canvas = image::imageops::fast_blur(&canvas, (source.blur / SHRINK as f32).max(0.5));
    let (width, height) = canvas.dimensions();
    let mut pixels = canvas.into_raw();
    crate::paint::to_native(&mut pixels);
    yapfast_iced_kit::widgets::BlurredWindow::new(width, height, pixels, window)
}

fn sounds(
    folder: &Path,
    resolved: &Resolved,
    package_options: &[options::ThemeOption],
    choices: &options::Choices,
) -> ThemeSounds {
    let enabled = package_options
        .iter()
        .find(|option| option.id == options::SOUNDS)
        .and_then(|option| option.value(choices).as_f64())
        .unwrap_or(100.0)
        .clamp(0.0, 100.0) as f32
        / 100.0;
    if enabled <= 0.0 {
        return ThemeSounds::default();
    }
    let mut out = ThemeSounds::default();
    for (&slot, choice) in &resolved.sounds {
        let samples: Vec<Arc<[i16]>> = choice
            .files
            .iter()
            .filter_map(|file| {
                let path = folder.join(file);
                let bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::warn!(%error, sound = %file.display(), "an installed theme sound could not be read");
                        return None;
                    }
                };
                match media::decode_sound(&path, &bytes) {
                    Ok(samples) => Some(Arc::from(samples)),
                    Err(error) => {
                        tracing::warn!(%error, sound = %file.display(), "an installed theme sound could not be decoded");
                        None
                    }
                }
            })
            .collect();
        if samples.is_empty() {
            continue;
        }
        let volume = choice.volume * enabled;
        if let Some(sound) = slot.app_sound() {
            out.app.insert(sound, ThemedSound { samples, volume });
        } else if matches!(slot, media::SoundSlot::Ring | media::SoundSlot::Ringback) {
            let scaled: Arc<[i16]> = samples[0]
                .iter()
                .map(|sample| (f32::from(*sample) * volume).round() as i16)
                .collect::<Vec<_>>()
                .into();
            out.rings.insert(slot, scaled);
        } else if slot.interface() {
            out.cues.insert(slot, SoundCue { samples, volume });
        }
    }
    out
}

fn icons(
    folder: &Path,
    resolved: &Resolved,
    palette: &palette::Theme,
) -> HashMap<String, yapfast_iced_kit::icons::Replacement> {
    resolved
        .icons
        .iter()
        .filter_map(|(name, choice)| {
            let svg = match std::fs::read_to_string(folder.join(&choice.file)) {
                Ok(svg) => svg,
                Err(error) => {
                    tracing::warn!(%error, icon = *name, "an installed theme icon could not be read");
                    return None;
                }
            };
            let colour = match choice.colour {
                media::IconColour::Tint => yapfast_iced_kit::icons::ReplacementColour::Tint,
                media::IconColour::Keep => yapfast_iced_kit::icons::ReplacementColour::Keep,
                media::IconColour::Colour(colour) => {
                    yapfast_iced_kit::icons::ReplacementColour::Colour(colour & 0x00ff_ffff)
                }
                media::IconColour::Token(token) => {
                    // A palette token, or one of the package's own extra
                    // colours (Zine's logo in `accent-text`), as Slint's.
                    let colour = palette::token_index(token)
                        .and_then(|index| palette.colour(index))
                        .map(Colour::rgb_part)
                        .or_else(|| {
                            resolved.colours.iter().find_map(|(name, paint)| match paint {
                                package::Paint::Colour(argb) if *name == token => {
                                    Some(argb & 0x00ff_ffff)
                                }
                                _ => None,
                            })
                        })
                        .unwrap_or(0);
                    yapfast_iced_kit::icons::ReplacementColour::Colour(colour)
                }
            };
            Some((
                (*name).to_owned(),
                yapfast_iced_kit::icons::Replacement {
                    svg: svg.into(),
                    colour,
                },
            ))
        })
        .collect()
}

/// The package's variant after its built-in mode option. `auto` keeps the
/// existing dark preference until iced exposes a stable system-theme value
/// at this boundary.
fn selected_mode(
    package: &package::Package,
    choices: &options::Choices,
    system_mode: Mode,
) -> Mode {
    let preferred = match choices
        .get(options::MODE)
        .and_then(serde_json::Value::as_str)
    {
        Some("light") => Mode::Light,
        Some("dark") => Mode::Dark,
        _ => system_mode,
    };
    package.mode_for(preferred)
}

/// A resolved package's shape, spacing and motion tokens in the kit's
/// shape-preserving companion data.
fn metrics(
    resolved: &Resolved,
    performance: Performance,
    reduced: bool,
) -> yapfast_iced_kit::theme::Metrics {
    let paints = resolved
        .colours
        .iter()
        .filter(|(token, _)| {
            package::BRUSHES.contains(token) || package::EXTRA_COLOURS.contains(token)
        })
        .map(|(token, value)| ((*token).to_owned(), paint(value)))
        .collect();
    let mut decorations: std::collections::HashSet<String> = resolved
        .art
        .surfaces
        .iter()
        .filter(|(_, layers)| !layers.is_empty())
        .map(|(surface, _)| surface.name().to_owned())
        .collect();
    if let Some(glass) = &resolved.art.glass {
        decorations.extend(
            glass
                .surfaces
                .iter()
                .map(|surface| surface.name().to_owned()),
        );
    }
    let shapes = resolved
        .shapes
        .iter()
        .map(|(surface, shape)| {
            (
                surface.name().to_owned(),
                yapfast_iced_kit::theme::Shape {
                    radius: shape.radius,
                    border: shape.border,
                    border_colour: shape.border_colour.map(argb),
                    shadow: shape.shadow.map(shadow),
                    inset: shape.inset,
                    inset_top: shape.inset_top,
                    width: shape.width,
                    hover_width: shape.hover_width,
                    ground: shape.ground.as_ref().map(paint),
                    text: shape.text.map(argb),
                    bar: shape.bar.as_ref().map(paint),
                    bar_text: shape.bar_text.map(argb),
                    stripe: shape.stripe.map(|stripe| yapfast_iced_kit::theme::Stripe {
                        colour: argb(stripe.colour),
                        channels: stripe.channels,
                        members: stripe.members,
                        messages: stripe.messages,
                    }),
                    case: shape.case,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let spacing = &resolved.spacing;
    let outlines = resolved
        .layout
        .borders
        .iter()
        .map(|(place, border)| {
            let stroke = match border.stroke {
                yapfast_core::themes::layout::Stroke::Solid => {
                    yapfast_iced_kit::theme::OutlineStroke::Solid
                }
                yapfast_core::themes::layout::Stroke::Dashed => {
                    yapfast_iced_kit::theme::OutlineStroke::Dashed
                }
                yapfast_core::themes::layout::Stroke::Dotted => {
                    yapfast_iced_kit::theme::OutlineStroke::Dotted
                }
            };
            (
                place.name().to_owned(),
                yapfast_iced_kit::theme::Outline {
                    stroke,
                    dash: border.dash,
                    gap: border.gap,
                    width: border.width,
                    colour: border.colour.map(argb),
                },
            )
        })
        .collect();
    yapfast_iced_kit::theme::Metrics {
        paints,
        decorations,
        shapes,
        outlines,
        radii: resolved.radii,
        shadows: resolved.shadows.map(|shadow| shadow.map(self::shadow)),
        outline_width: resolved.layout.outline_width,
        spacing: yapfast_iced_kit::theme::Spacing {
            message_gap: spacing.message_gap,
            message_group_gap: spacing.message_group_gap,
            channel_row_height: resolved
                .layout
                .channel_rows
                .as_ref()
                .and_then(|rows| rows.height)
                .or(spacing.channel_row_height),
            channel_heading_height: resolved
                .layout
                .channel_rows
                .as_ref()
                .and_then(|rows| rows.headers.height),
            member_row_height: resolved
                .layout
                .member_rows
                .as_ref()
                .and_then(|rows| rows.height)
                .or(spacing.member_row_height),
            avatar_size: spacing.avatar_size,
            chat_padding: spacing.chat_padding,
            sidebar_width: spacing.sidebar_width,
            members_width: spacing.members_width,
            cards_gap: (resolved.layout.messages == Some(theme_layout::MessageStyle::Cards))
                .then(|| resolved.layout.cards.gap.unwrap_or(8.0)),
        },
        motion: motion(&resolved.motion, performance, reduced),
        types: HashMap::new(),
        readable_text: false,
        tooltip: None,
        hover_cue: false,
        press_cue: false,
    }
}

/// Loads each resolved theme font and keeps all typography tokens at
/// the kit Theme boundary. A file that somehow becomes unreadable after the
/// package was checked falls back to Wired's face for that role, while its
/// remaining size/weight/case tokens still apply.
fn typography(
    folder: &Path,
    resolved: &Resolved,
) -> HashMap<String, yapfast_iced_kit::theme::TypeStyle> {
    resolved
        .types
        .iter()
        .map(|(role, kind)| {
            let font = kind.font.as_ref().and_then(|file| {
                let path = folder.join(file);
                match std::fs::read(&path) {
                    Ok(bytes) => match yapfast_iced_kit::fonts::load_theme_font(bytes) {
                        Ok(font) => Some(font.font),
                        Err(error) => {
                            tracing::warn!(%error, font = %path.display(), "an installed theme font could not be loaded");
                            None
                        }
                    },
                    Err(error) => {
                        tracing::warn!(%error, font = %path.display(), "an installed theme font could not be read");
                        None
                    }
                }
            });
            (
                role.name().to_owned(),
                yapfast_iced_kit::theme::TypeStyle {
                    font,
                    size: kind.size,
                    weight: kind.weight,
                    letter_spacing: kind.letter_spacing,
                    uppercase: kind.uppercase,
                    header_size: kind.header_size,
                    title_size: kind.title_size,
                },
            )
        })
        .collect()
}

fn shadow(shadow: package::Shadow) -> yapfast_iced_kit::theme::ShapeShadow {
    yapfast_iced_kit::theme::ShapeShadow {
        blur: shadow.blur,
        x: shadow.x,
        y: shadow.y,
        colour: argb(shadow.colour),
    }
}

pub(crate) fn paint(paint: &package::Paint) -> iced::Background {
    match paint {
        package::Paint::Colour(colour) => argb(*colour).into(),
        package::Paint::Linear { angle, stops } => {
            let mut gradient = iced::gradient::Linear::new(iced::Degrees(*angle));
            for (colour, offset) in stops.iter().take(8) {
                gradient = gradient.add_stop(offset.clamp(0.0, 1.0), argb(*colour));
            }
            gradient.into()
        }
    }
}

fn motion(
    motion: &package::Motion,
    performance: Performance,
    reduced: bool,
) -> yapfast_iced_kit::theme::Motion {
    use yapfast_iced_kit::theme::{Close, Curve, Hover, Press, Transition};
    let still = performance == Performance::Light || reduced;
    let millis = |value: Option<f32>, stock: f32| {
        Duration::from_micros((value.filter(|_| !still).unwrap_or(stock) * 1000.0).round() as u64)
    };
    let quick = millis(motion.quick, 120.0);
    let curve = match motion.curve.filter(|_| !still).unwrap_or_default() {
        package::Curve::Linear => Curve::Linear,
        package::Curve::Ease => Curve::Ease,
        package::Curve::EaseIn => Curve::EaseIn,
        package::Curve::EaseOut => Curve::EaseOut,
        package::Curve::EaseInOut => Curve::EaseInOut,
        package::Curve::Spring => Curve::Spring,
        package::Curve::Bounce => Curve::Bounce,
        package::Curve::Elastic => Curve::Elastic,
    };
    let open = match motion.open.filter(|_| !still).unwrap_or_default() {
        package::Transition::Grow => Transition::Grow,
        package::Transition::None => Transition::None,
        package::Transition::Fade => Transition::Fade,
        package::Transition::Zoom => Transition::Zoom,
        package::Transition::Pop => Transition::Pop,
    };
    let close = match motion.close.filter(|_| !still).unwrap_or_default() {
        package::Close::Reverse => Close::Reverse,
        package::Close::None => Close::None,
    };
    let hover = match motion.hover.filter(|_| !still).unwrap_or_default() {
        package::Hover::Fade => Hover::Fade,
        package::Hover::None => Hover::None,
        package::Hover::Lift => Hover::Lift,
        package::Hover::Glow => Hover::Glow,
        package::Hover::Scale => Hover::Scale,
    };
    let press = match motion.press.filter(|_| !still).unwrap_or_default() {
        package::Press::None => Press::None,
        package::Press::Scale => Press::Scale,
        package::Press::Sink => Press::Sink,
    };
    let unfocused = match motion.unfocused.unwrap_or_default() {
        package::Unfocused::Full => yapfast_iced_kit::theme::Unfocused::Full,
        package::Unfocused::Slow => yapfast_iced_kit::theme::Unfocused::Slow,
        package::Unfocused::Still => yapfast_iced_kit::theme::Unfocused::Still,
    };
    yapfast_iced_kit::theme::Motion {
        quick,
        popup: millis(motion.popup, 100.0),
        hover_in: motion
            .hover_in
            .filter(|_| !still)
            .map_or(quick, |ms| millis(Some(ms), 120.0)),
        hover_out: motion
            .hover_out
            .filter(|_| !still)
            .map_or(quick, |ms| millis(Some(ms), 120.0)),
        curve,
        hover,
        press,
        open,
        close,
        unfocused,
    }
}

/// What Roll back names the revision core kept: its version (`v3.5.0`),
/// read from its stored `theme.json`, else its commit's first seven
/// characters, else "the earlier version" (a local install has no commit).
fn earlier_version(folder: &Path, previous: &install::Entry) -> String {
    let version = folder
        .parent()
        .map(|store| {
            store
                .join(&previous.store)
                .join(yapfast_core::themes::MANIFEST)
        })
        .and_then(|manifest| std::fs::read(manifest).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|manifest| manifest.get("version")?.as_str().map(str::to_owned))
        // A version is short and plain; anything else isn't shown.
        .filter(|version| {
            !version.is_empty()
                && version.len() <= 32
                && version
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        });
    match version {
        Some(version) => format!("v{version}"),
        None if !previous.rev.is_empty() => previous.rev.chars().take(7).collect(),
        None => "the earlier version".to_owned(),
    }
}

/// Where gallery thumbnails are kept, one PNG per store folder, named for
/// it: a store folder is its files' digest, so a kept thumbnail never goes
/// stale.
const THUMBNAILS: &str = "theme-thumbs";

/// The installed packages' gallery thumbnails, in order: read from
/// `<data>/theme-thumbs/` when kept there, else decoded from each one's
/// first readable preview at thumbnail size and kept. They're made side by
/// side, so a start with six themes waits for about one. Kept thumbnails of
/// packages no longer installed are removed.
fn thumbnails(
    root: &Path,
    packages: &[(&Path, &[PathBuf])],
) -> Vec<Option<iced::advanced::image::Handle>> {
    let kept = root.join(THUMBNAILS);
    // At most four at once, however many themes are installed.
    const AT_ONCE: usize = 4;
    let mut made = Vec::with_capacity(packages.len());
    for batch in packages.chunks(AT_ONCE) {
        std::thread::scope(|scope| {
            let kept = &kept;
            #[allow(
                clippy::needless_collect,
                reason = "every thread starts before the first is joined"
            )]
            let threads: Vec<_> = batch
                .iter()
                .map(|&(folder, preview)| scope.spawn(move || thumbnail(kept, folder, preview)))
                .collect();
            made.extend(threads.into_iter().map(|made| made.join().ok().flatten()));
        });
    }
    if let Ok(entries) = std::fs::read_dir(&kept) {
        let installed: std::collections::HashSet<_> = packages
            .iter()
            .filter_map(|(folder, _)| folder.file_name())
            .collect();
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_stem()
                .is_none_or(|stem| !installed.contains(stem))
            {
                std::fs::remove_file(path).ok();
            }
        }
    }
    made
}

fn thumbnail(
    kept: &Path,
    folder: &Path,
    preview: &[PathBuf],
) -> Option<iced::advanced::image::Handle> {
    let file = kept.join(format!("{}.png", folder.file_name()?.to_str()?));
    if let Ok(bytes) = std::fs::read(&file)
        && let Some(shown) = crate::theme_remote::decode_preview(&bytes)
    {
        return Some(shown.handle);
    }
    let shrunk = preview.iter().find_map(|relative| {
        crate::theme_remote::shrunk_preview(&std::fs::read(folder.join(relative)).ok()?)
    })?;
    let mut png = Vec::new();
    if image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        shrunk.as_raw(),
        shrunk.width(),
        shrunk.height(),
        image::ExtendedColorType::Rgba8,
    )
    .is_ok()
        && std::fs::create_dir_all(kept).is_ok()
    {
        // Written beside, then renamed, so a start never reads half of one.
        let partial = file.with_extension("png.part");
        if std::fs::write(&partial, &png).is_ok() {
            std::fs::rename(&partial, &file).ok();
        }
    }
    Some(crate::theme_remote::preview_of(shrunk).handle)
}

fn installed_preview(folder: &Path, preview: &[PathBuf]) -> Option<iced::advanced::image::Handle> {
    preview.iter().find_map(|rel| {
        let bytes = std::fs::read(folder.join(rel)).ok()?;
        crate::theme_remote::decode_preview(&bytes).map(|p| p.handle)
    })
}

fn tooltip_surface(
    art: &ThemeArt,
) -> Option<std::sync::Arc<yapfast_iced_kit::theme::TooltipSurface>> {
    use yapfast_iced_kit::theme::{TooltipFit, TooltipGlass, TooltipLayer, TooltipSurface};
    let mut layers = Vec::new();
    if let Some(surface_layers) = art.surfaces.get(&ArtSurface::Tooltip) {
        for item in surface_layers {
            let layer = &item.layer;
            let period_ms = layer
                .frames
                .filter(|f| f.count > 1)
                .map_or(0.0, |f| f.period.max(100.0));
            let fit = match layer.fit {
                art::Fit::Cover => TooltipFit::Cover,
                art::Fit::Contain => TooltipFit::Contain,
                art::Fit::Tile => TooltipFit::Tile,
                art::Fit::Stretch => TooltipFit::Stretch,
            };
            layers.push(TooltipLayer {
                handle: item.handle.clone(),
                frames: item.frames.clone(),
                period_ms,
                width: item.width,
                height: item.height,
                opacity: layer.opacity,
                scale: layer.scale,
                position: layer.position,
                fit,
                drift: layer.drift,
                size: layer.size,
                moves: !art.reduced_motion
                    && art.performance != Performance::Light
                    && (art.performance == Performance::Full || !layer.heavy)
                    && (layer.frames.is_some_and(|f| f.count > 1)
                        || layer.drift.0 != 0.0
                        || layer.drift.1 != 0.0),
            });
        }
    }
    let glass = art.glass.as_ref().and_then(|pane| {
        if !pane.style.surfaces.contains(&ArtSurface::Tooltip) {
            return None;
        }
        let s = &pane.style;
        Some(TooltipGlass {
            blurred: pane.blurred.clone(),
            ground: argb(pane.ground(ArtSurface::Tooltip)),
            opacity: s.opacity,
            radius: s.radius,
            rim: s.rim.map(|(c, w)| (argb(c), w)),
            highlight: s.highlight.map(argb),
            inset: s
                .inset
                .as_ref()
                .map(|shadow| (argb(shadow.colour), shadow.blur)),
            gap: s.gap,
        })
    });
    if layers.is_empty() && glass.is_none() {
        return None;
    }
    let reserve = art
        .reserves
        .get(&ArtSurface::Tooltip)
        .map(|r| [r.top, r.right, r.bottom, r.left]);
    Some(std::sync::Arc::new(TooltipSurface {
        layers,
        glass,
        reserve,
    }))
}

const HOVER_GAP: Duration = Duration::from_millis(70);

#[derive(Default)]
struct CueRuntime {
    /// The theme and sound choices `hover` and `press` were taken from.
    synced: Option<(String, Option<SoundPrefs>)>,
    hover: Option<SoundCue>,
    press: Option<SoundCue>,
    speaker: Option<String>,
    allowed: bool,
    last_hover: Option<Instant>,
}

static CUE_RUNTIME: std::sync::OnceLock<Mutex<CueRuntime>> = std::sync::OnceLock::new();

fn cue_runtime() -> &'static Mutex<CueRuntime> {
    CUE_RUNTIME.get_or_init(|| Mutex::new(CueRuntime::default()))
}

/// Makes the next [`Themes::sync_cue_runtime`] take the theme's cues again.
pub fn forget_synced_cues() {
    if let Ok(mut rt) = cue_runtime().lock() {
        rt.synced = None;
    }
}

pub fn register_cue_handler() {
    yapfast_iced_kit::theme::set_cue_handler(|event| {
        let (samples, volume, speaker) = {
            let Ok(mut rt) = cue_runtime().lock() else {
                return;
            };
            if !rt.allowed {
                return;
            }
            let now = Instant::now();
            let cue = match event {
                yapfast_iced_kit::theme::CueEvent::Hover => {
                    if rt
                        .last_hover
                        .is_some_and(|last| now.saturating_duration_since(last) < HOVER_GAP)
                    {
                        return;
                    }
                    let cue = rt.hover.as_ref().map(|c| {
                        let idx = NEXT_CUE.fetch_add(1, Ordering::Relaxed) % c.samples.len();
                        (c.samples[idx].clone(), c.volume)
                    });
                    if cue.is_some() {
                        rt.last_hover = Some(now);
                    }
                    cue
                }
                yapfast_iced_kit::theme::CueEvent::Press => rt.press.as_ref().map(|c| {
                    let idx = NEXT_CUE.fetch_add(1, Ordering::Relaxed) % c.samples.len();
                    (c.samples[idx].clone(), c.volume)
                }),
            };
            let Some((samples, volume)) = cue else {
                return;
            };
            (samples, volume, rt.speaker.clone())
        };
        yapfast_core::sounds::play_decoded(
            samples,
            volume * yapfast_core::volume::speakers(),
            speaker.as_deref(),
        );
    });
}

pub(crate) fn argb(colour: u32) -> Color {
    Color::from_rgba8(
        (colour >> 16) as u8,
        (colour >> 8) as u8,
        colour as u8,
        ((colour >> 24) as u8) as f32 / 255.0,
    )
}

pub(crate) fn list_case(
    case: Option<yapfast_core::themes::lists::Case>,
    words: &str,
) -> Cow<'_, str> {
    use yapfast_core::themes::lists::Case;
    match case {
        Some(Case::Upper) => Cow::Owned(words.to_uppercase()),
        Some(Case::Lower) => Cow::Owned(words.to_lowercase()),
        _ => Cow::Borrowed(words),
    }
}

pub(crate) fn list_font(
    theme: &Theme,
    role: Option<yapfast_core::themes::lists::FontRole>,
    fallback_role: &'static str,
    weight: Option<f32>,
    stock: iced::Font,
) -> iced::Font {
    let role = role.map_or(fallback_role, yapfast_core::themes::lists::FontRole::name);
    // The list's own weight beats its role's, as in Slint (Daydream's
    // channel names are 700 over a 500 body).
    let mut font = theme.font(role, stock);
    if let Some(weight) = weight {
        font.weight = crate::fonts::font_weight(weight.round().clamp(1.0, 1000.0) as u16);
    }
    font
}

pub(crate) fn offset_shadow(shadow: Option<(u32, f32, f32)>) -> crate::piece::TextLight {
    crate::piece::TextLight {
        shadow: shadow.map(|(colour, x, y)| (argb(colour), iced::Vector::new(x, y))),
        ..Default::default()
    }
}

/// A resolved package's colours over the built-in base it names.
fn resolved_palette(resolved: &Resolved) -> palette::Theme {
    let mut colours = palette::find(&resolved.base).map_or(palette::YAP, |base| base.theme);
    colours.dark = resolved.dark;
    for (token, paint) in &resolved.colours {
        if let Some(slot) = palette::token_index(token).and_then(|index| colours.colour_mut(index))
        {
            *slot = Colour::from_argb(paint.solid());
        }
    }
    colours
}

/// Fills the kit's theme from the core's.
macro_rules! from_palette {
    ($($name:ident,)*) => {
        /// The core's `theme`, named `id`, as the window paints with it.
        pub fn theme(id: &str, theme: &palette::Theme) -> Theme {
            Theme::unregister_metrics(id);
            let mut shown = Theme::new(id, theme.dark);
            shown.bonfire = theme.bonfire();
            $(shown.$name = color(theme.$name);)*
            shown.mention = color(theme.mention());
            shown.worked_out()
        }
    };
}

yapfast_iced_kit::for_theme_colours!(from_palette);

/// The stock look protected screens show whatever theme is applied: sign-in,
/// a link's confirmation and a theme's review. It has no package, so no art,
/// overlays, icons, faces or motion of any theme come with it.
pub fn stock_theme() -> Theme {
    theme(
        DEFAULT,
        &yapfast_core::theme::find(DEFAULT)
            .map_or(yapfast_core::theme::YAP, |built_in| built_in.theme),
    )
}

/// A core colour as iced's.
pub fn color(colour: Colour) -> Color {
    Color::from_rgba8(
        colour.red(),
        colour.green(),
        colour.blue(),
        f32::from(colour.alpha()) / 255.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_pictures_straddle_their_edge() {
        let mut cards = theme_layout::Cards::default();
        let layer = art::Layer {
            source: Source::Image("art/tape.webp".into()),
            fit: art::Fit::Stretch,
            position: (0.0, 0.0),
            opacity: 1.0,
            blend: art::Blend::Normal,
            blur: 0.0,
            dim: 0.0,
            scale: 1.0,
            colour: 0,
            drift: (0.0, 0.0),
            frames: None,
            heavy: false,
            size: None,
            reserve: None,
        };
        cards.art.push(theme_layout::CardArt {
            layer,
            choices: Vec::new(),
            full: false,
            across: 1.0,
            bottom: false,
            size: Some((96.0, 18.0)),
            offset: (12.0, -9.0),
            rotation: 0.0,
        });
        let tiles = card_tiles(&cards);
        let (bottom, picks) = &tiles[0];
        assert!(!bottom);
        // Half above the card's top, then its own offset.
        assert_eq!(picks[0].offset, (12.0, -18.0));
        assert_eq!(picks[0].layer.position, (1.0, 0.0));
        assert_eq!(picks[0].size, Some((96.0, 18.0)));
    }
    use crate::layout::LAYOUT;

    fn fetched_package(version: &str, chat: &str) -> install::Fetched {
        install::Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            format!("{version}-commit"),
            yapfast_core::themes::check::Files {
                manifest: format!(
                    r##"{{ "theme_api": 1, "id": "com.example.plum", "name": "Plum", "version": "{version}",
                    "variants": {{ "dark": {{ "base": "yap", "colours": {{ "chat": "{chat}" }} }} }} }}"##
                ),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn fetched_package_with_option() -> install::Fetched {
        install::Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            "options-commit".into(),
            yapfast_core::themes::check::Files {
                manifest: r##"{
                    "theme_api": 1,
                    "id": "com.example.plum",
                    "name": "Plum",
                    "version": "1.0.0",
                    "options": [{
                        "id": "wide",
                        "label": "Wide rail",
                        "kind": "toggle",
                        "default": false,
                        "on": { "layout.rail": "right" }
                    }],
                    "variants": { "dark": { "base": "yap" } }
                }"##
                .into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn fetched_dual_package() -> install::Fetched {
        install::Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            "dual-commit".into(),
            yapfast_core::themes::check::Files {
                manifest: r##"{
                    "theme_api": 1,
                    "id": "com.example.plum",
                    "name": "Plum",
                    "version": "1.0.0",
                    "variants": {
                        "dark": { "base": "yap" },
                        "light": { "base": "sakura" }
                    }
                }"##
                .into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn creator_package_companion_survives_layout_restart() {
        use yapfast_core::themes::creator::{self, Messages, Spec};

        let data = tempfile::tempdir().unwrap();
        let relative = "themes/creator/custom-1";
        let folder = data.path().join(relative);
        let spec = Spec {
            body: "slab".into(),
            corners: 90,
            messages: Messages::Cards,
            glass: true,
            ..Spec::default()
        };
        let derived = creator::derive(&spec);
        creator::write_folder(
            &folder,
            &spec,
            "Companion",
            "custom:1",
            &derived,
            |file| match file {
                "DisplayName-Tempo.ttf" => Some(include_bytes!(
                    "../../../crates/yapfast-iced-kit/fonts/DisplayName-Tempo.ttf"
                )),
                _ => None,
            },
        )
        .unwrap();
        let custom = Custom {
            id: "custom:1".into(),
            name: "Companion".into(),
            base: "yap",
            theme: palette::YAP,
            creator: Some(spec),
            folder: Some(relative.into()),
        };
        layout::store(data.path(), "custom_themes", [&custom]);
        layout::store(data.path(), "theme", "custom:1");
        layout::flush(data.path());

        let kept = Layout::read(data.path());
        let mut themes = Themes::read(data.path(), &kept);
        assert_eq!(themes.chosen_id(), "custom:1");
        assert_eq!(themes.companions.len(), 1, "{:?}", themes.refused);
        assert_eq!(themes.companions[0].folder, folder);
        assert_ne!(
            themes.shown().surface_radius("button", 6.0),
            theme("stock", &palette::YAP).surface_radius("button", 6.0),
            "the creator package's shape, not only its palette, reloads"
        );
        assert!(
            themes.companions[0]
                .metrics
                .types
                .get("body")
                .and_then(|style| style.font)
                .is_some(),
            "the creator package's checked font role reloads"
        );
        assert!(
            themes.companions[0].art.glass.is_some(),
            "the creator package's glass art reloads"
        );
        assert!(themes.resized((1440, 900)));
        assert_eq!(
            themes.companions[0]
                .art
                .glass
                .as_ref()
                .unwrap()
                .blurred
                .window,
            (1440, 900),
            "glass backing follows the live window size"
        );
        // The resize bakes from what was decoded once, off the window's
        // thread, and only the latest size's bake is taken.
        let pixels = |themes: &Themes| {
            let blurred = &themes.companions[0].art.glass.as_ref().unwrap().blurred;
            blurred
                .under(iced::Rectangle::new(
                    iced::Point::ORIGIN,
                    Size::new(1440.0, 900.0),
                ))
                .is_some()
        };
        assert!(pixels(&themes), "the old blur shows stretched meanwhile");
        let (jobs, generation, latest) = themes.glass_jobs().expect("a glass job");
        let baked = GlassBaked {
            generation,
            panes: jobs
                .into_iter()
                .map(|job| (job.id, bake_glass(&job.source, job.window)))
                .collect(),
        };
        assert!(themes.glass_baked(baked.clone()), "the bake is taken");
        assert!(themes.resized((1500, 900)));
        assert_ne!(latest.load(Ordering::Relaxed), generation);
        assert!(
            !themes.glass_baked(baked),
            "a bake for an older size is let go"
        );

        // Deleting that creator and later reusing its custom id for a plain
        // palette must not inherit the package's global companion metrics.
        drop(themes);
        let palette_only = Custom {
            id: "custom:1".into(),
            name: "Palette only".into(),
            base: "yap",
            theme: palette::YAP,
            creator: None,
            folder: None,
        };
        let plain_layout = Layout {
            theme: Some("custom:1".into()),
            custom_themes: vec![palette_only],
            ..Layout::default()
        };
        let plain = Themes::read(data.path(), &plain_layout).shown();
        assert_eq!(plain.surface_radius("button", 6.0), 6.0);
    }

    #[test]
    fn an_arbitrary_custom_folder_layers_over_its_layout_palette() {
        use yapfast_core::themes::creator::{self, Spec};

        let data = tempfile::tempdir().unwrap();
        let relative = "themes/custom/layered";
        let folder = data.path().join(relative);
        let spec = Spec {
            accent: 30,
            corners: 90,
            ..Spec::default()
        };
        creator::write_folder(
            &folder,
            &spec,
            "Layered",
            "custom:1",
            &creator::derive(&spec),
            |_| None,
        )
        .unwrap();

        let mut colours = palette::YAP;
        colours.avatar = Colour::rgb(0x123456);
        let custom = Custom {
            id: "custom:1".into(),
            name: "Layered".into(),
            base: "yap",
            theme: colours,
            creator: None,
            folder: Some(relative.into()),
        };
        let layout = Layout {
            theme: Some(custom.id.clone()),
            custom_themes: vec![custom],
            ..Layout::default()
        };

        let themes = Themes::read(data.path(), &layout);
        assert_eq!(themes.companions.len(), 1, "{:?}", themes.refused);
        assert_eq!(themes.shown().avatar, color(Colour::rgb(0x123456)));
        assert_ne!(
            themes.shown().surface_radius("button", 6.0),
            theme("stock", &colours).surface_radius("button", 6.0),
            "the linked folder's non-colour layers remain active"
        );
    }

    #[test]
    fn a_missing_managed_creator_folder_regenerates_from_its_saved_spec() {
        let data = tempfile::tempdir().unwrap();
        let relative = "themes/creator/1";
        let custom = Custom {
            id: "custom:1".into(),
            name: "Recovered".into(),
            base: "yap",
            theme: palette::YAP,
            creator: Some(yapfast_core::themes::creator::Spec {
                body: "slab".into(),
                glass: true,
                ..Default::default()
            }),
            folder: Some(relative.into()),
        };
        let layout = Layout {
            theme: Some(custom.id.clone()),
            custom_themes: vec![custom],
            ..Layout::default()
        };
        assert!(!data.path().join(relative).exists());
        let themes = Themes::read(data.path(), &layout);
        assert!(data.path().join(relative).join("theme.json").is_file());
        assert_eq!(themes.companions.len(), 1);
        assert_eq!(themes.chosen_id(), "custom:1");
    }

    #[test]
    fn forgetting_an_active_custom_never_keeps_its_deleted_id() {
        let data = tempfile::tempdir().unwrap();
        let custom = Custom {
            id: "custom:1".into(),
            name: "Gone".into(),
            base: "yap",
            theme: palette::YAP,
            creator: None,
            folder: None,
        };
        let layout = Layout {
            theme: Some("custom:1".into()),
            previous_theme: Some("sakura".into()),
            custom_themes: vec![custom],
            ..Layout::default()
        };
        let mut themes = Themes::read(data.path(), &layout);
        assert!(themes.forget_custom(data.path(), "custom:1"));
        assert!(themes.custom.is_empty());
        assert_eq!(themes.chosen.as_deref(), Some("sakura"));
        assert!(themes.previous.is_none());

        themes.chosen = Some("custom:missing".into());
        themes.previous = Some("also-missing".into());
        assert!(themes.forget_custom(data.path(), "custom:missing"));
        assert_eq!(themes.chosen.as_deref(), Some("yap"));
        assert!(themes.previous.is_none());
    }

    #[test]
    fn package_lifecycle_refreshes_without_losing_preferences() {
        let folder = tempfile::tempdir().unwrap();
        let layout_id = "installed:com.example.plum";
        std::fs::write(
            folder.path().join(LAYOUT),
            format!(
                r#"{{
                    "theme": "sakura",
                    "previous_theme": "yap",
                    "theme_options": {{ "{layout_id}": {{ "kept": true }} }},
                    "theme_content": ["{layout_id}"],
                    "theme_sounds": {{ "{layout_id}": {{ "on": false, "interface": true, "volume": 37 }} }},
                    "zoom": 1.2
                }}"#
            ),
        )
        .unwrap();
        let mut layout = Layout::read(folder.path());
        let mut themes = Themes::read(folder.path(), &layout);

        assert_eq!(
            themes
                .install_fetched(folder.path(), &layout, &fetched_package("1.0.0", "#2a1a3a"))
                .unwrap(),
            layout_id
        );
        assert_eq!(themes.installed[0].version, "1.0.0");
        assert_eq!(themes.installed[0].choices["kept"], true);
        assert_eq!(
            themes.sound_prefs(layout_id),
            Some(SoundPrefs {
                on: false,
                interface: true,
                volume: 37,
            })
        );
        assert!(layout.theme_content.contains(layout_id));

        let error = themes
            .roll_back(folder.path(), &layout, "com.example.plum")
            .unwrap_err();
        assert_eq!(error, "there's no earlier version to go back to");
        themes.choose(folder.path(), layout_id);
        themes
            .install_fetched(folder.path(), &layout, &fetched_package("1.1.0", "#2b1b3b"))
            .unwrap();
        assert_eq!(themes.installed[0].version, "1.1.0");
        // Roll back names the version it keeps, read from its stored files.
        assert_eq!(themes.installed[0].previous.as_deref(), Some("v1.0.0"));
        assert_eq!(themes.chosen.as_deref(), Some(layout_id));
        assert_eq!(themes.previous.as_deref(), Some("sakura"));
        themes
            .roll_back(folder.path(), &layout, "com.example.plum")
            .unwrap();
        assert_eq!(themes.installed[0].version, "1.0.0");
        assert!(themes.installed[0].previous.is_none());
        assert_eq!(themes.installed[0].choices["kept"], true);
        assert!(layout.theme_content.contains(layout_id));

        assert!(
            themes
                .uninstall(folder.path(), &mut layout, "com.example.plum")
                .unwrap()
        );
        assert!(themes.installed.is_empty());
        assert_eq!(themes.chosen.as_deref(), Some("sakura"));
        assert!(themes.previous.is_none());
        assert_eq!(layout.theme.as_deref(), Some("sakura"));
        assert!(layout.previous_theme.is_none());
        assert!(!layout.theme_options.contains_key(layout_id));
        assert!(!layout.theme_sounds.contains_key(layout_id));
        assert!(!layout.theme_content.contains(layout_id));

        layout::flush(folder.path());
        let kept = Layout::read(folder.path());
        assert_eq!(kept.theme.as_deref(), Some("sakura"));
        assert!(kept.previous_theme.is_none());
        assert_eq!(kept.zoom, Some(1.2), "unrelated layout keys stay");
        assert!(!kept.theme_options.contains_key(layout_id));
        assert!(!kept.theme_sounds.contains_key(layout_id));
        assert!(!kept.theme_content.contains(layout_id));
    }

    #[test]
    fn uninstall_does_not_restore_a_missing_back_target() {
        let data = tempfile::tempdir().unwrap();
        let mut layout = Layout {
            theme: Some("installed:com.example.plum".into()),
            previous_theme: Some("installed:gone".into()),
            ..Layout::default()
        };
        let mut themes = Themes::read(data.path(), &layout);
        themes
            .install_fetched(data.path(), &layout, &fetched_package("1.0.0", "#2a1a3a"))
            .unwrap();
        assert!(
            themes
                .uninstall(data.path(), &mut layout, "com.example.plum")
                .unwrap()
        );
        assert!(themes.chosen.is_none());
        assert!(themes.previous.is_none());
        assert_eq!(themes.shown().id, DEFAULT);
        assert!(layout.theme.is_none());
        assert!(layout.previous_theme.is_none());
    }

    #[test]
    fn removing_the_theme_on_trial_ends_the_trial_and_shows_what_was_before() {
        let data = tempfile::tempdir().unwrap();
        let mut layout = Layout {
            theme: Some("sakura".into()),
            ..Layout::default()
        };
        let mut themes = Themes::read(data.path(), &layout);
        let id = themes
            .install_fetched(data.path(), &layout, &fetched_package("1.0.0", "#2a1a3a"))
            .unwrap();
        themes.try_installed(&id).unwrap();
        assert_eq!(themes.shown().id, id);

        let changed = themes
            .uninstall(data.path(), &mut layout, "com.example.plum")
            .unwrap();
        assert!(changed, "what shows changed, so the window must show it");
        assert!(themes.trial().is_none());
        assert_eq!(themes.shown().id, "sakura");
        assert_eq!(layout.theme.as_deref(), Some("sakura"));
    }

    #[test]
    fn default_theme_this_run_never_loads_the_theme_it_escapes() {
        let data = tempfile::tempdir().unwrap();
        let mut layout = Layout::default();
        let id = Themes::read(data.path(), &layout)
            .install_fetched(data.path(), &layout, &fetched_package("1.0.0", "#2a1a3a"))
            .unwrap();
        layout.theme = Some(id.clone());

        let themes = Themes::start_with(data.path(), &layout, (1024, 768), true, None);
        let package = themes
            .installed
            .iter()
            .find(|package| package.id == id)
            .unwrap();
        assert!(!package.live, "its art, sounds and faces aren't loaded");
        assert_eq!(themes.shown().id, DEFAULT);
        assert_eq!(
            themes.previous.as_deref(),
            Some(id.as_str()),
            "Back offers it"
        );

        let themes = Themes::start_with(data.path(), &layout, (1024, 768), false, None);
        assert!(
            themes
                .installed
                .iter()
                .any(|package| package.id == id && package.live)
        );
    }

    #[test]
    fn theme_dev_reads_a_failing_folder_once_per_save_and_the_emergency_key_stops_it() {
        let data = tempfile::tempdir().unwrap();
        let dev = tempfile::tempdir().unwrap();
        let manifest = dev.path().join("theme.json");
        std::fs::write(&manifest, "{ not a theme").unwrap();
        let layout = Layout::default();
        let mut themes = Themes::start_with(
            data.path(),
            &layout,
            (1024, 768),
            false,
            Some(dev.path().to_owned()),
        );
        assert!(themes.dev.is_none(), "it failed its checks");
        assert!(themes.dev_seen.is_some(), "a failed read is remembered");
        assert!(!themes.dev_saved(&layout), "not read again before a save");
        assert!(!themes.dev_saved(&layout));

        std::fs::write(
            &manifest,
            r##"{ "theme_api": 1, "id": "com.example.dev", "name": "Dev", "version": "1.0.0",
                "variants": { "dark": { "base": "yap", "colours": { "chat": "#2a1a3a" } } } }"##,
        )
        .unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&manifest)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let read = themes.dev_saved(&layout);
        assert!(read, "a save is read: {:?}", themes.read_dev(&layout));
        assert_eq!(themes.shown().id, DEV_ID);
        assert!(!themes.dev_saved(&layout), "and only once");

        themes.turn_off(data.path());
        assert_ne!(
            themes.shown().id,
            DEV_ID,
            "Ctrl+Shift+T leaves the dev theme"
        );
        assert!(!themes.developing(), "and stops watching the folder");
        assert!(!themes.dev_saved(&layout));
    }

    #[test]
    fn a_shown_themes_transition_comes_from_its_resolution_not_the_disk() {
        let data = tempfile::tempdir().unwrap();
        let layout = Layout::default();
        let mut themes = Themes::read(data.path(), &layout);
        let fetched = install::Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            "transition-commit".into(),
            yapfast_core::themes::check::Files {
                manifest: r##"{ "theme_api": 1, "id": "com.example.plum", "name": "Plum",
                    "version": "1.0.0", "art": { "transition": { "kind": "fade", "duration": 260 } },
                    "variants": { "dark": { "base": "yap", "colours": { "chat": "#2a1a3a" } } } }"##
                    .into(),
                ..Default::default()
            },
        )
        .unwrap();
        let id = themes
            .install_fetched(data.path(), &layout, &fetched)
            .unwrap();
        themes.choose(data.path(), &id);
        // Gone from the store, the shown theme still knows its transition:
        // a channel switch reads nothing from disk.
        std::fs::remove_dir_all(data.path().join("themes")).ok();
        let transition = themes.transition(&id).expect("a transition");
        assert_eq!(transition.kind, art::TransitionKind::Fade);
        assert_eq!(transition.duration, 260.0);
        assert!(themes.transition(DEFAULT).is_none());
    }

    #[test]
    fn trial_folders_left_behind_are_swept_but_not_live_ones() {
        let temp = tempfile::tempdir().unwrap();
        let left = temp.path().join(format!("{TRIAL_FOLDER}left"));
        let live = temp.path().join(format!("{TRIAL_FOLDER}live"));
        let other = temp.path().join("someone-elses");
        for folder in [&left, &live, &other] {
            std::fs::create_dir(folder).unwrap();
        }
        let long_ago = std::time::SystemTime::now() - Duration::from_secs(2 * 60 * 60);
        for folder in [&left, &other] {
            std::fs::File::open(folder)
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
        }
        sweep_trials(temp.path(), Duration::from_secs(60 * 60));
        assert!(!left.exists());
        assert!(live.exists(), "a trial running now keeps its folder");
        assert!(other.exists());
    }

    /// The kit's copy of the easing curves (it can't depend on core) moves
    /// as core's does, so a popup and a theme animation on one curve agree.
    #[test]
    fn the_kits_easing_matches_cores_on_every_curve() {
        use package::Curve;
        for curve in [
            Curve::Linear,
            Curve::Ease,
            Curve::EaseIn,
            Curve::EaseOut,
            Curve::EaseInOut,
            Curve::Spring,
            Curve::Bounce,
            Curve::Elastic,
        ] {
            let declared = package::Motion {
                curve: Some(curve),
                open: Some(package::Transition::Fade),
                ..package::Motion::default()
            };
            let kit = motion(&declared, Performance::Full, false);
            for step in 0..=20 {
                let t = step as f32 / 20.0;
                let (ours, core) = (kit.ease(t), yapfast_core::themes::motion::ease(curve, t));
                assert!(
                    (ours - core).abs() < 1e-3,
                    "{curve:?} at {t}: {ours} vs {core}"
                );
            }
        }
    }

    #[test]
    fn resetting_an_option_to_default_does_not_pin_it() {
        let data = tempfile::tempdir().unwrap();
        let layout = Layout::default();
        let mut themes = Themes::read(data.path(), &layout);
        let id = themes
            .install_fetched(data.path(), &layout, &fetched_package_with_option())
            .unwrap();

        let non_default = themes
            .set_option(
                &id,
                "wide",
                serde_json::Value::Bool(true),
                Performance::Full,
                false,
            )
            .unwrap();
        assert_eq!(non_default["wide"], true);
        let reset = themes
            .set_option(
                &id,
                "wide",
                serde_json::Value::Bool(false),
                Performance::Full,
                false,
            )
            .unwrap();
        assert!(reset.is_empty());
        assert!(themes.package(&id).unwrap().choices.is_empty());
    }

    #[test]
    fn installed_trials_keep_revert_and_expire_without_persisting_early() {
        let data = tempfile::tempdir().unwrap();
        let layout = Layout {
            theme: Some("sakura".into()),
            ..Layout::default()
        };
        let mut themes = Themes::read(data.path(), &layout);
        let id = themes
            .install_fetched(data.path(), &layout, &fetched_package("1.0.0", "#2a1a3a"))
            .unwrap();

        themes.try_installed(&id).unwrap();
        assert_eq!(themes.chosen_id(), id);
        assert_eq!(themes.chosen.as_deref(), Some("sakura"));
        assert_eq!(themes.revert_trial(), Some(Some("sakura".into())));
        assert_eq!(themes.chosen_id(), "sakura");

        themes.try_installed(&id).unwrap();
        let expired_at = themes.trial.as_ref().unwrap().until;
        assert_eq!(themes.expire_trial(expired_at), Some(Some("sakura".into())));
        assert!(themes.trial().is_none());

        themes.try_installed(&id).unwrap();
        assert_eq!(themes.keep_trial(data.path(), &layout).unwrap(), Some(id));
        assert!(themes.trial().is_none());
        assert_eq!(themes.chosen.as_deref(), Some("sakura"));
    }

    #[test]
    fn auto_variant_follows_system_without_changing_the_persisted_id() {
        let data = tempfile::tempdir().unwrap();
        let layout = Layout::default();
        let mut themes = Themes::read(data.path(), &layout);
        let id = themes
            .install_fetched(data.path(), &layout, &fetched_dual_package())
            .unwrap();
        themes.choose(data.path(), &id);
        assert_eq!(themes.shown().chat, color(palette::YAP.chat));

        assert!(themes.system_mode_changed(Mode::Light));
        assert_eq!(
            themes.shown().chat,
            color(palette::find("sakura").unwrap().theme.chat)
        );
        assert_eq!(themes.chosen.as_deref(), Some(id.as_str()));

        themes
            .set_option(
                &id,
                options::MODE,
                serde_json::Value::String("dark".into()),
                Performance::Full,
                false,
            )
            .unwrap();
        assert_eq!(themes.shown().chat, color(palette::YAP.chat));
        assert!(!themes.system_mode_changed(Mode::Dark));
        assert!(!themes.system_mode_changed(Mode::Light));
        assert_eq!(themes.shown().chat, color(palette::YAP.chat));
    }

    #[test]
    fn reviewed_trial_installs_only_when_kept() {
        let data = tempfile::tempdir().unwrap();
        let layout = Layout {
            theme: Some("sakura".into()),
            ..Layout::default()
        };
        let fetched = fetched_package("1.0.0", "#2a1a3a");
        let mut themes = Themes::read(data.path(), &layout);

        themes.try_reviewed(&fetched, &layout).unwrap();
        assert_eq!(themes.chosen_id(), TRIAL_ID);
        assert!(themes.installed.is_empty());
        assert_eq!(themes.revert_trial(), Some(Some("sakura".into())));
        assert!(themes.installed.is_empty());

        themes.try_reviewed(&fetched, &layout).unwrap();
        let kept = themes.keep_trial(data.path(), &layout).unwrap().unwrap();
        assert_eq!(kept, "installed:com.example.plum");
        assert_eq!(themes.installed.len(), 1);
        assert_eq!(themes.chosen.as_deref(), Some("sakura"));
    }

    #[test]
    fn a_layout_without_a_theme_shows_bonfire_and_an_unknown_one_yap() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path()))
                .shown()
                .id,
            "bonfire"
        );
        std::fs::write(
            folder.path().join(LAYOUT),
            r#"{"theme": "gone", "zoom": 1.2}"#,
        )
        .unwrap();
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path()))
                .shown()
                .id,
            "yap"
        );
    }

    #[test]
    fn choosing_a_theme_keeps_the_rest_of_the_layout() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LAYOUT),
            r##"{"zoom": 1.2, "custom_themes": [{"id": "custom:1", "name": "Plum",
                "base": "sakura", "colours": {"chat": "#2a1a3a"}}]}"##,
        )
        .unwrap();
        let mut themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        let plum = themes.find("custom:1").unwrap();
        assert!(!plum.dark, "Sakura's");
        assert_eq!(plum.chat, crate::style::hex(0x2a1a3a));
        themes.choose(folder.path(), "custom:1");
        crate::layout::flush(folder.path());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.path().join(LAYOUT)).unwrap()).unwrap();
        assert_eq!(written["theme"], "custom:1");
        assert_eq!(written["zoom"], 1.2);
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path())).shown(),
            plum
        );
    }

    #[test]
    fn the_worked_out_colours_follow_the_theme() {
        let yap = Themes {
            custom: Vec::new(),
            installed: Vec::new(),
            companions: Vec::new(),
            refused: Vec::new(),
            chosen: Some("yap".into()),
            previous: None,
            trial: None,
            creator_preview: None,
            dev: None,
            dev_seen: None,
            dev_off: false,
            dev_folder: None,
            glass_generation: Arc::default(),
            system_mode: Mode::Dark,
            window: (1024, 768),
        }
        .shown();
        // Discord's menu on its dark theme is 40, 40, 45: a step over raised.
        let [r, g, b, _] = yap.menu.into_rgba8();
        assert!(r > 0x26 && g > 0x27 && b > 0x2e);
        assert_eq!(yap.danger_text, crate::style::hex(0xfa777c));
        let sakura = Themes {
            custom: Vec::new(),
            installed: Vec::new(),
            companions: Vec::new(),
            refused: Vec::new(),
            chosen: Some("sakura".into()),
            previous: None,
            trial: None,
            creator_preview: None,
            dev: None,
            dev_seen: None,
            dev_off: false,
            dev_folder: None,
            glass_generation: Arc::default(),
            system_mode: Mode::Dark,
            window: (1024, 768),
        }
        .shown();
        assert_eq!(sakura.menu, sakura.raised);
        // A light theme's danger text is its danger red, only as much
        // darker as it takes to read at 4.5:1 on its chat.
        assert!(yapfast_iced_kit::theme::contrast(sakura.danger_text, sakura.chat) >= 4.5);
        assert!(
            sakura.danger_text.r <= sakura.danger.r
                && sakura.danger.r - sakura.danger_text.r < 0.05
        );
    }

    #[test]
    fn a_package_is_resolved_over_its_base_before_it_reaches_the_kit() {
        let package = yapfast_core::themes::Package::parse(
            r##"{
                "theme_api": 1,
                "id": "dev.example.plum",
                "name": "Plum",
                "version": "1.0.0",
                "variants": {
                    "dark": {
                        "base": "yap",
                        "colours": { "chat": "#2a1a3a", "accent": "#c04080" }
                    }
                }
            }"##,
        )
        .expect("a package");
        let resolved = package
            .resolve_with(
                package.mode_for(Mode::Dark),
                &yapfast_core::themes::options::Choices::new(),
            )
            .expect("resolved");
        let palette = resolved_palette(&resolved);
        let shown = theme("installed:dev.example.plum", &palette);
        assert_eq!(shown.chat, crate::style::hex(0x2a1a3a));
        assert_eq!(shown.accent, crate::style::hex(0xc04080));
        assert_eq!(palette.rail, palette::YAP.rail, "the base fills gaps");
    }

    #[test]
    fn resolved_shapes_spacing_and_motion_reach_the_kit_theme() {
        let package = yapfast_core::themes::Package::parse(
            r##"{
                "theme_api": 1,
                "id": "dev.example.metrics",
                "name": "Metrics",
                "version": "1.0.0",
                "colours": {
                    "chat": "linear(135deg, #102030, #405060)",
                    "members": "#203040",
                    "composer": "#304050"
                },
                "shape": {
                    "button": { "radius": 13, "border": 2, "border_colour": "#c04080" },
                    "avatar": { "radius": 20 },
                    "row": { "radius": 4, "inset": [8, 2], "stripe": { "colour": "#ffffff11", "lists": ["messages"] } },
                    "label": { "ground": "#224466", "text": "#ddeeff", "case": "upper" },
                    "card": { "inset_top": 12 },
                    "card_section": { "ground": "#182838", "bar": "#446688", "bar_text": "#ffffff", "case": "lower" },
                    "radii": { "small": 3, "medium": 7, "large": 11 },
                    "outline_width": 1.5,
                    "borders": { "dialog": { "style": "dotted", "gap": 3, "width": 2, "colour": "#141414" } }
                },
                "spacing": {
                    "message_gap": 5,
                    "channel_row_height": 39,
                    "sidebar_width": 333
                },
                "type": {
                    "body": { "size": 17, "weight": 300 },
                    "headings": { "size": 20, "weight": 800, "uppercase": true, "header_size": 22, "title_size": 28 },
                    "names": { "size": 16, "weight": 700 }
                },
                "layout": {
                    "channel_rows": { "height": 41 },
                    "member_rows": { "height": 52 }
                },
                "art": { "footer_height": 72, "surfaces": {
                    "chat": { "texture": "paper", "reserve": { "left": 24, "bottom": 16 } },
                    "footer": { "texture": "paper" }
                } },
                "motion": { "quick": 90, "popup": 240, "hover_in": 70, "hover_out": 110, "curve": "linear", "hover": "lift", "press": "scale", "open": "zoom" },
                "variants": { "dark": { "base": "yap" } }
            }"##,
        )
        .expect("a package");
        let resolved = package
            .resolve_with(
                package.mode_for(Mode::Dark),
                &yapfast_core::themes::options::Choices::new(),
            )
            .expect("resolved");
        let shown = theme(
            "installed:dev.example.metrics",
            &resolved_palette(&resolved),
        );
        let mut kit_metrics = metrics(&resolved, Performance::Full, false);
        kit_metrics.types = typography(Path::new("."), &resolved);
        shown.register_metrics(kit_metrics);
        assert!(matches!(
            shown.package_paint("chat"),
            Some(iced::Background::Gradient(_))
        ));
        assert_eq!(
            shown.package_paint("members"),
            Some(argb(0xff20_3040).into())
        );
        assert_eq!(
            shown.package_paint("composer"),
            Some(argb(0xff30_4050).into())
        );
        let border = shown.surface_border("button", 6.0, 1.0, Color::WHITE);
        assert_eq!(border.radius, 13.0.into());
        assert_eq!(border.width, 2.0);
        assert_eq!(border.color, argb(0xffc0_4080));
        assert_eq!(shown.surface_radius("field", 6.0), 7.0);
        assert_eq!(shown.avatar_radius(40.0, 20.0), 8.0);
        assert_eq!(shown.row_radius(0.0), 4.0);
        assert_eq!(shown.surface_inset("row"), Some((8.0, 2.0)));
        assert_eq!(shown.surface_inset_top("card"), Some(12.0));
        assert!(shown.has_surface("card_section"));
        assert_eq!(
            shown.surface_words("card_section", "Member Since"),
            "member since"
        );
        assert_eq!(
            shown.surface_bar("card_section"),
            Some((argb(0xff44_6688).into(), Some(Color::WHITE)))
        );
        assert_eq!(shown.surface_words("label", "People"), "PEOPLE");
        assert_eq!(shown.type_size("body", 14.0), 17.0);
        assert_eq!(shown.type_size("names", 14.0), 16.0);
        assert_eq!(shown.header_size(16.0), 22.0);
        assert_eq!(shown.title_size(24.0), 28.0);
        assert_eq!(shown.heading("Themes"), "THEMES");
        assert_eq!(
            shown.font("body", crate::fonts::REGULAR).weight,
            iced::font::Weight::Light
        );
        // A bold run in the body keeps its emphasis in the theme's face.
        assert_eq!(
            shown.font("body", crate::fonts::BOLD).weight,
            iced::font::Weight::Bold
        );
        assert_eq!(shown.surface_text("label"), Some(argb(0xffdd_eeff)));
        assert_eq!(
            shown.outline("dialog"),
            Some(yapfast_iced_kit::theme::Outline {
                stroke: yapfast_iced_kit::theme::OutlineStroke::Dotted,
                dash: 2.0,
                gap: 3.0,
                width: Some(2.0),
                colour: Some(argb(0xff14_1414)),
            })
        );
        assert!(
            shown
                .surface_stripe("row")
                .is_some_and(|stripe| stripe.messages)
        );
        assert_eq!(shown.spacing().message_gap, Some(5.0));
        assert_eq!(shown.spacing().channel_row_height, Some(41.0));
        assert_eq!(shown.spacing().member_row_height, Some(52.0));
        assert_eq!(shown.spacing().sidebar_width, Some(333.0));
        assert_eq!(shown.motion().popup, Duration::from_millis(240));
        assert_eq!(shown.motion().quick, Duration::from_millis(90));
        assert_eq!(shown.motion().hover_in, Duration::from_millis(70));
        assert_eq!(shown.motion().hover_out, Duration::from_millis(110));
        assert_eq!(shown.motion().hover, yapfast_iced_kit::theme::Hover::Lift);
        assert_eq!(shown.motion().press, yapfast_iced_kit::theme::Press::Scale);
        assert_eq!(
            shown.motion().open,
            yapfast_iced_kit::theme::Transition::Zoom
        );
        assert!(shown.decorated("chat"));
        assert!(shown.decorated("footer"));
        let art = theme_art(
            Path::new("."),
            &resolved,
            Performance::Balanced,
            false,
            (1024, 768),
        );
        assert_eq!(
            art.reserves.get(&ArtSurface::Chat),
            Some(&art::Reserve {
                left: 24.0,
                bottom: 16.0,
                ..art::Reserve::default()
            })
        );
        assert_eq!(art.footer_height, Some(72.0));

        shown.register_metrics(metrics(&resolved, Performance::Light, false));
        assert_eq!(shown.motion(), yapfast_iced_kit::theme::Motion::default());
    }

    #[test]
    fn turning_a_theme_off_keeps_none_and_shows_the_default() {
        let folder = tempfile::tempdir().unwrap();
        let mut themes = Themes::read(folder.path(), &Layout::default());
        themes.choose(folder.path(), "sakura");
        themes.turn_off(folder.path());
        layout::flush(folder.path());
        assert_eq!(themes.shown().id, DEFAULT);
        let layout = Layout::read(folder.path());
        assert!(layout.theme.is_none());
        assert_eq!(layout.previous_theme.as_deref(), Some("sakura"));
        assert_eq!(themes.previous(), Some(("sakura", "Sakura")));
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.path().join(LAYOUT)).unwrap()).unwrap();
        assert!(written["theme"].is_null());
        assert_eq!(written["previous_theme"], "sakura");
    }

    #[test]
    fn choosing_and_reverting_keep_the_owner_theme_history() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LAYOUT),
            r#"{"theme":"sakura","previous_theme":"yap","zoom":1.2}"#,
        )
        .unwrap();
        let mut themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        assert_eq!(themes.previous(), Some(("yap", "Yap")));

        // Choosing what is already shown leaves Back useful.
        themes.choose(folder.path(), "sakura");
        assert_eq!(themes.previous(), Some(("yap", "Yap")));

        themes.choose(folder.path(), "bonfire");
        assert_eq!(themes.previous(), Some(("sakura", "Sakura")));
        assert!(themes.revert(folder.path()));
        assert_eq!(themes.chosen_id(), "sakura");
        assert_eq!(themes.previous(), Some(("bonfire", "Bonfire")));
        layout::flush(folder.path());

        let layout = Layout::read(folder.path());
        assert_eq!(layout.theme.as_deref(), Some("sakura"));
        assert_eq!(layout.previous_theme.as_deref(), Some("bonfire"));
        assert_eq!(layout.zoom, Some(1.2), "unrelated layout keys stay");
    }

    #[test]
    fn back_after_turning_off_restores_the_theme_once() {
        let folder = tempfile::tempdir().unwrap();
        let mut themes = Themes::read(folder.path(), &Layout::default());
        themes.choose(folder.path(), "sakura");
        themes.turn_off(folder.path());
        assert!(themes.revert(folder.path()));
        assert_eq!(themes.chosen_id(), "sakura");
        assert_eq!(themes.previous(), None);
        assert!(!themes.revert(folder.path()));
        layout::flush(folder.path());

        let layout = Layout::read(folder.path());
        assert_eq!(layout.theme.as_deref(), Some("sakura"));
        assert!(layout.previous_theme.is_none());
    }

    #[test]
    fn procedural_overlays_keep_their_scope_and_performance_cost() {
        let package = yapfast_core::themes::Package::parse(
            r##"{
                "theme_api": 1,
                "id": "dev.example.overlays",
                "name": "Overlays",
                "version": "1.0.0",
                "art": { "overlays": [
                    { "kind": "scanlines", "size": 4 },
                    { "kind": "flare", "on": "chat", "heavy": true }
                ] },
                "variants": { "dark": { "base": "yap" } }
            }"##,
        )
        .expect("an overlay package");
        let resolved = package
            .resolve_with(
                package.mode_for(Mode::Dark),
                &yapfast_core::themes::options::Choices::new(),
            )
            .expect("resolved");
        let balanced = theme_art(
            Path::new("."),
            &resolved,
            Performance::Balanced,
            false,
            (1024, 768),
        );
        assert_eq!(balanced.procedural_overlays.len(), 1);
        assert_eq!(balanced.chat_procedural_overlays.len(), 1);

        let light = theme_art(
            Path::new("."),
            &resolved,
            Performance::Light,
            false,
            (1024, 768),
        );
        assert_eq!(light.procedural_overlays.len(), 1);
        assert!(light.chat_procedural_overlays.is_empty());
    }

    #[test]
    fn pace_tooltip_cues_extra_colours_and_list_typography() {
        use yapfast_core::themes::lists::{FontRole, HoverGround};
        let _ = HoverGround::Hover;
        assert_eq!(
            pace_of(
                false,
                false,
                true,
                false,
                Performance::Full,
                false,
                package::Unfocused::Full
            ),
            Pace::Still
        );
        assert_eq!(
            pace_of(
                true,
                true,
                true,
                false,
                Performance::Full,
                false,
                package::Unfocused::Full
            ),
            Pace::Still
        );
        assert_eq!(
            pace_of(
                true,
                false,
                true,
                true,
                Performance::Full,
                false,
                package::Unfocused::Full
            ),
            Pace::Slow
        );
        assert_eq!(
            pace_of(
                true,
                false,
                false,
                false,
                Performance::Full,
                false,
                package::Unfocused::Slow
            ),
            Pace::Slow
        );
        assert_eq!(
            pace_of(
                true,
                false,
                false,
                false,
                Performance::Full,
                false,
                package::Unfocused::Full
            ),
            Pace::Full
        );
        assert_eq!(
            pace_of(
                true,
                false,
                false,
                false,
                Performance::Balanced,
                false,
                package::Unfocused::Full
            ),
            Pace::Still
        );
        assert_eq!(Pace::Slow.cap_fps(60), Pace::SLOW_FPS);
        assert_eq!(Pace::Still.cap_fps(60), 0);

        let package = yapfast_core::themes::Package::parse(
            r##"{
                "theme_api": 1,
                "id": "dev.example.parity",
                "name": "Parity",
                "version": "1.0.0",
                "based_on": "Bonfire",
                "colours": {
                    "accent-text": "#ffe082",
                    "on-tooltip": "#112233",
                    "composer-focus": "#445566",
                    "row-selected-text": "#778899",
                    "card-button": "#101010",
                    "mention-chip": "#223344",
                    "scrim": "#010203"
                },
                "art": { "surfaces": { "tooltip": { "texture": "paper", "reserve": { "left": 8, "right": 8 } } } },
                "motion": { "unfocused": "still" },
                "variants": { "dark": { "base": "yap" } }
            }"##,
        )
        .expect("package");
        let resolved = package
            .resolve_with(Mode::Dark, &yapfast_core::themes::options::Choices::new())
            .expect("resolved");
        let shown = theme("installed:dev.example.parity", &resolved_palette(&resolved));
        let mut kit_metrics = metrics(&resolved, Performance::Full, false);
        let art = theme_art(
            Path::new("."),
            &resolved,
            Performance::Full,
            false,
            (1024, 768),
        );
        kit_metrics.tooltip = tooltip_surface(&art);
        kit_metrics.hover_cue = true;
        shown.register_metrics(kit_metrics);

        assert_eq!(
            shown.motion().unfocused,
            yapfast_iced_kit::theme::Unfocused::Still
        );
        assert!(shown.tooltip_surface().is_some());
        assert_eq!(
            shown.tooltip_surface().and_then(|t| t.reserve),
            Some([0.0, 8.0, 0.0, 8.0])
        );
        assert!(shown.has_hover_cue());
        assert!(!shown.has_press_cue());
        assert_eq!(shown.accent_text(), argb(0xffff_e082));
        assert_eq!(
            shown.token_colour("on-tooltip", Color::BLACK),
            argb(0xff11_2233)
        );
        assert_eq!(
            shown.token_colour("composer-focus", Color::BLACK),
            argb(0xff44_5566)
        );
        assert_eq!(
            shown.token_colour("row-selected-text", Color::BLACK),
            argb(0xff77_8899)
        );
        assert_eq!(
            shown.token_colour("card-button", Color::BLACK),
            argb(0xff10_1010)
        );
        assert_eq!(
            shown.token_colour("mention-chip", Color::BLACK),
            argb(0xff22_3344)
        );
        assert_eq!(shown.token_colour("scrim", Color::BLACK), argb(0xff01_0203));

        let font = list_font(
            &shown,
            Some(FontRole::Headings),
            "body",
            Some(700.0),
            crate::fonts::REGULAR,
        );
        assert_eq!(font.weight, iced::font::Weight::Bold);
        assert!(
            offset_shadow(Some((0xffff_0000, 1.0, 2.0)))
                .shadow
                .is_some()
        );
    }
}
