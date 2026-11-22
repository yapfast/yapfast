//! The fonts the window draws with, in a font system of the client's own.
//!
//! iced's default font system loads every font on the system (9,474 faces
//! on the machine it was measured on) and copies the fonts given through its
//! API. [`install`] replaces it with one that holds DM Sans and Twemoji from
//! the binary's own bytes and the system's monospace face for code. The
//! display name fonts join it the first time a name needs one
//! ([`name_font`]), a script DM Sans lacks loads the system's font for it
//! the first time the shaper meets it, and a character DM Sans, Twemoji and
//! its script's font lack (𝔟, ⓒ, ⠃: often the Common script's, which no
//! script asks for) loads the system's font for that character the first
//! time the shaper can't draw it ([`load_wanted`]). fontdb maps system font
//! files rather than reading them, so nothing is copied.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use iced::{
    Font,
    advanced::graphics::text::{cosmic_text, font_system},
    font::{Family, Stretch, Style, Weight},
    futures::{Stream, StreamExt, channel::mpsc},
};

pub const REGULAR: Font = Font::with_name("DM Sans");
pub const MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..REGULAR
};
pub const SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..REGULAR
};
pub const BOLD: Font = Font {
    weight: Weight::Bold,
    ..REGULAR
};
pub const ITALIC: Font = Font {
    style: Style::Italic,
    ..REGULAR
};

/// The family name of the Twemoji font `crates/yapfast-iced/build.rs` makes.
pub const TWEMOJI: &str = "Yapfast Twemoji";

const DM_SANS: &[u8] = include_bytes!("../fonts/DMSans[opsz,wght].ttf");
/// Its italic, for Markdown's emphasis.
const DM_SANS_ITALIC: &[u8] = include_bytes!("../fonts/DMSans-Italic[opsz,wght].ttf");

/// Makes `family` (a theme's body face, loaded) the face of all text that
/// asks for the app's own DM Sans, as Slint's `default-font-family`
/// follows a theme's body face; `None` gives DM Sans back. Text already
/// shown is shaped again in the new face.
pub fn set_default_family(family: Option<&'static str>) {
    static ALIAS: Mutex<Option<&'static str>> = Mutex::new(None);
    let family = family.filter(|family| *family != "DM Sans");
    // Held while the alias is set, so the two never fall out of step.
    let mut alias = ALIAS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *alias == family {
        return;
    }
    *alias = family;
    cosmic_text::set_family_alias(family.map(|family| ("DM Sans", family)));
    let mut fonts = font_system().write().expect("font system");
    // The fallback's matches are cached by what text asks for, which
    // doesn't change.
    let _ = fonts.raw().db_mut();
    // Text shaped already is shaped again only when the version moves, as
    // in `load_wanted`.
    fonts.load_font(Cow::Owned(vec![0]));
}

/// Makes the window's font system: DM Sans and its italic, `twemoji` (the
/// font `crates/yapfast-iced/build.rs` packs, or nothing), and the system's
/// monospace face for code, with DM Sans then Twemoji as the fallback list,
/// so a display name font's missing space comes from DM Sans and ❤ from
/// Twemoji. What those two lack takes the system's font for the character
/// ([`load_wanted`]).
pub fn install(twemoji: &'static [u8]) {
    // `install` replaces the entire font database. Forget the theme-font
    // registry with it so a later load cannot mistake a face from the old
    // database for one still available to the shaper.
    *theme_fonts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = ThemeFonts::default();
    let mut db = cosmic_text::fontdb::Database::new();
    db.load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(DM_SANS)));
    db.load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(
        DM_SANS_ITALIC,
    )));
    if !twemoji.is_empty() {
        db.load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(twemoji)));
    }
    let found = monospace_file();
    if let Some(path) = &found
        && let Err(error) = db.load_font_file(path)
    {
        tracing::warn!(%error, path, "could not load the monospace font");
    }
    let mono = db
        .faces()
        .find(|face| face.monospaced)
        .map(|face| face.families[0].0.clone());
    tracing::info!(?found, ?mono, faces = db.len(), "fonts");
    if let Some(mono) = mono {
        db.set_monospace_family(mono);
    }
    db.set_sans_serif_family("DM Sans");
    let fallback = Fallback(&["DM Sans", TWEMOJI]);
    let mut system = font_system().write().expect("font system");
    *system.raw() =
        cosmic_text::FontSystem::new_with_locale_and_db_and_fallback("en-US".into(), db, fallback);
}

/// The system's monospace face, as fontconfig ranks them.
#[cfg(not(windows))]
fn monospace_file() -> Option<String> {
    std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "monospace"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .filter(|path| !path.is_empty())
}

