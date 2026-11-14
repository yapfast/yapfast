//! The interface's languages: the 32 Discord offers, and the words Rust
//! writes in them.
//!
//! Every text is its English wording; a translation is looked up by that
//! wording in `translations/<locale>/LC_MESSAGES/yapfast-ui.po`, the file the
//! Slint interface's `@tr` texts are translated from too (see
//! `docs/i18n.md`). A text with no translation reads in English.

use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use crate::clock::DateFormat;

/// How a language counts: which of its plural forms a number takes. The
/// forms are gettext's, so that Slint and Rust agree (`Plural-Forms` in each
/// `.po` file says the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plural {
    /// One form: Chinese, Japanese, Korean, Thai, Vietnamese, Indonesian.
    One,
    /// "1" and the rest: English, German, Spanish, Italian, ...
    OneOther,
    /// 0 and 1 and the rest: French, Brazilian Portuguese.
    ZeroOneOther,
    /// Russian, Ukrainian, Croatian: 1, 2 to 4 and the rest, with the teens.
    Slavic,
    /// Polish: 1, 2 to 4 and the rest, with the teens.
    Polish,
    /// Czech: 1, 2 to 4 and the rest.
    Czech,
    /// Lithuanian: 1, 2 to 9 and the rest, with the teens.
    Lithuanian,
    /// Romanian: 1, few and the rest.
    Romanian,
}

impl Plural {
    /// The index of the form a count takes.
    pub fn index(self, n: u64) -> usize {
        let (m10, m100) = (n % 10, n % 100);
        match self {
            Self::One => 0,
            Self::OneOther => usize::from(n != 1),
            Self::ZeroOneOther => usize::from(n > 1),
            Self::Slavic => {
                if m10 == 1 && m100 != 11 {
                    0
                } else if (2..=4).contains(&m10) && !(10..20).contains(&m100) {
                    1
                } else {
                    2
                }
            }
            Self::Polish => {
                if n == 1 {
                    0
                } else if (2..=4).contains(&m10) && !(10..20).contains(&m100) {
                    1
                } else {
                    2
                }
            }
            Self::Czech => {
                if n == 1 {
                    0
                } else if (2..=4).contains(&n) {
                    1
                } else {
                    2
                }
            }
            Self::Lithuanian => {
                if m10 == 1 && !(11..20).contains(&m100) {
                    0
                } else if (2..=9).contains(&m10) && !(11..20).contains(&m100) {
                    1
                } else {
                    2
                }
            }
            Self::Romanian => {
                if n == 1 {
                    0
                } else if n == 0 || (1..20).contains(&m100) {
                    1
                } else {
                    2
                }
            }
        }
    }

    /// gettext's `Plural-Forms` header for the language's `.po` file.
    pub fn header(self) -> &'static str {
        match self {
            Self::One => "nplurals=1; plural=0;",
            Self::OneOther => "nplurals=2; plural=(n != 1);",
            Self::ZeroOneOther => "nplurals=2; plural=(n > 1);",
            Self::Slavic => {
                "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);"
            }
            Self::Polish => {
                "nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);"
            }
            Self::Czech => "nplurals=3; plural=((n==1) ? 0 : (n>=2 && n<=4) ? 1 : 2);",
            Self::Lithuanian => {
                "nplurals=3; plural=(n%10==1 && (n%100<11 || n%100>19) ? 0 : n%10>=2 && n%10<=9 && (n%100<11 || n%100>19) ? 1 : 2);"
            }
            Self::Romanian => {
                "nplurals=3; plural=(n==1 ? 0 : (n==0 || (n%100 > 0 && n%100 < 20)) ? 1 : 2);"
            }
        }
    }
}

/// A language Discord offers, and what Yapfast knows of it.
#[derive(Debug)]
pub struct Locale {
    /// Discord's code for it, such as `pt-BR`; the folder in `translations/` too.
    pub code: &'static str,
    /// The language's name in itself, for the settings page.
    pub native: &'static str,
    /// Its name in English.
    pub english: &'static str,
    pub plural: Plural,
    /// The order of day, month and year where the language is spoken.
    pub dates: DateFormat,
    /// The translations, as a `.po` file; empty for the English they are keyed
    /// by and for a language nobody has translated yet.
    po: &'static str,
}

