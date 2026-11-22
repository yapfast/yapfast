//! Whether a theme may be applied, installed or listed, and what it costs.
//!
//! The same checks run everywhere a theme comes in: `yapfast theme check
//! <folder>`, `--theme-dev`, an install from a repository and the
//! marketplace's CI, so an author sees at home what review would refuse.
//! Nothing here decodes more than it has to to know a file is what it says:
//! a font's tables are parsed, a picture's header is read and its pixels
//! decoded under limits, an SVG is read as XML and refused if it could run or
//! reach anything.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub mod art;
pub mod budget;
pub mod plain;

use super::layout::BUBBLE_PADDING;
use super::package::{Mode, Package, Paint, READABLE, Resolved, TEXT_ON, follows};
use super::{MANIFEST, media};

/// The largest font file.
pub const MAX_FONT_BYTES: u64 = 4 << 20;
/// The largest picture: previews and icons.
pub const MAX_IMAGE_BYTES: u64 = 2 << 20;
/// The largest picture, in pixels on either side.
pub const MAX_IMAGE_SIDE: u32 = 3840;
/// The most a whole theme may hold, `theme.json` and licence included.
pub const MAX_THEME_BYTES: u64 = 24 << 20;
/// The largest `theme.json`.
pub const MAX_MANIFEST_BYTES: u64 = 256 << 10;

/// Licence files a theme's folder may hold, one of which a listed theme
/// needs.
pub const LICENCE_FILES: [&str; 6] = [
    "LICENSE",
    "LICENSE.txt",
    "LICENSE.md",
    "LICENCE",
    "LICENCE.txt",
    "OFL.txt",
];

/// A built-in theme's colour as ARGB, by the theme's id and the colour's
/// name in [`COLOURS`](super::package::COLOURS): what a theme's own colours
/// are checked over.
pub fn base_colour(base: &str, token: &str) -> Option<u32> {
    crate::theme::find(base)?
        .theme
        .by_token(token)
        .map(crate::theme::Colour::argb)
}

/// How strict to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strictness {
    /// Applying or installing: what would break the window or reach outside
    /// the theme.
    Install,
    /// Listing in the marketplace: also a licence, a GitHub source, a
    /// description and a preview, which review reads.
    Marketplace,
}

/// A theme's files, read: `theme.json` and every file it names, by their
/// path inside the folder.
#[derive(Clone, Debug, Default)]
pub struct Files {
    pub manifest: String,
    pub files: BTreeMap<PathBuf, Vec<u8>>,
    /// The licence file the folder holds, if any, and its text.
    pub licence: Option<(PathBuf, Vec<u8>)>,
}

impl Files {
    /// Reads a theme's folder: `theme.json`, the files it names and a licence
    /// file, refusing anything too large before reading it.
    pub fn read(folder: &Path) -> Result<Self, String> {
        let manifest_path = folder.join(MANIFEST);
        let size = std::fs::metadata(&manifest_path)
            .map_err(|_| format!("{} has no {MANIFEST}", folder.display()))?
            .len();
        if size > MAX_MANIFEST_BYTES {
            return Err(format!("{MANIFEST} is at most 256 KiB"));
        }
        let manifest = std::fs::read_to_string(&manifest_path)
            .map_err(|error| format!("{MANIFEST} couldn't be read: {error}"))?;
        let mut files = BTreeMap::new();
        if let Ok(package) = Package::parse(&manifest) {
            let mut total = size;
            let root = std::fs::canonicalize(folder).ok();
            for relative in package.files() {
                let path = folder.join(&relative);
                // A link could name a file outside the folder, as the file
                // itself or as a folder on the way to it; a theme's files are
                // its own.
                let metadata = match std::fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                if !metadata.is_file() || !within(root.as_deref(), &path) {
                    continue;
                }
                total += metadata.len();
                if metadata.len() > MAX_FONT_BYTES.max(MAX_IMAGE_BYTES) || total > MAX_THEME_BYTES {
                    return Err(format!(
                        "{} is too large: a theme holds at most 24 MiB, a font 4 MiB and a picture 2 MiB",
                        relative.display()
                    ));
                }
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("{} couldn't be read: {error}", relative.display()))?;
                files.insert(relative, bytes);
            }
        }
        let licence = LICENCE_FILES
            .iter()
            .map(PathBuf::from)
            .find(|name| {
                std::fs::symlink_metadata(folder.join(name))
                    .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 1 << 20)
            })
            .and_then(|name| Some((name.clone(), std::fs::read(folder.join(&name)).ok()?)));
        Ok(Files {
            manifest,
            files,
            licence,
        })
    }

    /// Bytes on disk the theme takes.
    pub fn bytes(&self) -> u64 {
        self.manifest.len() as u64
            + self
                .files
                .values()
                .map(|bytes| bytes.len() as u64)
                .sum::<u64>()
    }
}

/// Whether `path` is inside `root` (already canonical) once every link on
/// the way is followed.
pub(crate) fn within(root: Option<&Path>, path: &Path) -> bool {
    match (root, std::fs::canonicalize(path)) {
        (Some(root), Ok(path)) => path.starts_with(root),
        _ => false,
    }
}

/// What a check found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Why the theme can't be used; empty when it can.
    pub errors: Vec<String>,
    /// What review would ask about, or what a person should know.
    pub warnings: Vec<String>,
    /// What this Yapfast ignored.
    pub notes: Vec<String>,
    pub package: Option<Package>,
    pub cost: Cost,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }

    /// The first reason it can't be used, as one [`super::sentence`]: an
    /// error can quote the theme's own words, which must not reach a screen
    /// as more lines, turned around or as long as the theme likes.
    pub fn reason(&self) -> Option<String> {
        self.errors.first().map(|error| super::sentence(error))
    }

    /// What `yapfast theme check` prints.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if let Some(package) = &self.package {
            let _ = writeln!(
                out,
                "{} {} ({}), {}",
                package.name,
                package.version,
                package.id,
                package
                    .modes()
                    .iter()
                    .map(|mode| mode.name())
                    .collect::<Vec<_>>()
                    .join(" and ")
            );
        }
        for error in &self.errors {
            let _ = writeln!(out, "error: {}", super::sentence(error));
        }
        for warning in &self.warnings {
            let _ = writeln!(out, "warning: {}", super::shown(warning));
        }
        for note in &self.notes {
            let _ = writeln!(out, "note: {}", super::shown(note));
        }
        let _ = writeln!(out, "cost: {}", self.cost.text());
        out.push_str(if self.passed() {
            "passed\n"
        } else {
            "failed\n"
        });
        out
    }
}

/// What a theme costs to run, measured from what it holds: shown before it's
/// installed, and against the Performance setting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cost {
    /// Bytes its fonts keep in memory once applied.
    pub memory: u64,
    /// Declared animations, and how many of them stay still in Balanced.
    pub animations: usize,
    pub heavy: usize,
    /// The most redraws a second its animations ask for in Full.
    pub redraws: u32,
    pub effects: usize,
    pub fonts: usize,
    /// Pictures and textures on surfaces, overlays over the window, and
    /// particle sets.
    pub layers: usize,
    pub overlays: usize,
    pub particles: usize,
    /// Sound files and replaced icons.
    pub sounds: usize,
    pub icons: usize,
    /// What its layout changes, as a short phrase such as `layout: irc, rail
    /// top`; it costs no more than the stock window.
    pub looks: Vec<String>,
    /// About how much of a processor core its always-on motion keeps busy
    /// while nobody touches the window, in Full and in Balanced, at most in
    /// one variant: see [`budget`].
    pub idle: f32,
    pub idle_balanced: f32,
}

impl Cost {
    /// As the marketplace and the review show it: `~1.2 MB memory · 2
    /// animations (1 heavy) · up to 60 redraws a second`.
    pub fn text(&self) -> String {
        let mut parts = vec![if self.memory == 0 {
            "no extra memory".to_owned()
        } else {
            format!("~{:.1} MB memory", self.memory as f64 / 1_000_000.0)
        }];
        if self.fonts > 0 {
            parts.push(format!(
                "{} font{}",
                self.fonts,
                if self.fonts == 1 { "" } else { "s" }
            ));
        }
        match (self.animations, self.heavy) {
            (0, _) => parts.push("still".to_owned()),
            (count, 0) => parts.push(format!(
                "{count} animation{}",
                if count == 1 { "" } else { "s" }
            )),
            (count, heavy) => parts.push(format!(
                "{count} animation{} ({heavy} heavy)",
                if count == 1 { "" } else { "s" }
            )),
        }
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        if self.layers > 0 {
            parts.push(plural(self.layers, "art layer", "art layers"));
        }
        if self.overlays > 0 {
            parts.push(plural(self.overlays, "overlay", "overlays"));
        }
        if self.particles > 0 {
            parts.push(plural(self.particles, "particle set", "particle sets"));
        }
        if self.redraws > 0 {
            parts.push(format!("up to {} redraws a second", self.redraws));
        }
        if self.idle >= 0.01 {
            parts.push(format!(
                "~{:.2} {} idle ({:.2} in Balanced){}",
                self.idle,
                if self.idle < 1.005 { "core" } else { "cores" },
                self.idle_balanced,
                if self.idle > 1.0 {
                    ", more than the window can draw"
                } else {
                    ""
                }
            ));
        }
        if self.effects > 0 {
            parts.push(format!(
                "{} text effect{}",
                self.effects,
                if self.effects == 1 { "" } else { "s" }
            ));
        }
        if self.sounds > 0 {
            parts.push(format!(
                "{} sound{}",
                self.sounds,
                if self.sounds == 1 { "" } else { "s" }
            ));
        }
        if self.icons > 0 {
            parts.push(format!(
                "{} icon{}",
                self.icons,
                if self.icons == 1 { "" } else { "s" }
            ));
        }
        parts.extend(self.looks.iter().cloned());
        parts.join(" · ")
    }