/// Windows has no fontconfig: Consolas, which it ships.
#[cfg(windows)]
fn monospace_file() -> Option<String> {
    std::env::var("WINDIR")
        .ok()
        .map(|windows| format!("{windows}\\Fonts\\consola.ttf"))
        .filter(|path| std::path::Path::new(path).exists())
}

/// The fallback list, and the scripts the shaper asks about.
struct Fallback(&'static [&'static str]);

impl cosmic_text::Fallback for Fallback {
    fn common_fallback(&self) -> &[&'static str] {
        self.0
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }

    /// cosmic-text asks once per script it meets. A script DM Sans may lack
    /// is queued for [`load_wanted`], which loads the system's font for it
    /// outside the shaper's lock. The answer names the alias that font's
    /// face will have, so the script's text takes it ahead of any font
    /// chosen for single characters.
    fn script_fallback(&self, script: unicode_script::Script, _locale: &str) -> &[&'static str] {
        let Some(language) = script_language(script) else {
            return &[];
        };
        WANTED.lock().expect("wanted scripts").push(script);
        wake();
        std::slice::from_ref(alias(language))
    }

    /// cosmic-text (patch 1 in `vendor/cosmic-text`) hands over each
    /// cluster DM Sans, Twemoji and the script's font lack, every time it
    /// shapes one, and tries the families named here next.
    fn missing(&self, cluster: &str, families: &mut Vec<&'static str>) {
        if ask(cluster, families) {
            wake();
        }
    }
}

/// Has the window call [`load_wanted`].
fn wake() {
    if let Some(wake) = &*WAKE.lock().expect("font wake") {
        let _ = wake.unbounded_send(());
    }
}

