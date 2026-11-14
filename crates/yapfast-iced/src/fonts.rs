//! The fonts the window draws with, in a font system of the client's own.
//!
//! iced's default font system loads every font on the system (9,474 faces
//! on the machine it was measured on) and copies the fonts given through its
//! API. [`install`] replaces it with one that holds DM Sans and Twemoji from
//! the binary's own bytes and the system's monospace face for code. The
//! display name fonts join it the first time a name needs one
//! ([`name_font`]), and a script DM Sans lacks loads the system's font for
//! it the first time the shaper meets it ([`load_wanted`]). fontdb maps
//! system font files rather than reading them, so nothing is copied.

use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
};

use iced::{
    Font,
    advanced::graphics::text::{cosmic_text, font_system},
    font::{Family, Style, Weight},
    futures::{Stream, StreamExt, channel::mpsc},
};

use crate::looks::NameLook;

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

/// The family name of the Twemoji font `build.rs` makes.
pub const TWEMOJI: &str = "Yapfast Twemoji";

const DM_SANS: &[u8] = include_bytes!("../../../ui/fonts/DMSans[opsz,wght].ttf");

/// Makes the window's font system: DM Sans, `twemoji` (the font the root
/// crate's `build.rs` packs, or nothing), and the system's monospace face,
/// with DM Sans then Twemoji first in the fallback list, so a display name
/// font's missing space comes from DM Sans and ❤ from Twemoji rather than
/// the monospace face.
pub fn install(twemoji: &'static [u8]) {
    let mut db = cosmic_text::fontdb::Database::new();
    db.load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(DM_SANS)));
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
    let mut common: Vec<&'static str> = vec!["DM Sans", TWEMOJI];
    if let Some(mono) = mono {
        db.set_monospace_family(mono.clone());
        common.push(Box::leak(mono.into_boxed_str()));
    }
    db.set_sans_serif_family("DM Sans");
    let fallback = Fallback(Box::leak(common.into_boxed_slice()));
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
    /// face will have, so the script's text takes it ahead of the monospace
    /// face in the common list.
    fn script_fallback(&self, script: unicode_script::Script, _locale: &str) -> &[&'static str] {
        let Some(language) = script_language(script) else {
            return &[];
        };
        WANTED.lock().expect("wanted scripts").push(script);
        if let Some(wake) = &*WAKE.lock().expect("font wake") {
            let _ = wake.unbounded_send(());
        }
        std::slice::from_ref(alias(language))
    }
}

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

/// One item each time scripts are waiting for a font, for a subscription.
pub fn wakes() -> impl Stream<Item = ()> {
    let (wake, wakes) = mpsc::unbounded();
    if !WANTED.lock().expect("wanted scripts").is_empty() {
        let _ = wake.unbounded_send(());
    }
    *WAKE.lock().expect("font wake") = Some(wake);
    wakes.map(|()| ())
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

/// Loads the system's font for each script the shaper queued, one face per
/// file, then has iced shape its text again.
pub fn load_wanted() {
    static TRIED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let wanted = std::mem::take(&mut *WANTED.lock().expect("wanted scripts"));
    let mut loaded = false;
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
        if load_face(language, &file, index) {
            tracing::info!(?script, language, file, index, "fallback font loaded");
            loaded = true;
        }
    }
    if loaded {
        // iced shapes text again only when its font system's version moves,
        // and only `load_font` moves it. It would copy a real font, so it
        // gets a one-byte stand-in, which fontdb rejects.
        const STAND_IN: &[u8] = &[0];
        font_system()
            .write()
            .expect("font system")
            .load_font(Cow::Borrowed(STAND_IN));
    }
}

/// Adds the face `index` of `file` (a collection keeps only that one), and
/// the same face under the language's alias at each weight the window draws
/// text in: cosmic-text takes a script's fallback only at the text's own
/// weight.
fn load_face(language: &'static str, file: &str, index: u32) -> bool {
    use cosmic_text::fontdb::{Database, FaceInfo, Language, Source};
    let mut system = font_system().write().expect("font system");
    let db = system.raw().db_mut();
    let path = std::path::PathBuf::from(file);
    let known = |db: &Database| {
        db.faces()
            .filter(|face| matches!(&face.source, Source::File(known) if *known == path))
            .map(|face| (face.id, face.index))
            .collect::<Vec<_>>()
    };
    if known(db).is_empty() {
        if let Err(error) = db.load_font_file(&path) {
            tracing::warn!(%error, file, "could not load a fallback font");
            return false;
        }
        for (id, face_index) in known(db) {
            if face_index != index {
                db.remove_face(id);
            }
        }
    }
    let Some(face) = known(db)
        .into_iter()
        .find(|&(_, face_index)| face_index == index)
        .and_then(|(id, _)| db.face(id).cloned())
    else {
        return false;
    };
    for weight in [400, 500, 600, 700] {
        db.push_face_info(FaceInfo {
            families: vec![(
                (*alias(language)).to_owned(),
                Language::English_UnitedStates,
            )],
            weight: cosmic_text::fontdb::Weight(weight),
            ..face.clone()
        });
    }
    true
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

/// The bytes of a display name style's font family, from `ui/fonts`.
fn name_font_bytes(family: &str) -> Option<&'static [u8]> {
    macro_rules! font {
        ($file:literal) => {
            include_bytes!(concat!("../../../ui/fonts/", $file)).as_slice()
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

/// A name as it's given to the font system. Journal has no space, only a
/// no-break one, and cosmic-text draws the font's missing-glyph box for a
/// space rather than taking one from a fallback font, as Slint does.
pub fn shown_name<'a>(name: &'a str, style: Option<&NameLook>) -> Cow<'a, str> {
    match style.and_then(|style| style.family) {
        Some("Journal") if name.contains(' ') => name.replace(' ', "\u{a0}").into(),
        _ => name.into(),
    }
}

/// The font a name shows in: its style's, else DM Sans at `weight`.
pub fn style_font(style: Option<&NameLook>, weight: u16) -> Font {
    match style.and_then(|style| style.family.map(|family| (family, style.weight))) {
        Some((family, weight)) => name_font(Some(family), weight),
        None => name_font(None, weight),
    }
}