    fn of(resolved: &[Resolved], files: &Files, sound_seconds: &BTreeMap<PathBuf, f32>) -> Cost {
        let mut fonts: Vec<&Path> = Vec::new();
        let mut cost = Cost::default();
        let mut art_memory = 0;
        let mut media: Vec<&Path> = Vec::new();
        for variant in resolved {
            // A sound is held decoded, 48 kHz stereo 16-bit, once; an icon as
            // its file, and drawn at the size it shows.
            for file in variant
                .sounds
                .values()
                .flat_map(|choice| choice.files.iter())
            {
                if !media.contains(&file.as_path()) {
                    media.push(file);
                    cost.sounds += 1;
                    cost.memory += sound_seconds
                        .get(file)
                        .map_or(0, |seconds| (seconds * 48_000.0 * 4.0) as u64);
                }
            }
            for icon in variant.icons.values() {
                if !media.contains(&icon.file.as_path()) {
                    media.push(&icon.file);
                    cost.icons += 1;
                    cost.memory += files
                        .files
                        .get(&icon.file)
                        .map_or(0, |bytes| bytes.len() as u64);
                }
            }
            for font in variant.fonts() {
                if !fonts.contains(&font) {
                    fonts.push(font);
                    // A font is held whole once registered, as its file.
                    cost.memory += files.files.get(font).map_or(0, |bytes| bytes.len() as u64);
                }
            }
            cost.animations = cost.animations.max(variant.animations.len());
            cost.heavy = cost.heavy.max(
                variant
                    .animations
                    .iter()
                    .filter(|animation| animation.heavy)
                    .count(),
            );
            cost.effects = cost.effects.max(variant.effects.len());
            // Art: its pictures decoded, and what of it moves.
            let art = &variant.art;
            let (moving, heavy) = art.moving();
            cost.animations = cost.animations.max(variant.animations.len() + moving);
            cost.heavy = cost.heavy.max(
                variant
                    .animations
                    .iter()
                    .filter(|animation| animation.heavy)
                    .count()
                    + heavy,
            );
            if moving > 0 {
                cost.redraws = cost.redraws.max(60);
            }
            art_memory = art_memory.max(art::memory(variant, files));
            let (idle, balanced) = budget::idle(&budget::pieces(variant));
            cost.idle = cost.idle.max(idle);
            cost.idle_balanced = cost.idle_balanced.max(balanced);
            cost.layers = cost.layers.max(art.surfaces.values().map(Vec::len).sum());
            cost.overlays = cost.overlays.max(art.overlays.len());
            cost.particles = cost.particles.max(art.particles.len());
            for animation in &variant.animations {
                let rate = super::motion::frame_rate(animation, super::Performance::Full);
                cost.redraws = cost.redraws.max(rate);
            }
        }
        cost.fonts = fonts.len();
        cost.memory += art_memory;
        for variant in resolved {
            for look in looks(variant).iter().map(|look| super::shown(look)) {
                if !cost.looks.contains(&look) {
                    cost.looks.push(look);
                }
            }
        }
        cost
    }
}

/// Checks a theme's files.
pub fn check(files: &Files, strictness: Strictness) -> Report {
    let mut report = Report::default();
    let package = match Package::parse(&files.manifest) {
        Ok(package) => package,
        Err(error) => {
            report.errors.push(error.0);
            return report;
        }
    };
    report.notes.clone_from(&package.notes);
    let mut resolved = Vec::new();
    for mode in package.modes() {
        match package.resolve(mode) {
            Ok(variant) => {
                match crate::theme::find(&variant.base) {
                    None => report.errors.push(format!(
                        "the {} variant's base \"{}\" isn't a theme Yapfast comes with",
                        mode.name(),
                        variant.base
                    )),
                    Some(base) if base.theme.dark != (mode == Mode::Dark) => {
                        report.errors.push(format!(
                            "the {} variant's base \"{}\" is a {} theme",
                            mode.name(),
                            variant.base,
                            if base.theme.dark { "dark" } else { "light" }
                        ));
                    }
                    Some(_) => {}
                }
                report.errors.extend(unreadable(&variant));
                // A part this Yapfast doesn't have is ignored when the theme
                // shows, so one for a later Yapfast still opens; the check
                // refuses it, so a theme isn't published with a typo.
                for sentence in variant
                    .parts
                    .unknown
                    .iter()
                    .cloned()
                    .chain(super::parts::unreadable(&variant))
                {
                    if !report.errors.contains(&sentence) {
                        report.errors.push(sentence);
                    }
                }
                let (errors, warnings) = art::readable(&variant, files);
                report.errors.extend(errors);
                report.warnings.extend(warnings);
                for sentence in unreadable_content(&variant) {
                    if !report.errors.contains(&sentence) {
                        report.errors.push(sentence);
                    }
                }
                report.warnings.extend(crowded(&variant));
                for sentence in budget::warnings(&budget::pieces(&variant)) {
                    if !report.warnings.contains(&sentence) {
                        report.warnings.push(sentence);
                    }
                }
                for sentence in unreadable_layout(&variant) {
                    if !report.errors.contains(&sentence) {
                        report.errors.push(sentence);
                    }
                }
                resolved.push(variant);
            }
            Err(error) => report.errors.push(error.0),
        }
    }
    // Said once, as errors, above.
    report.notes.retain(|note| !parts_note(note));
    check_options(&package, files, &mut report);
    // Two names one letter case apart are one file on Windows and macOS.
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for relative in package.files() {
        let folded = relative.to_string_lossy().to_lowercase();
        if let Some(known) = seen.get(&folded) {
            if *known != relative {
                report.errors.push(format!(
                    "{} and {} differ only in letter case, which Windows and macOS read as one file",
                    known.display(),
                    relative.display()
                ));
            }
        } else {
            seen.insert(folded, relative);
        }
    }
    // Each sound is decoded once, here, and its length kept for the slots
    // that name it.
    let mut sound_seconds: BTreeMap<PathBuf, f32> = BTreeMap::new();
    for relative in package.files() {
        let Some(bytes) = files.files.get(&relative) else {
            report.errors.push(format!(
                "{MANIFEST} names {}, which isn't in the theme",
                relative.display()
            ));
            continue;
        };
        let checked = if media::is_sound(&relative) {
            media::decode_sound(&relative, bytes).map(|samples| {
                sound_seconds.insert(relative.clone(), media::seconds(&samples));
            })
        } else {
            check_file(&relative, bytes)
        };
        if let Err(error) = checked {
            report
                .errors
                .push(format!("{}: {error}", relative.display()));
        }
    }
    for variant in &resolved {
        for (slot, choice) in &variant.sounds {
            for file in &choice.files {
                if let Some(seconds) = sound_seconds.get(file)
                    && *seconds > slot.longest() + 0.005
                {
                    let sentence = format!(
                        "{}: lasts {seconds:.1} s, and a {} sound lasts at most {} s",
                        file.display(),
                        slot.name(),
                        slot.longest()
                    );
                    if !report.errors.contains(&sentence) {
                        report.errors.push(sentence);
                    }
                }
            }
        }
    }
    if files.bytes() > MAX_THEME_BYTES {
        report.errors.push("a theme holds at most 24 MiB".into());
    }
    if strictness == Strictness::Marketplace {
        if package
            .license
            .as_deref()
            .is_none_or(|license| license.trim().is_empty())
        {
            report
                .errors
                .push("a listed theme names its licence in \"license\", such as OFL-1.1".into());
        }
        if files.licence.is_none() {
            report.errors.push(format!(
                "a listed theme ships its licence as {}",
                LICENCE_FILES.join(", ")
            ));
        }
        match package.source.as_deref() {
            Some(source) if github(source) => {}
            _ => report
                .errors
                .push("a listed theme names its public GitHub repository in \"source\"".into()),
        }
        if package.description.trim().is_empty() {
            report
                .errors
                .push("a listed theme says what it looks like in \"description\"".into());
        }
        if package.preview.is_empty() {
            report
                .errors
                .push("a listed theme shows at least one picture in \"preview\"".into());
        }
    } else if package.preview.is_empty() {
        report
            .warnings
            .push("no \"preview\": the marketplace needs at least one".into());
    }
    if let Some(based_on) = &package.based_on
        && !github(based_on)
    {
        report
            .warnings
            .push("\"based_on\" names a repository that isn't on GitHub".into());
    }
    if let Some(note) = display_type(&resolved, files) {
        report.notes.push(note);
    }
    report.cost = Cost::of(&resolved, files, &sound_seconds);
    report.package = Some(package);
    report
}

/// Whether a package note is about its `parts`, which the check refuses.
fn parts_note(note: &str) -> bool {
    note.starts_with("\"parts.")
        || ["dark", "light"]
            .iter()
            .any(|mode| note.starts_with(&format!("\"variants.{mode}.parts.")))
}

/// A theme's options, each moved one at a time from the defaults, which the
/// checks above cover: every field an option names has to be one this
/// Yapfast reads, what it sets has to be in range, and the text has to
/// stay readable, as for the theme itself.
fn check_options(package: &Package, files: &Files, report: &mut Report) {
    let combinations = super::options::combinations(&package.options);
    if combinations.len() < 2 {
        return;
    }
    for mode in package.modes() {
        let mut stock_notes = Vec::new();
        if package
            .resolve_with_notes(mode, &super::options::Choices::new(), &mut stock_notes)
            .is_err()
        {
            continue;
        }
        for (phrase, choices) in combinations.iter().skip(1) {
            let mut notes = Vec::new();
            let mut sentences = Vec::new();
            match package.resolve_with_notes(mode, choices, &mut notes) {
                Err(error) => sentences.push(error.0),
                Ok(variant) => {
                    for note in notes {
                        if !stock_notes.contains(&note) {
                            sentences.push(note.replace("; ignored", ""));
                        }
                    }
                    sentences.extend(unreadable(&variant));
                    sentences.extend(super::parts::unreadable(&variant));
                    sentences.extend(art::readable(&variant, files).0);
                    sentences.extend(unreadable_content(&variant));
                    sentences.extend(unreadable_layout(&variant));
                }
            }
            for sentence in sentences {
                // What the defaults already break is said once, above.
                if report.errors.contains(&sentence) {
                    continue;
                }
                let sentence = format!("{phrase} ({}): {sentence}", mode.name());
                if !report.errors.contains(&sentence) {
                    report.errors.push(sentence);
                }
            }
        }
    }
}

/// Words in family and file names that mark a blackletter face, compared
/// with spaces, `-` and `_` taken out. "Gothic" alone isn't one: it names
/// sans faces too.
const BLACKLETTER: [&str; 9] = [
    "fraktur",
    "blackletter",
    "textura",
    "schwabacher",
    "rotunda",
    "oldenglish",
    "gotisch",
    "pirataone",
    "germaniaone",
];

/// Words that mark a pixel or bitmap face, compared as [`BLACKLETTER`] is.
const PIXEL: [&str; 14] = [
    "pixel",
    "8bit",
    "bitmap",
    "silkscreen",
    "pressstart",
    "departuremono",
    "vt323",
    "dotgothic",
    "tiny5",
    "micro5",
    "jersey10",
    "m5x7",
    "m6x11",
    "monogram",
];

/// Whether a font is a display face some people find hard to read for long
/// text: `blackletter` or `pixel`, by its family names and its file's name.
pub fn display_face(path: &Path, bytes: &[u8]) -> Option<&'static str> {
    let squash = |text: &str| -> String {
        text.chars()
            .filter(|letter| !matches!(letter, ' ' | '-' | '_' | '.'))
            .flat_map(char::to_lowercase)
            .collect()
    };
    let mut names = vec![squash(
        &path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )];
    if let Ok(Ok(families)) = std::panic::catch_unwind(|| {
        ttf_parser::Face::parse(bytes, 0).map(|face| {
            face.names()
                .into_iter()
                .filter(|name| {
                    matches!(
                        name.name_id,
                        ttf_parser::name_id::FAMILY | ttf_parser::name_id::TYPOGRAPHIC_FAMILY
                    )
                })
                .filter_map(|name| name.to_string())
                .collect::<Vec<_>>()
        })
    }) {
        names.extend(families.iter().map(|family| squash(family)));
    }
    let named = |words: &[&str]| {
        names
            .iter()
            .any(|name| words.iter().any(|word| name.contains(word)))
    };
    if named(&BLACKLETTER) {
        Some("blackletter")
    } else if named(&PIXEL) {
        Some("pixel")
    } else {
        None
    }
}