/// Each character a cluster was missing, sorted, with the family of the
/// face chosen for it, or none while it's being looked up or when no font
/// has it: each character is looked up once.
static CHOSEN: Mutex<Vec<(char, Option<&'static str>)>> = Mutex::new(Vec::new());

/// Adds the families chosen for `cluster`'s characters to `families`, and
/// queues it for [`load_wanted`] if it has a character not asked about
/// before. ASCII is DM Sans's, and the characters shapers draw nothing for
/// don't count.
fn ask(cluster: &str, families: &mut Vec<&'static str>) -> bool {
    let mut chosen = CHOSEN.lock().expect("chosen fonts");
    let mut new = false;
    for c in cluster.chars().filter(|&c| needs_glyph(c)) {
        match chosen.binary_search_by_key(&c, |&(known, _)| known) {
            Ok(at) => {
                if let Some(family) = chosen[at].1
                    && !families.contains(&family)
                {
                    families.push(family);
                }
            }
            Err(at) => {
                chosen.insert(at, (c, None));
                new = true;
            }
        }
    }
    if new {
        MISSING
            .lock()
            .expect("missing clusters")
            .push(cluster.into());
    }
    new
}

/// Whether a character needs a glyph of its own: not ASCII, which DM Sans
/// has, and not one of the default-ignorable characters (variation
/// selectors, joiners, tags), which a shaper hides when a font lacks them.
fn needs_glyph(c: char) -> bool {
    !c.is_ascii()
        && !matches!(c,
            '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}'..='\u{1160}' | '\u{17b4}'..='\u{17b5}'
            | '\u{180b}'..='\u{180f}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}' | '\u{3164}' | '\u{fe00}'..='\u{fe0f}' | '\u{feff}'
            | '\u{ffa0}' | '\u{fff0}'..='\u{fff8}' | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}' | '\u{e0000}'..='\u{e0fff}')
}

/// Clusters with characters not asked about before, waiting for a font.
static MISSING: Mutex<Vec<Box<str>>> = Mutex::new(Vec::new());

/// The family name a language's fallback face is known by too; one leaked
/// string per language, at most the few dozen [`script_language`] names.
fn alias(language: &'static str) -> &'static &'static str {
    static ALIASES: Mutex<Vec<(&'static str, &'static &'static str)>> = Mutex::new(Vec::new());
    let mut aliases = ALIASES.lock().expect("aliases");
    if let Some((_, alias)) = aliases.iter().find(|(known, _)| *known == language) {
        return alias;
    }
    let name: &'static str = Box::leak(format!("Yapfast fallback {language}").into_boxed_str());
    let alias: &'static &'static str = Box::leak(Box::new(name));
    aliases.push((language, alias));
    alias
}

/// Scripts the shaper met that want a fallback font, and the wake that has
/// the window load them.
static WANTED: Mutex<Vec<unicode_script::Script>> = Mutex::new(Vec::new());
static WAKE: Mutex<Option<mpsc::UnboundedSender<()>>> = Mutex::new(None);

/// One item each time fonts the shaper wanted are in, for a subscription:
/// the window then lays its text out again. The fonts are looked up and
/// loaded on a blocking thread, as asking fontconfig runs a program.
pub fn wakes() -> impl Stream<Item = ()> {
    let (wake, wakes) = mpsc::unbounded();
    if !WANTED.lock().expect("wanted scripts").is_empty()
        || !MISSING.lock().expect("missing clusters").is_empty()
    {
        let _ = wake.unbounded_send(());
    }
    *WAKE.lock().expect("font wake") = Some(wake);
    wakes.filter_map(|()| async {
        tokio::task::spawn_blocking(load_wanted)
            .await
            .unwrap_or(false)
            .then_some(())
    })
}

/// The language fontconfig knows a script by, for scripts DM Sans lacks.
/// Han goes by the locale: Chinese, Japanese and Korean draw it differently.
fn script_language(script: unicode_script::Script) -> Option<&'static str> {
    use unicode_script::Script;
    Some(match script {
        Script::Hangul => "ko",
        Script::Hiragana | Script::Katakana => "ja",
        Script::Han => {
            let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
                .iter()
                .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
                .unwrap_or_default()
                .to_lowercase();
            match locale.get(..5).unwrap_or(&locale) {
                tag if tag.starts_with("ja") => "ja",
                tag if tag.starts_with("ko") => "ko",
                "zh_tw" | "zh_hk" | "zh_mo" => "zh-tw",
                _ => "zh-cn",
            }
        }
        Script::Bopomofo => "zh-tw",
        Script::Cyrillic => "ru",
        Script::Greek => "el",
        Script::Arabic => "ar",
        Script::Hebrew => "he",
        Script::Armenian => "hy",
        Script::Georgian => "ka",
        Script::Thai => "th",
        Script::Lao => "lo",
        Script::Khmer => "km",
        Script::Myanmar => "my",
        Script::Devanagari => "hi",
        Script::Bengali => "bn",
        Script::Gurmukhi => "pa",
        Script::Gujarati => "gu",
        Script::Tamil => "ta",
        Script::Telugu => "te",
        Script::Kannada => "kn",
        Script::Malayalam => "ml",
        Script::Sinhala => "si",
        Script::Tibetan => "bo",
        Script::Ethiopic => "am",
        _ => return None,
    })
}

/// The system's font for a language, as fontconfig ranks them, if it covers
/// the language: its file and face index.
#[cfg(not(windows))]
fn language_font(language: &str) -> Option<(String, u32)> {
    let output = std::process::Command::new("fc-match")
        .args([
            "-f",
            "%{file}\n%{index}\n%{lang}",
            &format!(":lang={language}"),
        ])
        .output()
        .ok()?;
    let output = String::from_utf8(output.stdout).ok()?;
    let mut lines = output.lines();
    let (file, index, languages) = (
        lines.next()?,
        lines.next()?,
        lines.next().unwrap_or_default(),
    );
    // A variable font's named instance rides in the high bits.
    let index = index.parse::<u32>().ok()? & 0xffff;
    languages
        .split('|')
        .any(|known| known == language)
        .then(|| (file.to_owned(), index))
}

/// Windows has no fontconfig: the fonts it ships for each language.
#[cfg(windows)]
fn language_font(language: &str) -> Option<(String, u32)> {
    let files: &[&str] = match language {
        "ko" => &["malgun.ttf"],
        "ja" => &["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"],
        "zh-cn" => &["msyh.ttc", "simsun.ttc"],
        "zh-tw" => &["msjh.ttc", "mingliu.ttc"],
        "th" | "lo" | "km" => &["LeelawUI.ttf", "leelawad.ttf"],
        "my" => &["mmrtext.ttf"],
        "bo" => &["himalaya.ttf"],
        "am" => &["ebrima.ttf"],
        "hi" | "bn" | "pa" | "gu" | "ta" | "te" | "kn" | "ml" | "si" => {
            &["Nirmala.ttc", "Nirmala.ttf"]
        }
        _ => &["segoeui.ttf"],
    };
    let fonts = std::path::Path::new(&std::env::var("WINDIR").ok()?).join("Fonts");
    files
        .iter()
        .map(|file| fonts.join(file))
        .find(|path| path.exists())
        .map(|path| (path.to_string_lossy().into_owned(), 0))
}

/// Loads the system's font for each script and each character the shaper
/// queued, one face per file, and has iced shape its text again: whether
/// anything changed.
pub fn load_wanted() -> bool {
    static TRIED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let start = std::time::Instant::now();
    let wanted = std::mem::take(&mut *WANTED.lock().expect("wanted scripts"));
    // The faces this call loads, which may have a queued character too.
    let mut added = Vec::new();
    for script in wanted {
        let Some(language) = script_language(script) else {
            continue;
        };
        {
            let mut tried = TRIED.lock().expect("tried languages");
            if tried.contains(&language) {
                continue;
            }
            tried.push(language);
        }
        let Some((file, index)) = language_font(language) else {
            tracing::info!(?script, language, "no fallback font");
            continue;
        };
        if let Some(id) = load_face(Some(language), &file, index, &[]) {
            tracing::info!(?script, language, file, index, "fallback font loaded");
            added.push(id);
        }
    }
    let mut changed = !added.is_empty();
    let clusters = std::mem::take(&mut *MISSING.lock().expect("missing clusters"));
    for cluster in clusters {
        let chars: Vec<char> = cluster.chars().filter(|&c| needs_glyph(c)).collect();
        let blocks: Vec<u32> = chars.iter().map(|&c| u32::from(c) >> 7).collect();
        // A face chosen for a character of the same block before, or loaded
        // for a script just now, that has these too saves asking the system.
        let reused = {
            let mut system = font_system().write().expect("font system");
            let db = system.raw().db();
            let faces = FACES.lock().expect("chosen faces");
            faces
                .iter()
                .filter(|(block, _)| blocks.contains(block))
                .map(|&(_, id)| id)
                .chain(added.iter().copied())
                .find(|&id| has_chars(db, id, &chars))
        };
        let found = reused.or_else(|| {
            char_fonts(&chars).into_iter().find_map(|(file, index)| {
                let id = load_face(None, &file, index, &chars)?;
                tracing::info!(cluster = &*cluster, file, index, "fallback font loaded");
                Some(id)
            })
        });
        let Some(id) = found else {
            tracing::info!(cluster = &*cluster, "no font has it");
            continue;
        };
        let Some(family) = font_system()
            .write()
            .expect("font system")
            .raw()
            .db()
            .face(id)
            .and_then(|face| face.families.first())
            .map(|(family, _)| intern(family))
        else {
            continue;
        };
        {
            let mut faces = FACES.lock().expect("chosen faces");
            for &block in &blocks {
                if !faces.contains(&(block, id)) {
                    faces.push((block, id));
                }
            }
        }
        let mut chosen = CHOSEN.lock().expect("chosen fonts");
        for c in chars {
            if let Ok(at) = chosen.binary_search_by_key(&c, |&(known, _)| known) {
                chosen[at].1 = Some(family);
            }
        }
        changed = true;
    }
    if changed {
        tracing::info!(elapsed = ?start.elapsed(), "fallback fonts chosen");
        // iced shapes text again only when its font system's version moves,
        // and only `load_font` moves it. It would copy a real font, so it
        // gets a one-byte stand-in, which fontdb rejects. An owned one: iced
        // takes borrowed bytes at an address it has seen before as loaded
        // already, and the version wouldn't move a second time.
        font_system()
            .write()
            .expect("font system")
            .load_font(Cow::Owned(vec![0]));
    }
    changed
}

/// The faces chosen for characters, by the block of 128 code points each
/// was chosen for.
static FACES: Mutex<Vec<(u32, cosmic_text::fontdb::ID)>> = Mutex::new(Vec::new());

/// A font family's name to hand cosmic-text, one leaked string per family
/// chosen, a few at most.
fn intern(family: &str) -> &'static str {
    static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut names = NAMES.lock().expect("family names");
    if let Some(name) = names.iter().find(|name| **name == family) {
        return name;
    }
    let name: &'static str = Box::leak(family.into());
    names.push(name);
    name
}