/// A language of [`LOCALES`]. With `translated` at the end, its translations
/// are read from `translations/<code>/LC_MESSAGES/yapfast-ui.po`, which must
/// exist; without it, the language reads in English until someone writes them.
macro_rules! locale {
    ($code:literal, $native:literal, $english:literal, $plural:ident, $dates:ident) => {
        locale!($code, $native, $english, $plural, $dates, "")
    };
    ($code:literal, $native:literal, $english:literal, $plural:ident, $dates:ident, translated) => {
        locale!(
            $code,
            $native,
            $english,
            $plural,
            $dates,
            include_str!(concat!(
                "../../../translations/",
                $code,
                "/LC_MESSAGES/yapfast-ui.po"
            ))
        )
    };
    ($code:literal, $native:literal, $english:literal, $plural:ident, $dates:ident, $po:expr) => {
        Locale {
            code: $code,
            native: $native,
            english: $english,
            plural: Plural::$plural,
            dates: DateFormat::$dates,
            po: $po,
        }
    };
}

/// How many languages there are.
const COUNT: usize = 32;

/// The languages, in the order the settings page lists them. A language's
/// place here is what the settings page shows as chosen, not what is saved.
pub static LOCALES: [Locale; COUNT] = [
    locale!(
        "en-US",
        "English (US)",
        "English (US)",
        OneOther,
        MonthDayYear
    ),
    locale!(
        "en-GB",
        "English (UK)",
        "English (UK)",
        OneOther,
        DayMonthYear
    ),
    locale!("bg", "Български", "Bulgarian", OneOther, DayMonthYear),
    locale!("zh-CN", "中文", "Chinese (China)", One, YearMonthDay),
    locale!("zh-TW", "繁體中文", "Chinese (Taiwan)", One, YearMonthDay),
    locale!("hr", "Hrvatski", "Croatian", Slavic, DayMonthYear),
    locale!("cs", "Čeština", "Czech", Czech, DayMonthYear),
    locale!("da", "Dansk", "Danish", OneOther, DayMonthYear),
    locale!("nl", "Nederlands", "Dutch", OneOther, DayMonthYear),
    locale!("fi", "Suomi", "Finnish", OneOther, DayMonthYear),
    locale!(
        "fr",
        "Français",
        "French",
        ZeroOneOther,
        DayMonthYear,
        translated
    ),
    locale!(
        "de",
        "Deutsch",
        "German",
        OneOther,
        DayMonthYear,
        translated
    ),
    locale!("el", "Ελληνικά", "Greek", OneOther, DayMonthYear),
    locale!("hi", "हिन्दी", "Hindi", OneOther, DayMonthYear),
    locale!("hu", "Magyar", "Hungarian", OneOther, YearMonthDay),
    locale!("id", "Bahasa Indonesia", "Indonesian", One, DayMonthYear),
    locale!("it", "Italiano", "Italian", OneOther, DayMonthYear),
    locale!("ja", "日本語", "Japanese", One, YearMonthDay),
    locale!("ko", "한국어", "Korean", One, YearMonthDay),
    locale!("lt", "Lietuvių", "Lithuanian", Lithuanian, YearMonthDay),
    locale!("no", "Norsk", "Norwegian", OneOther, DayMonthYear),
    locale!("pl", "Polski", "Polish", Polish, DayMonthYear),
    locale!(
        "pt-BR",
        "Português (Brasil)",
        "Portuguese (Brazil)",
        ZeroOneOther,
        DayMonthYear
    ),
    locale!("ro", "Română", "Romanian", Romanian, DayMonthYear),
    locale!("ru", "Русский", "Russian", Slavic, DayMonthYear),
    locale!("es-ES", "Español", "Spanish", OneOther, DayMonthYear),
    locale!(
        "es-419",
        "Español (Latinoamérica)",
        "Spanish (Latin America)",
        OneOther,
        DayMonthYear
    ),
    locale!("sv-SE", "Svenska", "Swedish", OneOther, YearMonthDay),
    locale!("th", "ไทย", "Thai", One, DayMonthYear),
    locale!("tr", "Türkçe", "Turkish", OneOther, DayMonthYear),
    locale!("uk", "Українська", "Ukrainian", Slavic, DayMonthYear),
    locale!("vi", "Tiếng Việt", "Vietnamese", One, DayMonthYear),
];