/// A note for a theme that draws reading text in a display face: blackletter
/// or pixel faces for messages, names, labels or code, or ransom names. The
/// person can turn on Settings → Appearance → Readable text for any theme.
fn display_type(resolved: &[Resolved], files: &Files) -> Option<String> {
    use super::package::Role;
    let mut found: Vec<String> = Vec::new();
    let mut add = |sentence: String| {
        if !found.contains(&sentence) {
            found.push(sentence);
        }
    };
    for variant in resolved {
        for role in [Role::Body, Role::Names, Role::Labels, Role::Code] {
            let Some(font) = variant.types.get(&role).and_then(|kind| kind.font.as_ref()) else {
                continue;
            };
            let Some(bytes) = files.files.get(font) else {
                continue;
            };
            if let Some(face) = display_face(font, bytes) {
                add(format!("{} in {face} ({})", role.name(), font.display()));
            }
        }
        if variant.layout.names.is_some() {
            add("names cut as ransom letters".to_owned());
        }
    }
    (!found.is_empty()).then(|| {
        format!(
            "display type for reading text: {}. Some people find it hard to read; \
             Settings → Appearance → Readable text keeps the theme and draws messages, \
             names and labels in Yapfast's own font, without text effects",
            found.join(", ")
        )
    })
}

/// Whether a repository address is a public GitHub repository's, which is
/// where themes come from for now.
pub fn github(source: &str) -> bool {
    let Some(rest) = source.trim().strip_prefix("https://github.com/") else {
        return false;
    };
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = rest.split('/');
    let fine = |part: Option<&str>| {
        part.is_some_and(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        })
    };
    fine(parts.next()) && fine(parts.next())
}

/// Every pair of a text colour and a surface it's drawn on that would read
/// under 4.5:1, each as a sentence naming the token. A pair the base already
/// draws lower keeps the base's own contrast as its bar.
pub fn unreadable(variant: &Resolved) -> Vec<String> {
    // A colour the theme leaves, as it shows: the base's, or for a colour
    // only themes set, what it follows.
    fn shown_in(variant: &Resolved, token: &str) -> Option<Vec<u32>> {
        variant
            .paint(token)
            .map(Paint::colours)
            .or_else(|| base_colour(&variant.base, token).map(|colour| vec![colour]))
            .or_else(|| follows(token).and_then(|follow| shown_in(variant, follow)))
    }
    let own_colour = |token: &str| {
        base_colour(&variant.base, token)
            .or_else(|| follows(token).and_then(|follow| base_colour(&variant.base, follow)))
    };
    let shown = |token: &str| shown_in(variant, token);
    // Whether the theme changes what a token shows, itself or through what it
    // follows.
    fn changed(variant: &Resolved, token: &str) -> bool {
        variant.paint(token).is_some()
            || follows(token).is_some_and(|follow| changed(variant, follow))
    }
    let mut refused = Vec::new();
    for (text, surfaces) in TEXT_ON {
        for surface in surfaces.iter().copied() {
            if !changed(variant, text) && !changed(variant, surface) {
                continue;
            }
            // A see-through member list shows the conversation's ground and
            // art: `art::readable` checks its text over what shows.
            if surface == "members" && art::see_through(variant, surface) {
                continue;
            }
            let (Some(fronts), Some(backs)) = (shown(text), shown(surface)) else {
                continue;
            };
            let own = own_colour(text)
                .zip(own_colour(surface))
                .map_or(READABLE, |(front, back)| contrast(front, back));
            let needed = READABLE.min(own) - 0.005;
            let worst = fronts
                .iter()
                .flat_map(|front| backs.iter().map(move |back| contrast(*front, *back)))
                .fold(f64::INFINITY, f64::min);
            if worst < needed {
                let sentence = format!(
                    "{text} on {surface} contrasts {worst:.1}:1, under {:.1}:1; text has to stay readable",
                    needed + 0.005
                );
                if !refused.contains(&sentence) {
                    refused.push(sentence);
                }
            }
        }
    }
    refused
}

/// Reserves that leave a pane's content little room at the window's
/// smallest: about 480 pixels of height, and each pane's narrowest width.
fn crowded(variant: &Resolved) -> Vec<String> {
    use super::package::art::ArtSurface;
    let mut warnings = Vec::new();
    for (surface, width) in [
        (ArtSurface::Sidebar, 200.0),
        (ArtSurface::Members, 200.0),
        (ArtSurface::Chat, 360.0),
        (ArtSurface::Composer, 360.0),
    ] {
        let Some(reserve) = variant.art.reserve(surface) else {
            continue;
        };
        let height = if surface == ArtSurface::Composer {
            0.0
        } else {
            480.0
        };
        if (height > 0.0 && reserve.top + reserve.bottom > height * 0.7)
            || reserve.left + reserve.right > width * 0.5
        {
            warnings.push(format!(
                "the {} art's reserve leaves its content little room in a small window",
                surface.name()
            ));
        }
    }
    warnings
}

/// What a variant's `layout` changes, as the cost line lists it:
/// `layout: irc, rail top, ransom names (3 fonts)`.
fn looks(variant: &Resolved) -> Vec<String> {
    let layout = &variant.layout;
    let mut changes: Vec<String> = Vec::new();
    if let Some(style) = layout.messages {
        changes.push(style.name().to_owned());
    }
    if let Some(side) = layout.rail {
        changes.push(format!("rail {}", side.name()));
    }
    if let Some(side) = layout.members {
        changes.push(format!("members {}", side.name()));
    }
    let bubble_art = layout.bubbles.own_art.len() + layout.bubbles.other_art.len();
    if bubble_art > 0 {
        changes.push(format!(
            "{bubble_art} picture{} on bubbles",
            if bubble_art == 1 { "" } else { "s" }
        ));
    }
    if let Some(stagger) = layout.cards.stagger.filter(|stagger| *stagger > 0.0) {
        changes.push(format!("cards staggered {stagger}px"));
    }
    if let Some(side) = layout.composer {
        changes.push(format!("composer {}", side.name()));
    }
    if let Some(width) = layout.chat_max_width {
        changes.push(format!("chat at most {width}px"));
    }
    if layout.chat_avatars == Some(false) {
        changes.push("no chat avatars".to_owned());
    }
    if layout.irc.rule == Some(true) {
        changes.push("irc rule".to_owned());
    }
    if let Some(width) = layout.outline_width {
        changes.push(format!("{width}px outlines"));
    }
    if !layout.borders.is_empty() {
        changes.push(format!(
            "{} cut outline{}",
            layout.borders.len(),
            if layout.borders.len() == 1 { "" } else { "s" }
        ));
    }
    if layout.cards.shadow.is_some() {
        changes.push("card shadows".to_owned());
    }
    if !layout.cards.art.is_empty() {
        let count = layout.cards.art.len();
        let choices: usize = layout.cards.art.iter().map(|art| art.choices.len()).sum();
        changes.push(format!(
            "{count} picture{} on each card{}",
            if count == 1 { "" } else { "s" },
            if choices > 0 {
                format!(" ({} to pick from)", count + choices)
            } else {
                String::new()
            }
        ));
    }
    if layout.cards.tab.is_some() {
        changes.push("words on card tabs".to_owned());
    }
    if layout.members_style == Some(super::layout::MembersStyle::Grid) {
        changes.push(format!("member tiles ({} across)", layout.grid.columns));
    }
    for (surface, what) in [
        (super::package::Surface::Label, "label bars"),
        (super::package::Surface::Header, "own header"),
        (
            super::package::Surface::CardSection,
            "boxed profile sections",
        ),
    ] {
        if variant.shapes.contains_key(&surface) {
            changes.push(what.to_owned());
        }
    }
    if variant
        .shapes
        .get(&super::package::Surface::Row)
        .is_some_and(|shape| shape.stripe.is_some())
    {
        changes.push("striped rows".to_owned());
    }
    if let Some(names) = &layout.names {
        changes.push(if names.fonts.is_empty() {
            "ransom names".to_owned()
        } else {
            format!("ransom names ({} fonts)", names.fonts.len())
        });
    }
    for surface in super::package::art::RESERVING {
        if let Some(reserve) = variant.art.reserve(surface) {
            let edges: Vec<String> = [
                ("top", reserve.top),
                ("bottom", reserve.bottom),
                ("left", reserve.left),
                ("right", reserve.right),
            ]
            .into_iter()
            .filter(|(_, pixels)| *pixels > 0.0)
            .map(|(edge, pixels)| format!("{pixels}px {edge}"))
            .collect();
            if !edges.is_empty() {
                changes.push(format!(
                    "{} content clear of its art ({})",
                    surface.name(),
                    edges.join(", ")
                ));
            }
        }
    }
    let mut out = Vec::new();
    if !changes.is_empty() {
        out.push(format!("layout: {}", changes.join(", ")));
    }
    if variant.layout.rail == Some(super::layout::RailSide::Hidden) {
        out.push("rail: hidden, the top bar switches servers".into());
    } else if let Some(tiles) = &variant.layout.rail_tiles {
        out.push(tiles.summary());
    }
    out.extend(
        variant
            .layout
            .channel_rows
            .as_ref()
            .map(|rows| rows.summary()),
    );
    out.extend(
        variant
            .layout
            .member_rows
            .as_ref()
            .map(|rows| rows.summary()),
    );
    out.extend(variant.content.summary());
    if variant
        .art
        .events
        .contains_key(&super::package::art::EventSlot::NameHover)
    {
        out.push("names decrypt under the pointer".into());
    }
    let chrome = variant.chrome.summary();
    if !chrome.is_empty() {
        out.push(format!("chrome: {}", chrome.join(", ")));
    }
    out
}

/// The colour `content.names` draws every name in, against the grounds names
/// sit on. Clamped role colours are made readable as they're drawn, so only
/// a set colour is held to it here.
pub fn unreadable_content(variant: &Resolved) -> Vec<String> {
    let mut refused = unreadable_initials(variant);
    refused.extend(unreadable_topbar(variant));
    // Each colour of a names palette, on the chat.
    for (index, colour) in variant.content.names.palette().iter().enumerate() {
        let worst = shown_colours(variant, "chat")
            .unwrap_or_default()
            .iter()
            .map(|back| contrast(*colour, *back))
            .fold(f64::INFINITY, f64::min);
        if worst < READABLE - 0.005 {
            refused.push(format!(
                "content.names.palette[{index}] on chat contrasts {worst:.1}:1, under {READABLE:.1}:1; text has to stay readable"
            ));
        }
    }
    let Some(colour) = variant.content.names.colour else {
        return refused;
    };
    for ground in ["chat", "sidebar", "members"] {
        let Some(backs) = shown_colours(variant, ground) else {
            continue;
        };
        let worst = backs
            .iter()
            .map(|back| contrast(colour, *back))
            .fold(f64::INFINITY, f64::min);
        if worst < READABLE - 0.005 {
            refused.push(format!(
                "content.names.colour on {ground} contrasts {worst:.1}:1, under {READABLE:.1}:1; text has to stay readable"
            ));
        }
    }
    refused
}