/// Adds the face `index` of `file` (a collection keeps only that one) if it
/// has a glyph for each of `chars`, and the face again where cosmic-text
/// would pass it over: under the alias of the `language` it's loaded for,
/// which the script's text asks for by name, and upright and italic at
/// normal width, as cosmic-text tries only the faces of the text's own style
/// and width. Any weight does: the shaper takes a face at the weight nearest
/// the text's (patch 2 in `vendor/cosmic-text`). Its id, if it's in.
fn load_face(
    language: Option<&'static str>,
    file: &str,
    index: u32,
    chars: &[char],
) -> Option<cosmic_text::fontdb::ID> {
    use cosmic_text::fontdb::{Language, Stretch, Style};
    let mut system = font_system().write().expect("font system");
    let db = system.raw().db_mut();
    let path = std::path::Path::new(file);
    let known = |db: &cosmic_text::fontdb::Database| {
        db.faces()
            .filter(|face| file_of(&face.source) == Some(path) && face.index == index)
            .map(|face| face.id)
            .next()
    };
    let id = match known(db) {
        Some(id) => id,
        None => {
            if let Err(error) = db.load_font_file(path) {
                tracing::warn!(%error, file, "could not load a fallback font");
                return None;
            }
            let loaded: Vec<_> = db
                .faces()
                .filter(|face| file_of(&face.source) == Some(path))
                .map(|face| (face.id, face.index))
                .collect();
            for &(id, face_index) in &loaded {
                if face_index != index || !has_chars(db, id, chars) {
                    db.remove_face(id);
                }
            }
            known(db)?
        }
    };
    if !has_chars(db, id, chars) {
        return None;
    }
    let mut face = db.face(id)?.clone();
    if let Some(language) = language {
        face.families = vec![(
            (*alias(language)).to_owned(),
            Language::English_UnitedStates,
        )];
    }
    for style in [Style::Normal, Style::Italic] {
        let there = db.faces().any(|known| {
            file_of(&known.source) == Some(path)
                && known.index == index
                && known.families == face.families
                && known.style == style
                && known.stretch == Stretch::Normal
        });
        if !there {
            db.push_face_info(cosmic_text::fontdb::FaceInfo {
                style,
                stretch: Stretch::Normal,
                ..face.clone()
            });
        }
    }
    Some(id)
}