/// One of [`LOCALES`], by its place there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language(u8);

impl Language {
    pub const ENGLISH: Language = Language(0);

    /// Every language, in the settings page's order.
    pub const ALL: [Language; COUNT] = {
        let mut all = [Language(0); COUNT];
        let mut i = 0;
        while i < all.len() {
            all[i] = Language(i as u8);
            i += 1;
        }
        all
    };

    pub fn locale(self) -> &'static Locale {
        &LOCALES[usize::from(self.0).min(COUNT - 1)]
    }

    /// Discord's code for it, such as `de`.
    pub fn code(self) -> &'static str {
        self.locale().code
    }

    /// The language's name in itself, for the settings page.
    pub fn name(self) -> &'static str {
        self.locale().native
    }

    /// Its place in [`Language::ALL`].
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// Whether its texts are the English ones: English itself, or a language
    /// with no translations yet.
    pub fn is_english(self) -> bool {
        self.locale().po.is_empty()
    }

    /// Whether the settings page offers it: the English kinds and each
    /// language that has translations.
    pub fn is_offered(self) -> bool {
        let locale = self.locale();
        !locale.po.is_empty() || locale.code.starts_with("en-")
    }

    /// The languages the settings page offers, in its order.
    pub fn offered() -> Vec<Language> {
        Self::ALL.into_iter().filter(|l| l.is_offered()).collect()
    }

    /// The language with Discord's `code`, such as `pt-BR`; case does not matter.
    pub fn from_code(code: &str) -> Option<Language> {
        Self::ALL
            .into_iter()
            .find(|language| language.code().eq_ignore_ascii_case(code))
    }

    /// The language a locale tag such as `en_AU.UTF-8`, `pt-PT` or `zh-Hant-HK`
    /// is closest to, if Discord offers one for its language.
    pub fn from_tag(tag: &str) -> Option<Language> {
        let tag = tag.split(['.', '@']).next()?.replace('_', "-");
        let mut parts = tag.split('-').map(str::to_ascii_lowercase);
        let language = parts.next()?;
        let rest: Vec<String> = parts.collect();
        let has = |part: &str| rest.iter().any(|p| p == part);
        let code = match language.as_str() {
            "en" if has("gb") || has("au") || has("nz") || has("ie") || has("za") || has("in") => {
                "en-GB"
            }
            "en" => "en-US",
            "zh" if has("hant") || has("tw") || has("hk") || has("mo") => "zh-TW",
            "zh" => "zh-CN",
            "pt" => "pt-BR",
            "es" if rest.is_empty() || has("es") => "es-ES",
            "es" => "es-419",
            "nb" | "nn" | "no" => "no",
            "sv" => "sv-SE",
            other => return Self::from_code(other),
        };
        Self::from_code(code)
    }

    /// The language of the system, English when the interface has none for it.
    pub fn system() -> Language {
        sys_locale::get_locale()
            .and_then(|tag| Self::from_tag(&tag))
            .filter(|language| language.is_offered())
            .unwrap_or(Self::ENGLISH)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switches the language every [`tr`] answers in.
pub fn set_language(language: Language) {
    CURRENT.store(language.0, Ordering::Relaxed);
}

pub fn language() -> Language {
    Language(CURRENT.load(Ordering::Relaxed).min(COUNT as u8 - 1))
}

/// A language's translations: what its `.po` file gives, by English text.
struct Catalog {
    /// The forms of each text, by `context\x04text` or just the text.
    forms: HashMap<String, Vec<String>>,
}

/// The catalogs, each read when its language is first needed and kept.
static CATALOGS: [OnceLock<Catalog>; COUNT] = [const { OnceLock::new() }; COUNT];

fn catalog(language: Language) -> &'static Catalog {
    CATALOGS[language.index()].get_or_init(|| Catalog {
        forms: parse_po(language.locale().po),
    })
}

fn key(context: Option<&str>, text: &str) -> String {
    match context {
        Some(context) => format!("{context}\x04{text}"),
        None => text.to_owned(),
    }
}