/// An illuminated initial is a large letter, held to 3:1 against its ground
/// (or the chat's, without one), as large text is.
fn unreadable_initials(variant: &Resolved) -> Vec<String> {
    let Some(initials) = variant.content.avatars.initials else {
        return Vec::new();
    };
    let colours = match initials.colour {
        Some(colour) => vec![colour],
        None => shown_colours(variant, "accent-text").unwrap_or_default(),
    };
    let grounds = match initials.ground.filter(|ground| ground >> 24 == 0xff) {
        Some(ground) => vec![ground],
        None => shown_colours(variant, "chat").unwrap_or_default(),
    };
    let worst = colours
        .iter()
        .flat_map(|colour| grounds.iter().map(|ground| contrast(*colour, *ground)))
        .fold(f64::INFINITY, f64::min);
    if worst < 3.0 - 0.005 {
        return vec![format!(
            "content.avatars.initial's letter contrasts {worst:.1}:1 with its ground, under 3.0:1; it has to stay readable"
        )];
    }
    Vec::new()
}

/// The top bar's words on its ground.
fn unreadable_topbar(variant: &Resolved) -> Vec<String> {
    let Some(bar) = &variant.chrome.topbar else {
        return Vec::new();
    };
    let pick = |set: Option<u32>, token: &str| match set {
        Some(colour) => vec![colour],
        None => shown_colours(variant, token).unwrap_or_default(),
    };
    let grounds = pick(bar.ground, "rail");
    let mut refused = Vec::new();
    // Its words are dim, as `muted` is, and held to 3:1 as `muted` is; the
    // open workspace to 4.5:1.
    for (name, colours, least) in [
        ("colour", pick(bar.text, "muted"), 3.0),
        ("current", pick(bar.current, "text"), READABLE),
    ] {
        let worst = colours
            .iter()
            .flat_map(|colour| grounds.iter().map(|ground| contrast(*colour, *ground)))
            .fold(f64::INFINITY, f64::min);
        if worst < least - 0.005 {
            refused.push(format!(
                "chrome.topbar.{name} contrasts {worst:.1}:1 with the bar, under {least:.1}:1; text has to stay readable"
            ));
        }
    }
    refused
}

/// A colour as a variant shows it: its own, the base's, or what it follows.
pub(super) fn shown_colours(variant: &Resolved, token: &str) -> Option<Vec<u32>> {
    variant
        .paint(token)
        .map(Paint::colours)
        .or_else(|| base_colour(&variant.base, token).map(|colour| vec![colour]))
        .or_else(|| follows(token).and_then(|follow| shown_colours(variant, follow)))
}

/// Text on the grounds a layout sets: bubbles and cards. Each pair under
/// 4.5:1 is a sentence naming its place.
pub fn unreadable_layout(variant: &Resolved) -> Vec<String> {
    let layout = &variant.layout;
    let colours = |set: Option<u32>, token: &str| match set {
        Some(colour) => Some(vec![colour]),
        None => shown_colours(variant, token),
    };
    let paints = |set: Option<&Paint>, token: &str| match set {
        Some(paint) => Some(paint.colours()),
        None => shown_colours(variant, token),
    };
    // (text's name, its colours, ground's name, its colours)
    type Pair<'a> = (&'a str, Option<Vec<u32>>, &'a str, Option<Vec<u32>>);
    let mut pairs: Vec<Pair<'_>> = Vec::new();
    // Only a theme that lays messages out on these grounds, or sets one,
    // is held to them.
    let bubbles = layout.messages == Some(super::layout::MessageStyle::Bubbles);
    if bubbles || layout.bubbles.own.is_some() || layout.bubbles.own_text.is_some() {
        // Unset, the text on your own bubbles is `on-accent` on the accent,
        // and `text` on a ground the theme sets.
        let own_ink = if layout.bubbles.own.is_some() {
            "text"
        } else {
            "on-accent"
        };
        pairs.push((
            if layout.bubbles.own_text.is_some() {
                "layout.bubbles.own_text"
            } else {
                own_ink
            },
            colours(layout.bubbles.own_text, own_ink),
            if layout.bubbles.own.is_some() {
                "layout.bubbles.own"
            } else {
                "accent"
            },
            paints(layout.bubbles.own.as_ref(), "accent"),
        ));
    }
    if bubbles || layout.bubbles.other.is_some() {
        pairs.push((
            "text",
            shown_colours(variant, "text"),
            if layout.bubbles.other.is_some() {
                "layout.bubbles.other"
            } else {
                "raised"
            },
            paints(layout.bubbles.other.as_ref(), "raised"),
        ));
    }
    // A drawn gel lays its gloss and glow where text can be: the band
    // reaches the text's first line, past the bubble's top padding, and the
    // glow its last, over the bottom padding. Each is laid at the most it
    // lays there, on every bubble, over each ground colour.
    let (_, pad) = layout.bubbles.padding.unwrap_or(BUBBLE_PADDING);
    let own_text_name = if layout.bubbles.own_text.is_some() {
        "layout.bubbles.own_text"
    } else if layout.bubbles.own.is_some() {
        "text"
    } else {
        "on-accent"
    };
    let own_ink = colours(
        layout.bubbles.own_text,
        if layout.bubbles.own.is_some() {
            "text"
        } else {
            "on-accent"
        },
    );
    let sides = [
        (
            &layout.bubbles.own_gel,
            ["layout.bubbles.own.gloss", "layout.bubbles.own.glow"],
            own_text_name,
            own_ink,
            paints(layout.bubbles.own.as_ref(), "accent"),
        ),
        (
            &layout.bubbles.other_gel,
            ["layout.bubbles.other.gloss", "layout.bubbles.other.glow"],
            "text",
            shown_colours(variant, "text"),
            paints(layout.bubbles.other.as_ref(), "raised"),
        ),
    ];
    for (gel, [gloss_name, glow_name], ink_name, ink, grounds) in sides {
        let Some(grounds) = grounds else { continue };
        let laid = |colour: u32, alpha: f32| {
            Some(
                grounds
                    .iter()
                    .map(|ground| over_at(colour, alpha, *ground))
                    .collect(),
            )
        };
        let gloss = gel
            .gloss
            .map(|gloss| (gloss_name, gloss.colour, gloss.alpha_below(pad)));
        let glow = gel
            .glow
            .map(|glow| (glow_name, glow.colour, glow.alpha_above(pad)));
        for (name, colour, alpha) in [gloss, glow].into_iter().flatten() {
            if alpha > 0.0 {
                // The alpha already holds the colour's own.
                pairs.push((
                    ink_name,
                    ink.clone(),
                    name,
                    laid(colour | 0xff00_0000, alpha),
                ));
            }
        }
    }
    if let Some(ground) = layout.cards.ground {
        pairs.push((
            "text",
            shown_colours(variant, "text"),
            "layout.cards.ground",
            Some(vec![ground]),
        ));
    }
    // A translucent ground lies over the surface under it.
    let laid = |grounds: Vec<u32>, under: &str| -> Option<Vec<u32>> {
        let unders = shown_colours(variant, under)?;
        Some(
            grounds
                .iter()
                .flat_map(|ground| unders.iter().map(move |under| laid_over(*ground, *under)))
                .collect(),
        )
    };
    // A card tab's words, on its own ground.
    if let Some(tab) = &layout.cards.tab
        && let Some(ground) = tab.ground
    {
        pairs.push((
            "layout.cards.tab.colour",
            colours(tab.colour, "text"),
            "layout.cards.tab.ground",
            Some(vec![ground]),
        ));
    }
    // Framed surfaces' words on their grounds, and a section's on its bar.
    for (surface, stock) in [
        (super::package::Surface::Label, "sidebar"),
        (super::package::Surface::Header, "chat"),
        (super::package::Surface::CardSection, "raised"),
    ] {
        let Some(shape) = variant.shapes.get(&surface) else {
            continue;
        };
        let text = colours(shape.text, "text");
        let ground = match shape.ground.as_ref() {
            Some(paint) => laid(paint.colours(), stock),
            None => shown_colours(variant, stock),
        };
        let (text_name, ground_name) = match surface {
            super::package::Surface::Label => ("shape.label.text", "shape.label.ground"),
            super::package::Surface::Header => ("shape.header.text", "shape.header.ground"),
            _ => ("shape.card_section.text", "shape.card_section.ground"),
        };
        pairs.push((text_name, text.clone(), ground_name, ground.clone()));
        if let Some(bar) = &shape.bar {
            pairs.push((
                "shape.card_section.bar_text",
                shape.bar_text.map(|colour| vec![colour]).or(text),
                "shape.card_section.bar",
                ground.clone().map(|grounds| {
                    bar.colours()
                        .iter()
                        .flat_map(|bar| grounds.iter().map(move |ground| laid_over(*bar, *ground)))
                        .collect()
                }),
            ));
        }
    }
    // Every text colour still reads on a striped row.
    if let Some(stripe) = variant
        .shapes
        .get(&super::package::Surface::Row)
        .and_then(|shape| shape.stripe)
    {
        pairs.push((
            "text",
            shown_colours(variant, "text"),
            "shape.row.stripe",
            laid(vec![stripe.colour], "chat"),
        ));
    }
    // A tree row's mention count, in the colour the theme names for it.
    if let Some(ink) = variant.chrome.counts.as_ref().and_then(|counts| counts.ink) {
        for ground in ["sidebar", "rail"] {
            pairs.push((
                "chrome.tree.counts.colour",
                shown_colours(variant, ink.name()),
                ground,
                shown_colours(variant, ground),
            ));
        }
    }
    let mut refused = Vec::new();
    for (text, fronts, ground, backs) in pairs {
        let (Some(fronts), Some(backs)) = (fronts, backs) else {
            continue;
        };
        let worst = fronts
            .iter()
            .flat_map(|front| backs.iter().map(move |back| contrast(*front, *back)))
            .fold(f64::INFINITY, f64::min);
        if worst < READABLE - 0.005 {
            refused.push(format!(
                "{text} on {ground} contrasts {worst:.1}:1, under {READABLE:.1}:1; text has to stay readable"
            ));
        }
    }
    refused
}