/// The file a face comes from: named, or once drawn with, shared.
fn file_of(source: &cosmic_text::fontdb::Source) -> Option<&std::path::Path> {
    use cosmic_text::fontdb::Source;
    match source {
        Source::File(path) | Source::SharedFile(path, _) => Some(path),
        Source::Binary(_) => None,
    }
}

/// Whether face `id` has a glyph for each of `chars`, from its character
/// map, read where fontdb maps the file.
fn has_chars(
    db: &cosmic_text::fontdb::Database,
    id: cosmic_text::fontdb::ID,
    chars: &[char],
) -> bool {
    chars.is_empty()
        || db
            .with_face_data(id, |data, index| {
                ttf_parser::Face::parse(data, index)
                    .is_ok_and(|face| chars.iter().all(|&c| face.glyph_index(c).is_some()))
            })
            .unwrap_or(false)
}

/// The system's fonts that may have `chars`, best first: fontconfig's
/// best match for them.
#[cfg(not(windows))]
fn char_fonts(chars: &[char]) -> Vec<(String, u32)> {
    let charset: Vec<String> = chars
        .iter()
        .map(|&c| format!("{:x}", u32::from(c)))
        .collect();
    let Ok(output) = std::process::Command::new("fc-match")
        .args([
            "-f",
            "%{file}\n%{index}",
            &format!(":charset={}", charset.join(" ")),
        ])
        .output()
    else {
        return Vec::new();
    };
    let output = String::from_utf8_lossy(&output.stdout);
    let mut lines = output.lines();
    match (
        lines.next(),
        lines.next().and_then(|index| index.parse::<u32>().ok()),
    ) {
        // A variable font's named instance rides in the high bits.
        (Some(file), Some(index)) if !file.is_empty() => vec![(file.to_owned(), index & 0xffff)],
        _ => Vec::new(),
    }
}

/// Windows has no fontconfig: the fonts it ships for symbols, letter-like
/// characters and the scripts its UI font lacks, in the order they're tried.
#[cfg(windows)]
fn char_fonts(_chars: &[char]) -> Vec<(String, u32)> {
    const FILES: &[(&str, u32)] = &[
        ("segoeui.ttf", 0),
        ("seguisym.ttf", 0),
        // Cambria Math.
        ("cambria.ttc", 1),
        ("seguihis.ttf", 0),
        ("gadugi.ttf", 0),
        ("ebrima.ttf", 0),
        ("himalaya.ttf", 0),
        ("msgothic.ttc", 0),
        ("simsunb.ttf", 0),
        ("seguiemj.ttf", 0),
    ];
    let Ok(windows) = std::env::var("WINDIR") else {
        return Vec::new();
    };
    let fonts = std::path::Path::new(&windows).join("Fonts");
    FILES
        .iter()
        .map(|&(file, index)| (fonts.join(file), index))
        .filter(|(path, _)| path.exists())
        .map(|(path, index)| (path.to_string_lossy().into_owned(), index))
        .collect()
}

/// The font of a display name style's `family` at `weight`, or DM Sans.
/// Each of the eleven files is added to the font system the first time a
/// name needs it; the bytes are the binary's own, mapped with it, and only
/// the faces asked for are parsed.
pub fn name_font(family: Option<&'static str>, weight: u16) -> Font {
    let Some(family) = family else {
        return Font {
            weight: font_weight(weight),
            ..REGULAR
        };
    };
    static LOADED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut loaded = LOADED.lock().expect("loaded name fonts");
    if !loaded.contains(&family) {
        loaded.push(family);
        if let Some(bytes) = name_font_bytes(family) {
            let mut system = font_system().write().expect("font system");
            // `db_mut` also forgets the matches cosmic-text cached.
            system
                .raw()
                .db_mut()
                .load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(bytes)));
            tracing::info!(family, "display name font loaded");
        }
    }
    Font {
        family: Family::Name(family),
        weight: font_weight(weight),
        ..REGULAR
    }
}