fn lookup(language: Language, context: Option<&str>, text: &str) -> Option<&'static [String]> {
    if language.is_english() {
        return None;
    }
    catalog(language)
        .forms
        .get(&key(context, text))
        .map(Vec::as_slice)
}

/// `text` in the current language; in English when it has no translation.
pub fn tr(text: &'static str) -> &'static str {
    tr_in(language(), text)
}

/// `text` in `language`.
pub fn tr_in(language: Language, text: &'static str) -> &'static str {
    lookup(language, None, text)
        .and_then(|forms| forms.first())
        .map_or(text, String::as_str)
}

/// `text` as `context` tells it apart from the same words elsewhere, in the
/// current language.
pub fn tr_ctx(context: &'static str, text: &'static str) -> &'static str {
    tr_ctx_in(language(), context, text)
}

pub fn tr_ctx_in(language: Language, context: &'static str, text: &'static str) -> &'static str {
    lookup(language, Some(context), text)
        .and_then(|forms| forms.first())
        .map_or(text, String::as_str)
}

/// `text` with its `{}` filled in order, and `{0}`, `{1}` by place, from
/// `values`; so a language can put them in the order its sentence needs.
pub fn fill(text: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut next = 0;
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        let place = if name.is_empty() {
            next += 1;
            Some(next - 1)
        } else {
            name.parse::<usize>().ok()
        };
        match place.and_then(|place| values.get(place)) {
            Some(value) => out.push_str(&value.to_string()),
            None => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// A counted text, `one` for 1 and `other` for the rest in English, in the
/// form the current language gives `n`, with `{n}` replaced by the count.
pub fn trn(one: &'static str, other: &'static str, n: usize) -> String {
    trn_in(language(), one, other, n)
}

pub fn trn_in(language: Language, one: &'static str, other: &'static str, n: usize) -> String {
    let index = language.locale().plural.index(n as u64);
    let english = if n == 1 { one } else { other };
    let form = lookup(language, None, one)
        .and_then(|forms| forms.get(index))
        .map_or(english, String::as_str);
    form.replace("{n}", &n.to_string())
}

/// The entries of a `.po` file, by English text (after the context and a
/// `\x04` when it has one), each with its forms. Entries marked fuzzy or left
/// untranslated are left out, so those read in English.
fn parse_po(po: &str) -> HashMap<String, Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Field {
        None,
        Context,
        Id,
        Plural,
        Str(usize),
    }
    let mut entries = HashMap::new();
    let mut context = None::<String>;
    let mut id = String::new();
    let mut forms: Vec<String> = Vec::new();
    let mut field = Field::None;
    let mut fuzzy = false;
    let mut has_id = false;

    let mut finish = |context: &mut Option<String>,
                      id: &mut String,
                      forms: &mut Vec<String>,
                      fuzzy: &mut bool,
                      has_id: &mut bool| {
        if *has_id && !id.is_empty() && !*fuzzy && forms.iter().any(|form| !form.is_empty()) {
            let key = key(context.as_deref(), id);
            entries.insert(key, std::mem::take(forms));
        }
        *context = None;
        id.clear();
        forms.clear();
        *fuzzy = false;
        *has_id = false;
    };

    for line in po.lines() {
        let line = line.trim();
        if line.is_empty() {
            finish(&mut context, &mut id, &mut forms, &mut fuzzy, &mut has_id);
            field = Field::None;
        } else if let Some(flags) = line.strip_prefix("#,") {
            fuzzy |= flags.contains("fuzzy");
        } else if line.starts_with('#') {
        } else if let Some(rest) = line.strip_prefix("msgctxt ") {
            if field != Field::None && has_id {
                finish(&mut context, &mut id, &mut forms, &mut fuzzy, &mut has_id);
            }
            context = Some(unquote(rest));
            field = Field::Context;
        } else if let Some(rest) = line.strip_prefix("msgid_plural ") {
            let _ = rest;
            field = Field::Plural;
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            if field != Field::Context && has_id {
                finish(&mut context, &mut id, &mut forms, &mut fuzzy, &mut has_id);
            }
            id = unquote(rest);
            has_id = true;
            field = Field::Id;
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            let Some((index, text)) = rest.split_once("] ") else {
                continue;
            };
            let index: usize = index.parse().unwrap_or(0);
            if forms.len() <= index {
                forms.resize(index + 1, String::new());
            }
            forms[index] = unquote(text);
            field = Field::Str(index);
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            forms = vec![unquote(rest)];
            field = Field::Str(0);
        } else if line.starts_with('"') {
            let more = unquote(line);
            match field {
                Field::Context => context.get_or_insert_with(String::new).push_str(&more),
                Field::Id => id.push_str(&more),
                Field::Str(index) => {
                    if let Some(form) = forms.get_mut(index) {
                        form.push_str(&more);
                    }
                }
                Field::Plural | Field::None => {}
            }
        }
    }
    finish(&mut context, &mut id, &mut forms, &mut fuzzy, &mut has_id);
    entries
}

/// A `.po` string literal's text: the quoted part with its escapes undone.
fn unquote(literal: &str) -> String {
    let literal = literal.trim();
    let inner = literal
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(literal);
    let mut text = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => text.push('\r'),
            Some('"') => text.push('"'),
            Some('\\') => text.push('\\'),
            Some(other) => {
                text.push('\\');
                text.push(other);
            }
            None => text.push('\\'),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_puts_values_in_by_order_or_place() {
        assert_eq!(fill("Kick {}", &[&"Sam"]), "Kick Sam");
        assert_eq!(fill("{} of {}", &[&2, &5]), "2 of 5");
        assert_eq!(fill("{1} von {0}", &[&5, &2]), "2 von 5");
        assert_eq!(fill("{name} stays", &[&1]), "{name} stays");
        assert_eq!(fill("open {", &[&1]), "open {");
    }

    fn german() -> Language {
        Language::from_code("de").unwrap()
    }

    #[test]
    fn there_are_discords_thirty_two_languages() {
        let mut codes: Vec<&str> = LOCALES.iter().map(|l| l.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 32);
        assert_eq!(Language::ALL.len(), 32);
        assert_eq!(Language::ENGLISH.code(), "en-US");
        assert!(Language::ENGLISH.is_english());
        assert_eq!(german().name(), "Deutsch");
        assert_eq!(Language::from_code("PT-br").unwrap().code(), "pt-BR");
    }

    #[test]
    fn the_settings_offer_english_and_what_is_translated() {
        let offered: Vec<&str> = Language::offered()
            .into_iter()
            .map(Language::code)
            .collect();
        assert_eq!(offered, ["en-US", "en-GB", "fr", "de"]);
        assert!(!Language::from_code("es-ES").unwrap().is_offered());
        // A language nobody translated reads in English rather than breaking.
        assert_eq!(
            tr_in(Language::from_code("es-ES").unwrap(), "Activity"),
            "Activity"
        );
    }

    #[test]
    fn locale_tags_find_the_closest_language() {
        let code = |tag| Language::from_tag(tag).map(Language::code);
        assert_eq!(code("en_US.UTF-8"), Some("en-US"));
        assert_eq!(code("en-AU"), Some("en-GB"));
        assert_eq!(code("de_AT"), Some("de"));
        assert_eq!(code("pt-PT"), Some("pt-BR"));
        assert_eq!(code("es"), Some("es-ES"));
        assert_eq!(code("es-MX"), Some("es-419"));
        assert_eq!(code("zh-Hant-HK"), Some("zh-TW"));
        assert_eq!(code("zh_CN"), Some("zh-CN"));
        assert_eq!(code("nb_NO"), Some("no"));
        assert_eq!(code("sv"), Some("sv-SE"));
        assert_eq!(code("xx"), None);
    }

    #[test]
    fn plural_forms_count_as_each_language_does() {
        assert_eq!(Plural::OneOther.index(1), 0);
        assert_eq!(Plural::OneOther.index(0), 1);
        assert_eq!(Plural::ZeroOneOther.index(0), 0);
        assert_eq!(Plural::ZeroOneOther.index(2), 1);
        assert_eq!(Plural::One.index(7), 0);
        // Russian: 1 file, 2 files, 5 files, 11 files, 21 file, 22 files.
        let ru: Vec<usize> = [1, 2, 5, 11, 21, 22]
            .map(|n| Plural::Slavic.index(n))
            .into();
        assert_eq!(ru, [0, 1, 2, 2, 0, 1]);
        let pl: Vec<usize> = [1, 2, 5, 12, 22, 25]
            .map(|n| Plural::Polish.index(n))
            .into();
        assert_eq!(pl, [0, 1, 2, 2, 1, 2]);
        let cs: Vec<usize> = [1, 3, 5, 0].map(|n| Plural::Czech.index(n)).into();
        assert_eq!(cs, [0, 1, 2, 2]);
        let lt: Vec<usize> = [1, 2, 10, 11, 21]
            .map(|n| Plural::Lithuanian.index(n))
            .into();
        assert_eq!(lt, [0, 1, 2, 2, 0]);
        let ro: Vec<usize> = [1, 0, 7, 20, 21].map(|n| Plural::Romanian.index(n)).into();
        assert_eq!(ro, [0, 1, 1, 2, 2]);
    }

    #[test]
    fn a_po_file_gives_texts_forms_and_contexts() {
        let po = r#"
msgid ""
msgstr ""
"Plural-Forms: nplurals=2; plural=(n != 1);\n"

#: ui/x.slint:1
msgid "Hello"
msgstr "Hallo"

msgctxt "month"
msgid "May"
msgstr "Mai"

#, fuzzy
msgid "Fuzzy"
msgstr "Unsicher"

msgid "Untranslated"
msgstr ""

msgid "{n} file"
msgid_plural "{n} files"
msgstr[0] "{n} Datei"
msgstr[1] "{n} Dateien"

msgid "Two "
"lines \"quoted\"\n"
msgstr "Zwei "
"Zeilen"
"#;
        let entries = parse_po(po);
        assert_eq!(entries["Hello"], ["Hallo"]);
        assert_eq!(entries["month\x04May"], ["Mai"]);
        assert!(!entries.contains_key("Fuzzy"));
        assert!(!entries.contains_key("Untranslated"));
        assert_eq!(entries["{n} file"], ["{n} Datei", "{n} Dateien"]);
        assert_eq!(entries["Two lines \"quoted\"\n"], ["Zwei Zeilen"]);
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn untranslated_texts_read_in_english() {
        assert_eq!(
            tr_in(german(), "No such text anywhere"),
            "No such text anywhere"
        );
        assert_eq!(tr_in(Language::ENGLISH, "Activity"), "Activity");
        assert_eq!(
            trn_in(
                Language::ENGLISH,
                "{n} mutual friend",
                "{n} mutual friends",
                1
            ),
            "1 mutual friend"
        );
        assert_eq!(
            trn_in(
                Language::ENGLISH,
                "{n} mutual friend",
                "{n} mutual friends",
                5
            ),
            "5 mutual friends"
        );
    }

    #[test]
    fn names_and_counts_fill_in() {
        // The process-wide language is left alone, since other tests read it.
        assert_eq!(fill(tr("Message @{}"), &[&"sam"]), "Message @sam");
        assert_eq!(fill(tr("Message #{}"), &[&"dev"]), "Message #dev");
        assert_eq!(fill(tr("{0} and {1}"), &[&"a", &"b"]), "a and b");
    }

    #[test]
    fn every_translation_file_reads() {
        for locale in LOCALES.iter().filter(|l| !l.po.is_empty()) {
            let entries = parse_po(locale.po);
            for (text, forms) in &entries {
                let english = text.rsplit('\x04').next().unwrap_or(text);
                let placeholders = |s: &str| {
                    let mut found: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_owned()))
                        .collect();
                    found.sort();
                    found
                };
                // A counted text's forms may leave the count out ("one file").
                let counted = forms.len() > 1;
                for form in forms.iter().filter(|form| !form.is_empty()) {
                    let (found, wanted) = (placeholders(form), placeholders(english));
                    let fine = if counted {
                        found.iter().all(|name| wanted.contains(name))
                    } else {
                        found == wanted
                    };
                    assert!(
                        fine,
                        "{}: placeholders of {english:?} differ in {form:?}",
                        locale.code
                    );
                }
            }
        }
    }
}