/// `colour`'s red, green and blue at `alpha` (times its own alpha) over an
/// opaque `back`.
fn over_at(colour: u32, alpha: f32, back: u32) -> u32 {
    let alpha = f64::from(alpha) * f64::from(colour >> 24) / 255.0;
    let mix = |shift: u32| {
        let front = f64::from((colour >> shift) & 0xff);
        let behind = f64::from((back >> shift) & 0xff);
        ((front * alpha + behind * (1.0 - alpha)).round() as u32).min(255) << shift
    };
    0xff00_0000 | mix(16) | mix(8) | mix(0)
}

/// `top` laid over the opaque `under`, by its alpha, as an opaque colour.
fn laid_over(top: u32, under: u32) -> u32 {
    let alpha = f64::from(top >> 24) / 255.0;
    let mix = |shift: u32| {
        let (a, b) = (
            f64::from((top >> shift) & 0xff),
            f64::from((under >> shift) & 0xff),
        );
        ((a * alpha + b * (1.0 - alpha)).round() as u32) << shift
    };
    0xff00_0000 | mix(16) | mix(8) | mix(0)
}

/// How much two colours contrast, from 1 to 21, as WCAG measures it; a
/// translucent foreground is laid over the background first.
pub fn contrast(foreground: u32, background: u32) -> f64 {
    let channel = |colour: u32, shift: u32| f64::from((colour >> shift) & 0xff) / 255.0;
    let alpha = f64::from(foreground >> 24) / 255.0;
    let back = [16, 8, 0].map(|shift| channel(background, shift));
    let front = [16, 8, 0].map(|shift| channel(foreground, shift));
    let mixed = [0, 1, 2].map(|index| front[index] * alpha + back[index] * (1.0 - alpha));
    let (one, two) = (relative_luminance(mixed), relative_luminance(back));
    (one.max(two) + 0.05) / (one.min(two) + 0.05)
}

fn relative_luminance(rgb: [f64; 3]) -> f64 {
    let linear = |value: f64| {
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// Checks one file by what its name says it is.
pub fn check_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "ttf" | "otf" => check_font(bytes),
        "png" | "jpg" | "jpeg" | "webp" => check_image(bytes),
        "svg" => check_svg(bytes),
        "wav" | "ogg" | "opus" | "mp3" => media::decode_sound(path, bytes).map(|_| ()),
        _ => Err("isn't a kind of file a theme holds: fonts are .ttf or .otf, pictures .png, .jpg, .webp or .svg, sounds .wav, .ogg, .opus or .mp3".into()),
    }
}

/// A font is parsed in full before anything draws with it: its tables, its
/// name, and a glyph for a letter. A parser that panics on a hostile file is
/// caught, and the file refused.
pub fn check_font(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err("a font is at most 4 MiB".into());
    }
    let parsed = std::panic::catch_unwind(|| {
        let face = ttf_parser::Face::parse(bytes, 0)
            .map_err(|error| format!("isn't a font Yapfast reads ({error})"))?;
        if face.number_of_glyphs() == 0 {
            return Err("holds no glyphs".to_owned());
        }
        let named = face
            .names()
            .into_iter()
            .any(|name| name.name_id == ttf_parser::name_id::FAMILY && name.to_string().is_some());
        if !named {
            return Err("has no family name".to_owned());
        }
        if face.glyph_index('a').is_none() && face.glyph_index('A').is_none() {
            return Err("has no Latin letters to draw text with".to_owned());
        }
        Ok(face.names().into_iter().count())
    });
    match parsed {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("isn't a font Yapfast reads".into()),
    }
}