/// The bytes of a display name style's font family, from the kit's `fonts/`.
fn name_font_bytes(family: &str) -> Option<&'static [u8]> {
    macro_rules! font {
        ($file:literal) => {
            include_bytes!(concat!("../fonts/", $file)).as_slice()
        };
    }
    Some(match family {
        "Sakura" => font!("DisplayName-Sakura.ttf"),
        "Jellybean" => font!("DisplayName-Jellybean.ttf"),
        "Modern" => font!("DisplayName-Modern.ttf"),
        "Néo-castel" => font!("DisplayName-Medieval.otf"),
        "8Bit" => font!("DisplayName-8Bit.ttf"),
        "Sinistre" => font!("DisplayName-Vampyre.ttf"),
        "Tempo" => font!("DisplayName-Tempo.ttf"),
        "Monkey Bars" => font!("DisplayName-MonkeyBars.ttf"),
        "Mainframe" => font!("DisplayName-Mainframe.ttf"),
        "Headbang" => font!("DisplayName-Headbang.ttf"),
        "Journal" => font!("DisplayName-Journal.ttf"),
        _ => return None,
    })
}

/// iced's nine weights, nearest first: 548 shows as 500.
pub fn font_weight(weight: u16) -> Weight {
    match (weight + 50) / 100 {
        0 | 1 => Weight::Thin,
        2 => Weight::ExtraLight,
        3 => Weight::Light,
        4 => Weight::Normal,
        5 => Weight::Medium,
        6 => Weight::Semibold,
        7 => Weight::Bold,
        8 => Weight::ExtraBold,
        _ => Weight::Black,
    }
}

/// A name's own font, as a name style sets it: its family and weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameFont {
    pub family: &'static str,
    pub weight: u16,
}

/// A name as it's given to the font system. Journal has no space, only a
/// no-break one, and cosmic-text draws the font's missing-glyph box for a
/// space rather than taking one from a fallback font.
pub fn shown_name(name: &str, font: Option<NameFont>) -> Cow<'_, str> {
    match font.map(|font| font.family) {
        Some("Journal") if name.contains(' ') => name.replace(' ', "\u{a0}").into(),
        _ => name.into(),
    }
}

/// The font a name shows in: its own, else DM Sans at `weight`.
pub fn style_font(font: Option<NameFont>, weight: u16) -> Font {
    match font {
        Some(font) => name_font(Some(font.family), font.weight),
        None => name_font(None, weight),
    }
}

/// A checked theme font loaded into the window's font system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeFont {
    /// The face to draw with, including the file's weight, width and style.
    pub font: Font,
    /// The family name iced looks the face up by.
    pub family: &'static str,
    /// Another, different file already occupies this family and face.
    /// iced cannot distinguish the two, so callers can surface the warning.
    pub clash: Option<ThemeFontClash>,
}

/// A different file already loaded under the same family and face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeFontClash {
    pub family: &'static str,
}

/// Why bytes accepted for a theme could not be named as an iced font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeFontError {
    /// The bytes are not a font ttf-parser can read.
    NotAFont,
    /// The font has neither a typographic nor an ordinary family name.
    Unnamed,
}

impl std::fmt::Display for ThemeFontError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotAFont => "isn't a font Yapfast can draw with",
            Self::Unnamed => "has no family name to draw it by",
        })
    }
}

impl std::error::Error for ThemeFontError {}

/// What distinguishes a face within a family. iced cannot select two files
/// with the same values here independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ThemeFace {
    weight: u16,
    italic: bool,
    width: u16,
}

/// Theme fonts loaded for this font-system installation. Identical bytes are
/// parsed and inserted once; names are leaked once because [`Font`] requires
/// a static family name for the lifetime of the process.
#[derive(Default)]
struct ThemeFonts {
    names: HashSet<&'static str>,
    loaded: HashMap<u64, (&'static str, ThemeFace)>,
    faces: HashMap<(&'static str, ThemeFace), Vec<u64>>,
}

fn theme_fonts() -> &'static Mutex<ThemeFonts> {
    static THEME_FONTS: std::sync::OnceLock<Mutex<ThemeFonts>> = std::sync::OnceLock::new();
    THEME_FONTS.get_or_init(Mutex::default)
}

/// Loads a checked installed-theme font directly into Yapfast's custom
/// cosmic-text font system and returns the iced face that selects it.
///
/// The family is read the same way fontdb does: an English typographic family
/// first, then another typographic family, then the ordinary family. Repeated
/// bytes are deduplicated. A distinct file with the same family, weight,
/// width and style is loaded but reported as a clash because iced may draw
/// whichever one its database finds first.
pub fn load_theme_font(bytes: Vec<u8>) -> Result<ThemeFont, ThemeFontError> {
    let digest = theme_font_digest(&bytes);
    let mut registry = theme_fonts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&(family, face)) = registry.loaded.get(&digest) {
        return Ok(ThemeFont {
            font: theme_iced_font(family, face),
            family,
            clash: theme_font_clash(&registry, family, face, digest),
        });
    }

