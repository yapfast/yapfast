//! The fonts text falls back through for a character its own lacks, as
//! cosmic-text asks them: first the fonts for the character's script, then
//! the common list, then every font but the forbidden ones.
//!
//! - Common: Twemoji first, so an emoji a font lacks is always Twemoji's,
//!   then DM Sans (or the person's font in its place), each of the person's
//!   files in the order they were added, the Windows fonts for symbols, then
//!   the platform's list without its emoji font.
//! - Script: the person's files, then Chinese, Japanese and Korean fonts in
//!   the system language's order ([`anomi_fonts::han`]) or the platform's
//!   list; Cyrillic and Greek, which DM Sans lacks, go straight to the
//!   platform's text fonts rather than trying Twemoji and DM Sans first.
//! - Forbidden: every installed font whose family says "emoji", so none of a
//!   system emoji font's pictures shows, and the display name faces, which
//!   answer every weight: a letter neither list has at the weight asked for
//!   would otherwise be tried in each of them, and drawn in one that has it,
//!   before a system font at a near weight. Text in a name face still draws
//!   in it, as cosmic-text keeps the forbidden from the fallback alone.
//!
//! cosmic-text takes a family of these lists only at a weight one of its
//! faces has, so [`super::weights`] gives each system family it names
//! ([`named`]) its nearest face at every other weight.
//!
//! cosmic-text reads a fallback once, when its font system is made, so
//! [`rebuild`] makes the font system again around the same database.
//! Ports `register_emoji` of `src/emoji_font.rs`, `src/han_fallback.rs`, and
//! the Windows list of `src/emoji.rs`.

use std::sync::{LazyLock, Mutex, OnceLock};

use anomi_fonts::Script;
use anomi_fonts::custom::MOST_FILES;
use anomi_fonts::han;
use iced::advanced::graphics::text::cosmic_text::{
    self, Fallback, FontSystem, PlatformFallback, fontdb,
};

use super::{Name, SANS, twemoji};

/// The family each of the person's files also takes, by its place among
/// them, so the fallback tries them in their order: "Anomi Font 0" and on.
pub(super) fn file_family(index: usize) -> &'static str {
    static NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
        (0..MOST_FILES)
            .map(|index| &*format!("Anomi Font {index}").leak())
            .collect()
    });
    NAMES[index.min(MOST_FILES - 1)]
}

/// Whether a family is an emoji font's, by its name.
fn is_emoji(family: &str) -> bool {
    family.to_lowercase().contains("emoji")
}

/// The fallback the interface's text takes.
pub(super) struct Chain {
    common: Vec<&'static str>,
    forbidden: Vec<&'static str>,
    files: Vec<&'static str>,
    /// The platform's text fonts, without its emoji font.
    letters: Vec<&'static str>,
    /// Each script's list, made the first time cosmic-text asks for it.
    scripts: [OnceLock<Box<[&'static str]>>; 256],
}

impl Chain {
    /// The chain over `db`, with the person's first `files` files.
    pub(super) fn new(db: &fontdb::Database, files: usize) -> Self {
        let files: Vec<&'static str> = (0..files.min(MOST_FILES)).map(file_family).collect();
        let letters = letters();
        let mut common = vec![twemoji::FAMILY, SANS];
        common.extend(&files);
        if cfg!(windows) {
            common.extend(han::WINDOWS_SYMBOLS);
        }
        for family in &letters {
            if !common.contains(family) {
                common.push(family);
            }
        }
        let mut forbidden = emoji_families(db);
        forbidden.extend(Name::ALL.iter().map(|face| face.family()));
        Self {
            common,
            forbidden,
            files,
            letters,
            scripts: [const { OnceLock::new() }; 256],
        }
    }
}

impl Fallback for Chain {
    fn common_fallback(&self) -> &[&'static str] {
        &self.common
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &self.forbidden
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        self.scripts[usize::from(script as u8)].get_or_init(|| {
            let letters: &[&'static str] = if matches!(script, Script::Cyrillic | Script::Greek) {
                &self.letters
            } else {
                &[]
            };
            let mut list: Vec<&'static str> = Vec::new();
            for family in self.files.iter().chain(own(script, locale)).chain(letters) {
                if !list.contains(family) {
                    list.push(family);
                }
            }
            list.into_boxed_slice()
        })
    }
}

/// cosmic-text's lists for the platform, kept for the process.
static PLATFORM: PlatformFallback = PlatformFallback;

/// The system's families for `script` in text of a system set to `locale`:
/// for Chinese, Japanese and Korean in the language's order ([`han`]), for
/// any other script the platform's.
fn own(script: Script, locale: &str) -> &'static [&'static str] {
    han::families(script, locale).unwrap_or_else(|| PLATFORM.script_fallback(script, locale))
}

/// The platform's text fonts: its common fallback without its emoji font.
pub(super) fn letters() -> Vec<&'static str> {
    PLATFORM
        .common_fallback()
        .iter()
        .copied()
        .filter(|family| !is_emoji(family))
        .collect()
}

/// Every system family the fallback names in text of a system set to
/// `locale`: the platform's text fonts, the families [`Chain`] asks for each
/// script there is ([`anomi_fonts::scripts`]), every one that draws Chinese,
/// Japanese and Korean among them, and on Windows its symbol fonts; never an
/// emoji font's. [`super::weights::system`] gives each a face at every
/// weight.
pub(super) fn named(locale: &str) -> Vec<&'static str> {
    let mut named = letters();
    let scripts = anomi_fonts::scripts();
    let symbols: &[&'static str] = if cfg!(windows) {
        &han::WINDOWS_SYMBOLS
    } else {
        &[]
    };
    let lists = scripts.into_iter().map(|script| own(script, locale));
    for family in lists.flatten().chain(symbols) {
        if !is_emoji(family) && !named.contains(family) {
            named.push(family);
        }
    }
    named
}

/// The families of `db`'s emoji fonts but Twemoji's, each name kept for the
/// process, as cosmic-text takes `&'static str`: the system's fonts don't
/// change while it runs, so each is kept once.
fn emoji_families(db: &fontdb::Database) -> Vec<&'static str> {
    static KEPT: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut kept = KEPT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut found: Vec<&'static str> = Vec::new();
    for face in db.faces() {
        for (family, _) in &face.families {
            if family == twemoji::FAMILY || !is_emoji(family) {
                continue;
            }
            let name = match kept.iter().find(|known| **known == family) {
                Some(known) => *known,
                None => {
                    let name: &'static str = family.clone().leak();
                    kept.push(name);
                    name
                }
            };
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found
}

/// Puts the interface's fallback, with the person's first `files` files, in
/// `system`: a font system made again of the same database and language,
/// whose caches start empty.
pub(super) fn rebuild(system: &mut FontSystem, files: usize) {
    let empty = FontSystem::new_with_locale_and_db(String::new(), fontdb::Database::new());
    let (locale, db) = std::mem::replace(system, empty).into_locale_and_db();
    let chain = Chain::new(&db, files);
    *system = cosmic_text::FontSystem::new_with_locale_and_db_and_fallback(locale, db, chain);
}