/// A picture is PNG, JPEG or WebP, no larger than [`MAX_IMAGE_SIDE`] on a
/// side, and decodes whole.
pub fn check_image(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("a picture is at most 2 MiB".into());
    }
    let format =
        image::guess_format(bytes).map_err(|_| "isn't a picture Yapfast reads".to_owned())?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    ) {
        return Err("is a picture Yapfast doesn't take: use PNG, JPEG or WebP".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(64 << 20);
    reader.limits(limits);
    reader.decode().map(|_| ()).map_err(|error| match error {
        image::ImageError::Limits(_) => format!(
            "is too large to decode: at most {MAX_IMAGE_SIDE} pixels on a side, and 8 bits a channel for the largest pictures"
        ),
        _ => "doesn't decode".into(),
    })
}

/// Elements an SVG may not hold: anything that runs, embeds another document
/// or animates.
const SVG_REFUSED: [&str; 9] = [
    "script",
    "foreignObject",
    "iframe",
    "embed",
    "object",
    "animate",
    "set",
    "animateTransform",
    "animateMotion",
];

/// An SVG is read as XML and refused if it holds a script, an event handler,
/// another document, or a reference to anything outside itself; then it has
/// to render.
pub fn check_svg(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("a picture is at most 2 MiB".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "isn't UTF-8 text".to_owned())?;
    if text.contains("<!ENTITY") || text.contains("<!DOCTYPE") {
        return Err("declares entities or a doctype, which an SVG in a theme can't".into());
    }
    let options = resvg::usvg::roxmltree::ParsingOptions {
        allow_dtd: false,
        ..Default::default()
    };
    let document = resvg::usvg::roxmltree::Document::parse_with_options(text, options)
        .map_err(|error| format!("isn't an SVG Yapfast reads ({error})"))?;
    for node in document.descendants().filter(|node| node.is_element()) {
        let name = node.tag_name().name();
        if SVG_REFUSED.contains(&name) {
            return Err(format!("holds <{name}>, which an SVG in a theme can't"));
        }
        for attribute in node.attributes() {
            let key = attribute.name();
            let value = attribute.value().trim();
            if key.to_ascii_lowercase().starts_with("on") {
                return Err(format!(
                    "has an event handler ({key}), which an SVG in a theme can't"
                ));
            }
            if key == "href" && !value.starts_with('#') {
                return Err(format!(
                    "links to \"{}\": an SVG in a theme only refers to itself",
                    value.chars().take(40).collect::<String>()
                ));
            }
            if (key == "style" || name == "style") && outside_reference(value) {
                return Err("its style reaches outside the file".into());
            }
            if outside_reference(value) && value.contains("url(") {
                return Err(format!("{key} reaches outside the file"));
            }
        }
        if name == "style"
            && let Some(css) = node.text()
            && outside_reference(css)
        {
            return Err("its style reaches outside the file".into());
        }
    }
    resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .map(|_| ())
        .map_err(|error| format!("doesn't render ({error})"))
}

/// Whether CSS or an attribute refers to something that isn't in the file:
/// an import, or a `url()` that isn't a `#fragment`.
fn outside_reference(text: &str) -> bool {
    if text.contains("@import") {
        return true;
    }
    let mut rest = text;
    while let Some(start) = rest.find("url(") {
        let after = rest[start + 4..]
            .trim_start()
            .trim_start_matches(['"', '\'']);
        if !after.starts_with('#') {
            return true;
        }
        rest = &rest[start + 4..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(manifest: &str) -> Files {
        Files {
            manifest: manifest.to_owned(),
            ..Files::default()
        }
    }

    const PLUM: &str = r##"{ "theme_api": 1, "id": "com.example.plum", "name": "Plum", "version": "1.0.0",
        "variants": { "dark": { "base": "yap", "colours": { "chat": "#2a1a3a" } } } }"##;

    #[test]
    fn a_reason_quoting_the_theme_stays_one_line() {
        let spoof = PLUM.replace("com.example.plum", "x\\n\\n\u{202e}Verified by Yapfast");
        let reason = check(&files(&spoof), Strictness::Install)
            .reason()
            .expect("a bad id fails");
        assert!(
            !reason.contains('\n') && !reason.contains('\u{202e}'),
            "{reason}"
        );
        assert!(reason.contains("\\u{a}"), "{reason}");
        // Only escaped, never cut: what to do comes last in a long one.
        let long = PLUM.replace("com.example.plum", &"a".repeat(200));
        let reason = check(&files(&long), Strictness::Install).reason().unwrap();
        assert!(
            reason.ends_with('"') && reason.contains(&"a".repeat(200)),
            "{reason}"
        );
        let text = check(&files(&long), Strictness::Install).text();
        assert!(text.contains(&format!("{}\"", "a".repeat(200))), "{text}");
        // But never as long as a theme likes.
        let huge = PLUM.replace("com.example.plum", &"a".repeat(200_000));
        let reason = check(&files(&huge), Strictness::Install).reason().unwrap();
        assert_eq!(reason.chars().count(), 1001, "{}", &reason[..80]);
    }

    /// PLUM with `options` (and anything else) added at its top level.
    fn with_options(extra: &str) -> String {
        PLUM.replace(r#""variants""#, &format!(r#"{extra}, "variants""#))
    }

    #[test]
    fn styled_parts_pass_and_unknown_ones_fail_the_check() {
        let good = with_options(
            r##""parts": { "sidebar.channel-row": { "radius": 4,
                "hover": { "ground": "#3a2a4a", "ink": "#f2f3f5" }, "selected": { "ink": "#ffffff" } } }"##,
        );
        let report = check(&files(&good), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        for (parts, expected) in [
            (
                r##""parts": { "sidebar.channel_row": { "ink": "#ffffff" } }"##,
                "\"parts.sidebar.channel_row\" isn't a part Yapfast has (did you mean \"sidebar.channel-row\"?)",
            ),
            (
                r##""parts": { "menu": { "hover": { "ground": "#000000" } } }"##,
                "\"parts.menu.hover\": menu has no hover state",
            ),
            (
                r##""parts": { "menu.row": { "glow": 3 } }"##,
                "\"parts.menu.row.glow\" isn't a property or state Yapfast has",
            ),
        ] {
            let report = check(&files(&with_options(parts)), Strictness::Install);
            assert!(!report.passed(), "{parts}");
            assert!(
                report.errors.iter().any(|error| error.contains(expected)),
                "{expected} in {:?}",
                report.errors
            );
            // Said once, as an error, not again as a note.
            assert!(
                !report.notes.iter().any(|note| note.contains("parts.")),
                "{:?}",
                report.notes
            );
        }
        // In a variant too.
        let variant = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r##""colours": { "chat": "#2a1a3a" }, "parts": { "rail.servr": {} }"##,
        );
        let report = check(&files(&variant), Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("\"variants.dark.parts.rail.servr\"")),
            "{:?}",
            report.errors
        );
        // Unreadable ink on a part's ground is refused.
        let dim = with_options(r##""parts": { "menu.row": { "hover": { "ink": "#2a2a2a" } } }"##);
        let report = check(&files(&dim), Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.starts_with("parts.menu.row (hover): its ink contrasts")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn an_option_that_sets_an_unknown_part_is_refused() {
        let theme = with_options(
            r##""options": [{ "id": "loud", "label": "Loud", "kind": "toggle", "default": false,
                    "on": { "parts.menu.row.hover.ink": "#ffffff", "parts.menu.rowz.ink": "#ffffff" } }]"##,
        );
        let report = check(&files(&theme), Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("parts.menu.rowz")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn two_files_a_letter_case_apart_are_refused() {
        let manifest = with_options(r#""preview": ["art/Sky.png", "art/sky.png"]"#);
        let report = check(&files(&manifest), Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("letter case")),
            "{:?}",
            report.errors
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_inside_a_theme_isnt_read_through() {
        let directory = tempfile::tempdir().unwrap();
        let outside = directory.path().join("outside");
        let theme = directory.path().join("theme");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(theme.join("own")).unwrap();
        std::fs::write(outside.join("private.png"), b"secret").unwrap();
        std::fs::write(theme.join("own/p.png"), b"mine").unwrap();
        std::os::unix::fs::symlink(&outside, theme.join("art")).unwrap();
        std::fs::write(
            theme.join(MANIFEST),
            with_options(r#""preview": ["art/private.png", "own/p.png"]"#),
        )
        .unwrap();
        let read = Files::read(&theme).unwrap();
        assert!(!read.files.contains_key(Path::new("art/private.png")));
        assert_eq!(
            read.files.get(Path::new("own/p.png")).map(Vec::as_slice),
            Some(&b"mine"[..])
        );
    }

    #[test]
    fn options_that_keep_text_readable_pass_and_name_their_flips() {
        let manifest = with_options(
            r##""colours": { "accent": "#ffbf47" },
            "options": [
              { "id": "dim", "label": "Dim chat", "kind": "toggle", "default": false,
                "on": { "variants.dark.colours.chat": "#140c1c" } },
              { "id": "side", "label": "Rail side", "kind": "choice", "default": "left",
                "choices": [{ "value": "left", "label": "Left" },
                            { "value": "right", "label": "Right", "set": { "layout.rail": "right" } }] }
            ]"##,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        assert_eq!(report.package.unwrap().options.len(), 2);
    }

    #[test]
    fn an_option_that_makes_text_unreadable_is_refused_naming_it() {
        let manifest = with_options(
            r##""options": [{ "id": "pale", "label": "Pale", "kind": "toggle", "default": false,
                "on": { "variants.dark.colours.chat": "#d8d8d8" } }]"##,
        );
        let report = check(&files(&manifest), Strictness::Install);
        let reason = report.reason().unwrap();
        assert!(
            reason.starts_with("with \"Pale\" on (dark): text on chat"),
            "{reason}"
        );
    }

    #[test]
    fn an_option_naming_a_field_yapfast_doesnt_read_is_refused() {
        let unknown = with_options(
            r##""options": [{ "id": "x", "label": "X", "kind": "toggle",
                "off": { "layout.wobble": true } }]"##,
        );
        let report = check(&files(&unknown), Strictness::Install);
        assert!(
            report.reason().unwrap().contains("layout.wobble"),
            "{:?}",
            report.errors
        );
        let missing = with_options(
            r##""options": [{ "id": "grain", "label": "Grain", "kind": "toggle",
                "off": { "art.overlays.grain": null } }]"##,
        );
        let report = check(&files(&missing), Strictness::Install);
        assert!(
            report.reason().unwrap().contains("art.overlays.grain"),
            "{:?}",
            report.errors
        );
        let out_of_range = with_options(
            r##""options": [{ "id": "pad", "label": "Padding", "kind": "slider", "min": 0,
                "max": 200, "default": 16, "paths": ["spacing.chat_padding"] }]"##,
        );
        let report = check(&files(&out_of_range), Strictness::Install);
        assert!(!report.passed());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("\"Padding\"")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn a_theme_with_ransom_names_offers_them_as_an_option() {
        let manifest = with_options(r##""type": { "names": { "style": "ransom" } }"##);
        let package = Package::parse(&manifest).unwrap();
        let option = &package.options[0];
        assert_eq!(option.id, super::super::options::RANSOM);
        let mut choices = super::super::options::Choices::new();
        assert!(
            package
                .resolve_with(Mode::Dark, &choices)
                .unwrap()
                .layout
                .names
                .is_some()
        );
        choices.insert(option.id.clone(), serde_json::Value::Bool(false));
        assert!(
            package
                .resolve_with(Mode::Dark, &choices)
                .unwrap()
                .layout
                .names
                .is_none()
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
    }

    #[test]
    fn a_readable_theme_passes() {
        let report = check(&files(PLUM), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("preview"))
        );
    }

    #[test]
    fn display_type_for_reading_text_points_at_readable_text() {
        let plain = check(&files(PLUM), Strictness::Install);
        assert!(
            !plain
                .notes
                .iter()
                .any(|note| note.contains("Readable text")),
            "{:?}",
            plain.notes
        );
        let mut theme = files(&PLUM.replace(
            "\"variants\"",
            r#""type": { "names": { "font": "fonts/HurtFraktur-Bold.ttf", "style": "ransom" },
                         "labels": { "font": "fonts/Silkscreen-Regular.ttf" },
                         "headings": { "font": "fonts/PixelifySans.ttf" } },
               "variants""#,
        ));
        for font in [
            "fonts/HurtFraktur-Bold.ttf",
            "fonts/Silkscreen-Regular.ttf",
            "fonts/PixelifySans.ttf",
        ] {
            theme.files.insert(PathBuf::from(font), Vec::new());
        }
        let report = check(&theme, Strictness::Install);
        let note = report
            .notes
            .iter()
            .find(|note| note.contains("Readable text"))
            .expect("a note about display type");
        assert!(
            note.contains("names in blackletter (fonts/HurtFraktur-Bold.ttf)"),
            "{note}"
        );
        assert!(
            note.contains("labels in pixel (fonts/Silkscreen-Regular.ttf)"),
            "{note}"
        );
        assert!(note.contains("ransom"), "{note}");
        assert!(
            !note.contains("Pixelify"),
            "headings may keep a display face: {note}"
        );
        assert_eq!(display_face(Path::new("fonts/Geist.ttf"), &[]), None);
        assert_eq!(
            display_face(Path::new("PressStart2P.ttf"), &[]),
            Some("pixel")
        );
    }

    #[test]
    fn unreadable_text_is_refused_with_the_token() {
        let report = check(
            &files(&PLUM.replace("#2a1a3a", "#d8d8d8")),
            Strictness::Install,
        );
        assert!(!report.passed());
        let reason = report.reason().unwrap();
        assert!(reason.starts_with("text on chat"), "{reason}");
        assert!(reason.contains("4.5:1"), "{reason}");
    }

    #[test]
    fn a_content_name_colour_must_read_and_content_shows_in_the_cost() {
        let content = |colour: &str| {
            PLUM.replace(
                r#""variants""#,
                &format!(
                    r#""content": {{ "media": "mono", "names": {{ "colours": "theme", "colour": "{colour}" }} }}, "variants""#
                ),
            )
        };
        let report = check(&files(&content("#3a2a4a")), Strictness::Install);
        assert!(!report.passed());
        assert!(
            report
                .reason()
                .unwrap()
                .starts_with("content.names.colour on chat"),
            "{:?}",
            report.errors
        );
        let report = check(&files(&content("#f0e0ff")), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        assert!(
            report
                .cost
                .text()
                .contains("content (if allowed): mono pictures, theme name colours"),
            "{}",
            report.cost.text()
        );
    }

    #[test]
    fn every_stop_of_a_gradient_must_read() {
        let report = check(
            &files(&PLUM.replace("\"#2a1a3a\"", "\"linear(#1a1a1a, #f0f0f0)\"")),
            Strictness::Install,
        );
        assert!(!report.passed());
    }

    #[test]
    fn a_dark_variant_needs_a_dark_base() {
        let report = check(
            &files(&PLUM.replace("\"yap\"", "\"sakura\"")),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("is a light theme")),
            "{:?}",
            report.errors
        );
        let report = check(
            &files(&PLUM.replace("\"yap\"", "\"neon\"")),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("isn't a theme Yapfast comes with"))
        );
    }

    #[test]
    fn a_named_file_has_to_be_there() {
        let manifest = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""type": { "body": { "font": "fonts/Body.ttf" } }"#,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("isn't in the theme"))
        );
        let mut with = files(&manifest);
        with.files
            .insert("fonts/Body.ttf".into(), b"not a font".to_vec());
        let report = check(&with, Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("isn't a font")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn the_marketplace_asks_for_more() {
        let report = check(&files(PLUM), Strictness::Marketplace);
        let errors = report.errors.join("\n");
        for expected in ["licence", "GitHub", "description", "preview"] {
            assert!(errors.contains(expected), "{expected} in {errors}");
        }
    }

    #[test]
    fn only_github_repositories() {
        assert!(github("https://github.com/ada/vaporwave"));
        assert!(github("https://github.com/ada/vaporwave.git"));
        assert!(github("https://github.com/ada/vaporwave/"));
        assert!(!github("https://gitlab.com/ada/vaporwave"));
        assert!(!github("https://github.com.evil.example/ada/vaporwave"));
        assert!(!github("http://github.com/ada/vaporwave"));
        assert!(!github("https://github.com/ada"));
        assert!(!github("https://github.com/../x"));
    }

    #[test]
    fn svgs_that_could_run_or_reach_out_are_refused() {
        let fine = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><linearGradient id="g"><stop offset="0" stop-color="red"/></linearGradient></defs><rect width="10" height="10" fill="url(#g)"/></svg>"##;
        assert_eq!(check_svg(fine), Ok(()));
        for (svg, says) in [
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
                "<script>",
            ),
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg" onload="x()"><rect width="1" height="1"/></svg>"#,
                "event handler",
            ),
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="https://example.com/a.png"/></svg>"#,
                "links to",
            ),
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg"><rect fill="url(https://example.com/x#g)" width="1" height="1"/></svg>"#,
                "outside",
            ),
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>@import url(x.css);</style></svg>"#,
                "outside",
            ),
            (
                r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg xmlns="http://www.w3.org/2000/svg"/>"#,
                "entities",
            ),
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject/></svg>"#,
                "foreignObject",
            ),
        ] {
            let error = check_svg(svg.as_bytes()).unwrap_err();
            assert!(error.contains(says), "{svg}: {error}");
        }
    }

    #[test]
    fn text_over_art_has_to_read() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([240, 240, 240, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let theme = |opacity: f32| {
            let manifest = PLUM.replace(
                r##""colours": { "chat": "#2a1a3a" }"##,
                &format!(
                    r##""art": {{ "surfaces": {{ "chat": {{ "image": "art/white.png", "opacity": {opacity} }} }} }}"##
                ),
            );
            let mut files = files(&manifest);
            files
                .files
                .insert(PathBuf::from("art/white.png"), png.clone());
            check(&files, Strictness::Install)
        };
        // A faint wash over the chat reads; a white picture under white text doesn't.
        assert!(theme(0.05).passed(), "{:?}", theme(0.05).errors);
        let refused = theme(0.9);
        assert!(
            refused
                .errors
                .iter()
                .any(|error| error.contains("text on the chat art")),
            "{:?}",
            refused.errors
        );
        assert!(
            theme(0.05).cost.text().contains("1 art layer"),
            "{}",
            theme(0.05).cost.text()
        );
    }

    #[test]
    fn a_named_picture_has_to_be_there() {
        let manifest = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r##""art": { "surfaces": { "window": { "image": "art/sky.jpg" } } }"##,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("art/sky.jpg"))
        );
    }

    #[test]
    fn pictures_are_checked_by_decoding() {
        let mut png = Vec::new();
        image::RgbaImage::new(4, 4)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(check_image(&png), Ok(()));
        assert!(check_image(b"GIF89a....").is_err());
        assert!(check_image(&png[..20]).is_err());
    }

    #[test]
    fn gradient_bubbles_and_a_see_through_member_list_are_checked() {
        let with = |layers: &str| PLUM.replace(r##""colours": { "chat": "#2a1a3a" }"##, layers);
        // Each stop of a gradient bubble holds the text on it.
        let report = check(
            &files(&with(
                r##""layout": { "messages": "bubbles", "bubbles": { "own": "linear(#1a1a40, #ffffff)", "own_text": "#ffffff" } }"##,
            )),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("layout.bubbles.own_text on layout.bubbles.own")),
            "{:?}",
            report.errors
        );
        let report = check(
            &files(&with(
                r##""layout": { "messages": "bubbles", "bubbles": { "own": { "ground": "linear(#101030, #202050)", "art": { "texture": "grain" } }, "own_text": "#ffffff" } }"##,
            )),
            Strictness::Install,
        );
        assert!(report.passed(), "{:?}", report.errors);
        assert!(
            report
                .cost
                .looks
                .iter()
                .any(|look| look.contains("1 picture on bubbles")),
            "{:?}",
            report.cost.looks
        );
        // A clear member list shows the conversation under it: light text
        // on a clear white ground reads over the dark chat.
        let report = check(
            &files(&with(
                r##""colours": { "chat": "#2a1a3a", "members": "#ffffff00" }"##,
            )),
            Strictness::Install,
        );
        assert!(report.passed(), "{:?}", report.errors);
        // But not over a light one.
        let report = check(
            &files(&with(
                r##""colours": { "chat": "#f0f0f0", "members": "#ffffff00" }"##,
            )),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("on the members art")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn bubbles_and_cards_have_to_read() {
        let with = |layout: &str| {
            PLUM.replace(
                r##""colours": { "chat": "#2a1a3a" }"##,
                &format!(r#""layout": {layout}"#),
            )
        };
        // Yap's on-accent on its accent, and text on raised, read.
        let report = check(
            &files(&with(r#"{ "messages": "bubbles" }"#)),
            Strictness::Install,
        );
        assert!(report.passed(), "{:?}", report.errors);
        let report = check(
            &files(&with(
                r##"{ "messages": "bubbles", "bubbles": { "own": "#ffffff", "own_text": "#eeeeee" } }"##,
            )),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("layout.bubbles.own_text on layout.bubbles.own")),
            "{:?}",
            report.errors
        );
        let report = check(
            &files(&with(r##"{ "bubbles": { "other": "#e0e0e0" } }"##)),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("text on layout.bubbles.other")),
            "{:?}",
            report.errors
        );
        let report = check(
            &files(&with(
                r##"{ "messages": "cards", "cards": { "ground": "#dddddd" } }"##,
            )),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("layout.cards.ground")),
            "{:?}",
            report.errors
        );
        // The hover bar's ground carries its "Delete for everyone?".
        let report = check(
            &files(&PLUM.replace(
                r##""colours": { "chat": "#2a1a3a" }"##,
                r##""colours": { "actions": "#e8e8e8" }"##,
            )),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("text on actions")),
            "{:?}",
            report.errors
        );
        // Outline colours are decoration.
        let report = check(
            &files(&PLUM.replace(
                r##""colours": { "chat": "#2a1a3a" }"##,
                r##""shape": { "borders": { "card": { "style": "dashed", "colour": "#1e1f24" } } }"##,
            )),
            Strictness::Install,
        );
        assert!(report.passed(), "{:?}", report.errors);
    }

    #[test]
    fn a_gel_keeps_its_gloss_off_the_text() {
        let with = |layout: &str| {
            PLUM.replace(
                r##""colours": { "chat": "#2a1a3a" }"##,
                &format!(r#""layout": {layout}"#),
            )
        };
        let gel = |gloss: &str, padding: &str| {
            with(&format!(
                r##"{{ "messages": "bubbles", "bubbles": {{ {padding}
                    "own": {{ "ground": "linear(#0c4cc0, #0a3f9e)", "gloss": {gloss} }},
                    "own_text": "#ffffff" }} }}"##
            ))
        };
        let refused = |report: &Report| {
            report
                .errors
                .iter()
                .any(|error| error.contains("layout.bubbles.own_text on layout.bubbles.own.gloss"))
        };
        // White text on deep blue reads; a bright gloss over its first
        // line doesn't.
        let report = check(
            &files(&gel(r#"{ "from": 0.7, "to": 0.3, "max": 30 }"#, "")),
            Strictness::Install,
        );
        assert!(refused(&report), "{:?}", report.errors);
        // The same gloss, ending above the text, reads.
        let report = check(
            &files(&gel(
                r#"{ "from": 0.7, "to": 0.3, "max": 8, "inset": 1 }"#,
                r#""padding": [12, 10],"#,
            )),
            Strictness::Install,
        );
        assert!(report.passed(), "{:?}", report.errors);
        // So does a faint one that fades as it reaches the text.
        let report = check(
            &files(&gel(
                r#"{ "from": 0.35, "to": 0.0, "max": 20 }"#,
                r#""padding": [12, 10],"#,
            )),
            Strictness::Install,
        );
        assert!(report.passed(), "{:?}", report.errors);
        // A glow is held to the last line the same way.
        let report = check(
            &files(&with(
                r##"{ "messages": "bubbles", "bubbles": { "own": { "ground": "#0c4cc0", "glow": { "colour": "#ffffff", "height": 30 } }, "own_text": "#ffffff" } }"##,
            )),
            Strictness::Install,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("layout.bubbles.own.glow")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn ransom_fonts_are_named_files_and_parsed() {
        let manifest = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""type": { "names": { "style": "ransom", "fonts": ["a.ttf", "b.otf"] } }"#,
        );
        let mut theme = files(&manifest);
        let report = check(&theme, Strictness::Install);
        assert!(
            report.errors.iter().any(|error| error.contains("a.ttf")),
            "{:?}",
            report.errors
        );
        theme.files.insert("a.ttf".into(), b"not a font".to_vec());
        theme.files.insert("b.otf".into(), b"not a font".to_vec());
        let report = check(&theme, Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.starts_with("a.ttf: ")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn the_cost_names_the_chrome() {
        let manifest = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""chrome": { "rail": { "mode": "text" }, "hide": ["member-avatars"], "composer": { "prompt": "$" } }"#,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        let text = report.cost.text();
        assert!(
            text.contains("chrome: no member-avatars, text rail, prompt"),
            "{text}"
        );
        let broken = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""chrome": { "labels": { "new": "{server}" } }"#,
        );
        assert!(!check(&files(&broken), Strictness::Install).passed());
    }

    #[test]
    fn the_cost_names_the_layout() {
        let manifest = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""layout": { "messages": "irc", "rail": "top" }, "type": { "names": { "style": "ransom" } }"#,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        let text = report.cost.text();
        assert!(
            text.contains("layout: irc, rail top, ransom names"),
            "{text}"
        );
        assert!(
            !check(&files(PLUM), Strictness::Install)
                .cost
                .text()
                .contains("layout")
        );
    }

    #[test]
    fn the_cost_counts_fonts_and_animations() {
        let manifest = PLUM.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""animations": [
                { "slot": "window", "property": "opacity", "values": [0, 0.04], "period": 100, "steps": 2 },
                { "slot": "unread", "property": "scale", "values": [1, 1.1], "period": 1000 }
            ]"#,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert_eq!(report.cost.animations, 2);
        assert_eq!(report.cost.heavy, 1);
        assert!(
            report.cost.text().contains("2 animations (1 heavy)"),
            "{}",
            report.cost.text()
        );
    }

    /// A mono 16-bit 48 kHz WAV of `seconds` of a tone.
    fn tone(seconds: f32) -> Vec<u8> {
        let data: Vec<u8> = (0..(seconds * 48_000.0) as usize)
            .flat_map(|index| {
                let value = (std::f32::consts::TAU * 440.0 * index as f32 / 48_000.0).sin();
                ((value * 9_000.0) as i16).to_le_bytes()
            })
            .collect();
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        file.extend_from_slice(&48_000u32.to_le_bytes());
        file.extend_from_slice(&96_000u32.to_le_bytes());
        file.extend_from_slice(b"\x02\0\x10\0data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        file
    }

    #[test]
    fn a_long_description_fails_the_check_naming_its_length() {
        let long = "x".repeat(301);
        let manifest = format!(
            r#"{{ "theme_api": 1, "id": "com.example.long", "name": "Long", "version": "1.0.0",
                 "description": "{long}", "variants": {{ "dark": {{ "base": "yap" }} }} }}"#
        );
        let files = Files {
            manifest,
            ..Files::default()
        };
        let report = check(&files, Strictness::Install);
        let reason = report
            .reason()
            .expect("a description over 300 characters fails");
        assert!(reason.contains("301 characters"), "{reason}");
        assert!(reason.contains("at most 300"), "{reason}");
    }

    #[test]
    fn sounds_are_decoded_and_held_to_their_slots_length() {
        let manifest = r##"{ "theme_api": 1, "id": "com.example.pops", "name": "Pops", "version": "1.0.0",
            "sounds": { "hover": "sounds/short.wav", "join": "sounds/long.wav" },
            "icons": { "hash": "icons/hash.svg" },
            "variants": { "dark": { "base": "yap" } } }"##;
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 4h16v16H4z"/></svg>"#;
        let mut theme = files(manifest);
        theme.files.insert("sounds/short.wav".into(), tone(0.1));
        theme.files.insert("sounds/long.wav".into(), tone(2.0));
        theme.files.insert("icons/hash.svg".into(), svg.to_vec());
        let report = check(&theme, Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        assert_eq!((report.cost.sounds, report.cost.icons), (2, 1));
        // Decoded, 48 kHz stereo 16-bit: 2.1 s is about 0.4 MB.
        assert!(
            (380_000..420_000).contains(&report.cost.memory),
            "{}",
            report.cost.memory
        );
        assert!(
            report.cost.text().contains("2 sounds · 1 icon"),
            "{}",
            report.cost.text()
        );

        // A blip slot takes no long sound, and a broken file is refused.
        let mut theme = files(&manifest.replace(
            "\"hover\": \"sounds/short.wav\"",
            "\"hover\": \"sounds/long.wav\"",
        ));
        theme.files.insert("sounds/long.wav".into(), tone(2.0));
        theme.files.insert("icons/hash.svg".into(), svg.to_vec());
        let report = check(&theme, Strictness::Install);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("a hover sound lasts at most 1.5 s")),
            "{:?}",
            report.errors
        );
        theme
            .files
            .insert("sounds/long.wav".into(), b"RIFF nonsense".to_vec());
        let report = check(&theme, Strictness::Install);
        assert!(
            report.errors.iter().any(|error| error.contains("WAV")),
            "{:?}",
            report.errors
        );
    }

    /// Plum with Hurt's always-on motion, which Yapfast measured (see
    /// `budget`), and `extra` fields beside it.
    fn hurt_motion(grain: u32, extra: &str) -> String {
        PLUM.replace(
            r#""variants""#,
            &format!(
                r##""art": {{
                    "overlays": [
                        {{ "kind": "flare", "on": "chat", "at": [0.72, 0.78], "size": 900, "colour": "#9E263B", "opacity": 0.1, "period": 9000 }},
                        {{ "kind": "vignette", "colour": "#000000", "opacity": 0.38 }},
                        {{ "kind": "grain", "period": {grain}, "opacity": 0.085 }}
                    ],
                    "particles": [ {{ "preset": "dust", "on": "ambient", "colours": ["#D8D5D622"], "count": 14, "rate": 1, "size": 2, "life": 9000 }} ]
                }},
                "animations": [
                    {{ "slot": "window", "property": "tint", "values": ["#9E263B00", "#9E263B0C", "#9E263B00"], "period": 9000, "easing": "ease-in-out", "heavy": true }},
                    {{ "slot": "home", "property": "rotation", "values": [0, 360], "period": 7200, "easing": "linear" }},
                    {{ "slot": "unread", "property": "opacity", "values": [1, 0.55, 1], "period": 2400 }},
                    {{ "slot": "mention", "property": "scale", "values": [1, 1.1, 1, 1, 1], "period": 2400 }},
                    {{ "slot": "typing", "property": "opacity", "values": [1, 0.25], "period": 800, "steps": 2 }},
                    {{ "slot": "channel.name", "property": "x", "values": [0, 0, 0, 3, -2, 0], "period": 5120, "steps": 6, "repeat": "once" }}
                ]{extra}, "variants""##
            ),
        )
    }

    #[test]
    fn the_idle_estimate_matches_what_was_measured() {
        let package = Package::parse(&hurt_motion(180, "")).unwrap();
        let variant = package.resolve(Mode::Dark).unwrap();
        let pieces = budget::pieces(&variant);
        let cost = |what: &str| {
            pieces
                .iter()
                .find(|piece| piece.what.contains(what))
                .map(|piece| piece.cores)
                .unwrap_or_else(|| panic!("no {what} in {pieces:?}"))
        };
        let near = |what: &str, measured: f32| {
            let estimate = cost(what);
            assert!(
                (estimate - measured).abs() <= 0.06,
                "{what}: {estimate} against {measured} measured"
            );
        };
        // Grain redraws the window when it jumps, about 5 times a second at 180 ms.
        near("grain", 0.09);
        let fast = Package::parse(&hurt_motion(100, "")).unwrap();
        let fast = budget::pieces(&fast.resolve(Mode::Dark).unwrap());
        let grain = fast
            .iter()
            .find(|piece| piece.what.contains("grain"))
            .unwrap();
        assert!((grain.cores - 0.16).abs() <= 0.02, "{grain:?}");
        near("flare", 0.86);
        near("dust", 0.41);
        near("window tint", 0.28);
        near("home rotation", 0.24);
        // Alone, a pulse costs what a frame does: a spinning record alone
        // measured 0.20.
        near("unread", 0.2);
        // A blinking caret changes about once a second: next to nothing.
        assert!(cost("typing") < 0.01);
        // One pass and it rests: not always on.
        assert!(
            !pieces
                .iter()
                .any(|piece| piece.what.contains("channel.name"))
        );
        assert_eq!(pieces[0].what, "a flare 900 px across");
    }

    #[test]
    fn expensive_idle_motion_warns_with_a_fix_and_shows_in_the_cost() {
        let report = check(&files(&hurt_motion(100, "")), Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        let grain = report
            .warnings
            .iter()
            .find(|warning| warning.contains("grain"))
            .expect("a warning about the grain");
        assert!(
            grain.contains("grain period 100ms redraws the whole window 10 times a second: try ≥600ms, or a still grain"),
            "{grain}"
        );
        // Slower grain is under the budget.
        let slow = check(&files(&hurt_motion(600, "")), Strictness::Install);
        assert!(
            !slow
                .warnings
                .iter()
                .any(|warning| warning.contains("grain"))
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("flare"))
        );
        assert!(report.cost.idle > 2.0, "{}", report.cost.idle);
        // Balanced holds the heavy tint and halves the rest.
        assert!(report.cost.idle_balanced < report.cost.idle / 2.0);
        let text = report.cost.text();
        assert!(text.contains("cores idle ("), "{text}");
        assert!(text.contains("more than the window can draw"), "{text}");
    }

    #[test]
    fn still_motion_costs_nothing_idle() {
        let report = check(&files(PLUM), Strictness::Install);
        assert_eq!(report.cost.idle, 0.0);
        assert!(!report.cost.text().contains("idle"));
        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.contains("idle cost"))
        );
        // Grain that doesn't change, and the vignette, cost nothing either.
        let package = Package::parse(&hurt_motion(0, "")).unwrap();
        let pieces = budget::pieces(&package.resolve(Mode::Dark).unwrap());
        assert!(!pieces.iter().any(|piece| piece.what.contains("grain")));
        assert!(!pieces.iter().any(|piece| piece.what.contains("vignette")));
    }

    #[test]
    fn small_motion_under_the_budget_doesnt_warn() {
        let manifest = PLUM.replace(
            r#""variants""#,
            r#""animations": [ { "slot": "typing", "property": "opacity", "values": [1, 0.25], "period": 800, "steps": 2 } ], "variants""#,
        );
        let report = check(&files(&manifest), Strictness::Install);
        assert!(report.cost.idle > 0.0 && report.cost.idle < budget::WARN_CORES);
        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.contains("idle cost"))
        );
    }

    #[test]
    fn many_small_pieces_warn_together() {
        let slots = ["home", "unread", "mention", "own.avatar"];
        let animations: Vec<String> = slots
            .iter()
            .map(|slot| {
                format!(
                    r#"{{ "slot": "{slot}", "property": "opacity", "values": [1, 0.5, 1], "period": 100, "steps": 60 }}"#
                )
            })
            .collect();
        let manifest = PLUM.replace(
            r#""variants""#,
            &format!(r#""animations": [{}], "variants""#, animations.join(", ")),
        );
        let report = check(&files(&manifest), Strictness::Install);
        let together: Vec<&String> = report
            .warnings
            .iter()
            .filter(|warning| warning.contains("idle cost"))
            .collect();
        assert_eq!(together.len(), 1, "{together:?}");
        assert!(together[0].contains("together"), "{}", together[0]);
    }

    #[test]
    fn a_stepped_window_flicker_is_priced_by_its_steps() {
        let flicker = |period: u32| {
            PLUM.replace(
                r#""variants""#,
                &format!(
                    r##""animations": [ {{ "slot": "window", "property": "tint", "values": ["#00ff4100", "#00ff410a"], "period": {period}, "steps": 2 }} ], "variants""##
                ),
            )
        };
        let idle = |period| {
            check(&files(&flicker(period)), Strictness::Install)
                .cost
                .idle
        };
        assert!(idle(120) > idle(2000));
        assert!(idle(120) <= 0.95 + f32::EPSILON);
    }

    #[test]
    fn an_older_themes_cursors_are_a_note_not_an_error() {
        let manifest = r##"{ "theme_api": 1, "id": "com.example.paws", "name": "Paws", "version": "1.0.0",
            "cursors": {
                "default": { "image": "cursors/arrow.png", "hotspot": [1, 1] }
            },
            "options": [{ "id": "paw", "label": "Paw", "kind": "choice", "default": "plain",
                "choices": [{ "value": "plain", "label": "Plain" },
                            { "value": "paw", "label": "Paw", "set": "paw" }] }],
            "sets": { "paw": { "cursors.default.image": "cursors/paw.png", "colours.accent": "#7aa2ff" } },
            "variants": { "dark": { "base": "yap",
                "cursors": { "pointer": { "image": "cursors/paw.png", "hotspot": [10, 2] } } } } }"##;
        let mut theme = files(manifest);
        theme.files.insert("cursors/arrow.png".into(), vec![0; 16]);
        let report = check(&theme, Strictness::Install);
        assert!(report.passed(), "{:?}", report.errors);
        let notes = report.notes.join("\n");
        assert!(
            notes.contains("\"cursors\": Yapfast doesn't use custom cursors"),
            "{notes}"
        );
        assert!(notes.contains("\"variants.dark.cursors\""), "{notes}");
        assert!(
            notes.contains("\"sets.paw\": its cursor changes are ignored"),
            "{notes}"
        );
        assert!(
            !report.cost.text().contains("cursor"),
            "{}",
            report.cost.text()
        );
        let package = report.package.unwrap();
        assert!(
            !package
                .files()
                .iter()
                .any(|path| path.starts_with("cursors")),
            "{:?}",
            package.files()
        );
    }

    #[test]
    fn unfocused_motion_is_read() {
        use crate::themes::package::Unfocused;
        let with = |value: &str| {
            Package::parse(&PLUM.replace(
                r#""variants""#,
                &format!(r#""motion": {{ "unfocused": {value} }}, "variants""#),
            ))
        };
        let shown = with(r#""still""#)
            .unwrap()
            .resolve(Mode::Dark)
            .unwrap()
            .motion
            .unfocused;
        assert_eq!(shown, Some(Unfocused::Still));
        assert_eq!(
            Package::parse(PLUM)
                .unwrap()
                .resolve(Mode::Dark)
                .unwrap()
                .motion
                .unfocused
                .unwrap_or_default(),
            Unfocused::Slow
        );
        assert!(
            with(r#""sometimes""#)
                .and_then(|package| package.resolve(Mode::Dark))
                .is_err()
        );
    }
}

#[cfg(test)]
mod lists_summary {
    use super::*;
    use crate::themes::package::{Mode, Package};

    #[test]
    fn the_summary_names_the_lists() {
        let package = Package::parse(
            r##"{ "theme_api": 1, "id": "dev.example.lists", "name": "Lists", "version": "1.0.0",
                "layout": { "rail_tiles": { "shape": "card", "label": "inside", "selected": { "offset": [6, 0] } },
                            "channel_rows": { "height": 40, "meta": { "voice": "voice · {n}" } },
                            "member_rows": { "avatar": { "size": 0 }, "meta": { "online": "online" } } },
                "variants": { "dark": { "base": "yap" } } }"##,
        )
        .unwrap();
        let lines = looks(&package.resolve(Mode::Dark).unwrap());
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("rail tiles: card 44×52, label inside")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "channel rows: 40 px, custom voice counts"),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "member rows: no avatar, meta words"),
            "{lines:?}"
        );
    }
}