    let (name, face) = read_theme_font(&bytes)?;
    let family = match registry.names.get(name.as_str()) {
        Some(family) => *family,
        None => {
            let family: &'static str = Box::leak(name.into_boxed_str());
            registry.names.insert(family);
            family
        }
    };
    let clash = theme_font_clash(&registry, family, face, digest);

    // The core checked the bytes before installation; parsing is still kept
    // fallible above because a data folder may be damaged after that check.
    font_system()
        .write()
        .expect("font system")
        .raw()
        .db_mut()
        .load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(bytes)));

    registry.loaded.insert(digest, (family, face));
    registry
        .faces
        .entry((family, face))
        .or_default()
        .push(digest);
    if clash.is_some() {
        tracing::warn!(
            family,
            "an installed theme font clashes with another theme font face"
        );
    }
    Ok(ThemeFont {
        font: theme_iced_font(family, face),
        family,
        clash,
    })
}

/// The family name an installed-theme font would use, without loading it.
#[cfg(test)]
pub fn theme_font_family(bytes: &[u8]) -> Option<String> {
    read_theme_font(bytes).ok().map(|(family, _)| family)
}

fn read_theme_font(bytes: &[u8]) -> Result<(String, ThemeFace), ThemeFontError> {
    // ttf-parser is careful with untrusted input, but keep a malformed font
    // from unwinding through app startup if the data folder was altered.
    let read = std::panic::catch_unwind(|| {
        let face = ttf_parser::Face::parse(bytes, 0).ok()?;
        let name = |id: u16| {
            let mut found: Vec<(String, bool)> = face
                .names()
                .into_iter()
                .filter(|name| name.name_id == id && name.is_unicode())
                .filter_map(|name| {
                    let english = name.language() == ttf_parser::Language::English_UnitedStates;
                    name.to_string().map(|text| (text, english))
                })
                .collect();
            found.sort_by_key(|(_, english)| !english);
            found.into_iter().map(|(text, _)| text).next()
        };
        let family = name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
            .or_else(|| name(ttf_parser::name_id::FAMILY));
        Some((
            family,
            ThemeFace {
                weight: face.weight().to_number(),
                italic: face.is_italic(),
                width: face.width().to_number(),
            },
        ))
    });
    match read {
        Ok(Some((Some(family), face))) if !family.is_empty() => Ok((family, face)),
        Ok(Some(_)) => Err(ThemeFontError::Unnamed),
        _ => Err(ThemeFontError::NotAFont),
    }
}

fn theme_font_clash(
    registry: &ThemeFonts,
    family: &'static str,
    face: ThemeFace,
    digest: u64,
) -> Option<ThemeFontClash> {
    registry
        .faces
        .get(&(family, face))
        .is_some_and(|digests| digests.iter().any(|other| *other != digest))
        .then_some(ThemeFontClash { family })
}

fn theme_iced_font(family: &'static str, face: ThemeFace) -> Font {
    let stretch = match face.width.clamp(1, 9) {
        1 => Stretch::UltraCondensed,
        2 => Stretch::ExtraCondensed,
        3 => Stretch::Condensed,
        4 => Stretch::SemiCondensed,
        5 => Stretch::Normal,
        6 => Stretch::SemiExpanded,
        7 => Stretch::Expanded,
        8 => Stretch::ExtraExpanded,
        _ => Stretch::UltraExpanded,
    };
    Font {
        weight: font_weight(face.weight),
        stretch,
        style: if face.italic {
            Style::Italic
        } else {
            Style::Normal
        },
        ..Font::with_name(family)
    }
}

fn theme_font_digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_font_is_named_loaded_and_deduplicated() {
        crate::testing::install_fonts();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fonts/AtkinsonHyperlegibleNext-Regular.ttf");
        let bytes = std::fs::read(&path).expect("the checked-in readable font");
        let family = theme_font_family(&bytes).expect("its family");
        let first = load_theme_font(bytes.clone()).expect("loaded");
        assert_eq!(first.family, family);
        assert_eq!(first.font.family, Family::Name(first.family));
        assert_eq!(first.clash, None);
        let faces = font_system()
            .write()
            .expect("font system")
            .raw()
            .db()
            .faces()
            .filter(|face| face.families.iter().any(|(known, _)| known == first.family))
            .count();
        assert!(faces > 0, "inserted into the active font database");

        let again = load_theme_font(bytes).expect("loaded once");
        assert_eq!(again, first);
        assert_eq!(
            font_system()
                .write()
                .expect("font system")
                .raw()
                .db()
                .faces()
                .filter(|face| face.families.iter().any(|(known, _)| known == first.family))
                .count(),
            faces,
            "identical bytes do not add another face"
        );
    }

    #[test]
    fn damaged_theme_font_bytes_are_refused() {
        assert_eq!(
            load_theme_font(b"not a font".to_vec()).unwrap_err(),
            ThemeFontError::NotAFont
        );
        assert_eq!(theme_font_family(b"not a font"), None);
    }

    /// Each glyph of `text` in DM Sans at `weight`, upright or italic: its
    /// glyph id, and its face's file.
    fn shape(text: &str, weight: u16, italic: bool) -> Vec<(u16, Option<std::path::PathBuf>)> {
        use cosmic_text::{Attrs, Buffer, Metrics, Shaping};
        let mut system = font_system().write().expect("font system");
        let raw = system.raw();
        let mut buffer = Buffer::new(raw, Metrics::new(16.0, 20.0));
        let attrs = Attrs::new()
            .family(cosmic_text::Family::Name("DM Sans"))
            .weight(cosmic_text::Weight(weight))
            .style(if italic {
                cosmic_text::Style::Italic
            } else {
                cosmic_text::Style::Normal
            });
        buffer.set_text(raw, text, &attrs, Shaping::Advanced, None);
        buffer
            .layout_runs()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|glyph| (glyph.glyph_id, glyph.font_id))
            })
            .map(|(glyph, id)| {
                let file = raw
                    .db()
                    .face(id)
                    .and_then(|face| file_of(&face.source).map(Into::into));
                (glyph, file)
            })
            .collect()
    }

    /// How many faces the font system holds from `file`.
    fn faces_from(file: &std::path::Path) -> usize {
        let mut system = font_system().write().expect("font system");
        system
            .raw()
            .db()
            .faces()
            .filter(|face| file_of(&face.source) == Some(file))
            .count()
    }

    /// A character none of the window's own fonts has (Fraktur, of the
    /// Common script) gets the system's font for it, loaded once, and
    /// draws in it at every weight and in italics.
    #[test]
    fn a_character_no_font_has_gets_the_systems_font_once() {
        crate::testing::install_fonts();
        let Some((file, _)) = char_fonts(&['𝔟']).into_iter().next() else {
            eprintln!("no system font has 𝔟; nothing to test");
            return;
        };
        let file = std::path::PathBuf::from(file);
        let name = "\u{1d51f}\u{1d51e}\u{1d531}";
        assert!(
            shape(name, 400, false).iter().all(|(glyph, _)| *glyph == 0),
            "boxes before"
        );
        assert!(load_wanted(), "a font was chosen");
        let faces = faces_from(&file);
        assert!(faces > 0, "{} loaded", file.display());
        for (weight, italic) in [(400, false), (600, false), (700, false), (400, true)] {
            let glyphs = shape(name, weight, italic);
            assert!(
                glyphs
                    .iter()
                    .all(|(glyph, face)| *glyph != 0 && face.as_deref() == Some(file.as_path())),
                "weight {weight}, italic {italic}: {glyphs:?}"
            );
        }
        // Shaped again, nothing more is looked up or loaded.
        assert!(
            !MISSING
                .lock()
                .expect("missing clusters")
                .iter()
                .any(|cluster| cluster.contains('\u{1d51f}'))
        );
        load_wanted();
        assert_eq!(faces_from(&file), faces);
    }

    /// The shaper is told the family chosen for a character, and a
    /// character is queued for a font only the first time.
    #[test]
    fn a_character_is_asked_about_once() {
        let mut families = Vec::new();
        assert!(ask("\u{2803}x", &mut families), "new");
        assert!(!ask("\u{2803}", &mut families), "asked already");
        assert!(families.is_empty(), "nothing chosen yet");
        {
            let mut chosen = CHOSEN.lock().expect("chosen fonts");
            let at = chosen
                .binary_search_by_key(&'\u{2803}', |&(known, _)| known)
                .expect("asked");
            chosen[at].1 = Some("Braille Sans");
        }
        assert!(!ask("\u{2803}", &mut families));
        assert_eq!(families, ["Braille Sans"]);
        // Joiners and variation selectors draw nothing, and ASCII is DM Sans's.
        assert!(!ask("a\u{fe0f}\u{200d}", &mut families));
    }
}
