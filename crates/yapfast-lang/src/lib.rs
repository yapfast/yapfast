//! The language of Yapfast's interface. The text in the code is English, and
//! each other language is one gettext file, `lang/<code>.po`, built into the
//! program: [`tr!`] and [`trn!`] give Rust's text in the chosen language, and
//! the window's `@tr` asks [`lookup`] and [`lookup_plural`]. See
//! `docs/translations.md`.

use std::borrow::Cow;
use std::fmt::{Display, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A language the interface comes in.
pub struct Language {
    /// Its code, as `layout.json` and a system locale name it: `ru`.
    pub code: &'static str,
    /// Its name in the language itself, which the settings list.
    pub name: &'static str,
    /// Its gettext file; empty for English, which the code holds.
    po: &'static str,
    /// Which of its plural forms a count takes.
    form: fn(u64) -> usize,
    /// The file as read, once the language was first chosen.
    catalog: OnceLock<Catalog>,
}

/// Every language, English first.
pub static LANGUAGES: [Language; 2] = [
    Language {
        code: "en",
        name: "English",
        po: "",
        form: |count| usize::from(count != 1),
        catalog: OnceLock::new(),
    },
    Language {
        code: "ru",
        name: "Русский",
        po: include_str!("../../../lang/ru.po"),
        form: |count| match (count % 10, count % 100) {
            (1, tens) if tens != 11 => 0,
            (2..=4, tens) if !(12..=14).contains(&tens) => 1,
            _ => 2,
        },
        catalog: OnceLock::new(),
    },
];

/// The chosen language, as an index into [`LANGUAGES`].
static CHOSEN: AtomicUsize = AtomicUsize::new(0);

/// Chooses the language of what's translated from now on, by its code or by a
/// locale that starts with it, as `ru_RU.UTF-8` does. Anything else is
/// English. Returns the code chosen.
pub fn choose(locale: &str) -> &'static str {
    let code = locale
        .split(['_', '-', '.', '@'])
        .next()
        .unwrap_or_default();
    let index = LANGUAGES
        .iter()
        .position(|language| language.code.eq_ignore_ascii_case(code))
        .unwrap_or(0);
    CHOSEN.store(index, Ordering::Relaxed);
    LANGUAGES[index].code
}

/// The chosen language.
pub fn chosen() -> &'static Language {
    &LANGUAGES[CHOSEN.load(Ordering::Relaxed)]
}

impl Language {
    /// Whether the interface shows the code's own text, untranslated.
    pub fn is_english(&self) -> bool {
        self.po.is_empty()
    }

    fn catalog(&self) -> &Catalog {
        self.catalog.get_or_init(|| Catalog::read(self.po))
    }
}

/// A text in each of a language's plural forms.
type Forms = Box<[&'static str]>;

/// A language's translations, each list sorted by its English text. Text
/// borrows from the file wherever the file has it on one line with nothing
/// escaped; the rest is allocated once and kept.
#[derive(Default)]
struct Catalog {
    texts: Box<[(&'static str, &'static str)]>,
    /// The few translations that hold in one context only, by text and
    /// context.
    contexts: Box<[((&'static str, &'static str), &'static str)]>,
    /// Text that differs with a count, in each of the language's forms.
    plurals: Box<[(&'static str, Forms)]>,
}

impl Catalog {
    fn read(po: &'static str) -> Self {
        let (mut texts, mut contexts, mut plurals) = (Vec::new(), Vec::new(), Vec::new());
        for entry in entries(po) {
            let Some(first) = entry.translations.first().filter(|text| !text.is_empty()) else {
                continue;
            };
            if entry.text.is_empty() {
                continue;
            }
            if entry.plural.is_some() {
                if entry.translations.iter().all(|text| !text.is_empty()) {
                    plurals.push((entry.text, entry.translations.into_boxed_slice()));
                }
            } else if let Some(context) = entry.context {
                contexts.push(((entry.text, context), *first));
            } else {
                texts.push((entry.text, *first));
            }
        }
        texts.sort_unstable_by_key(|&(text, _)| text);
        contexts.sort_unstable_by_key(|&(key, _)| key);
        plurals.sort_unstable_by_key(|&(text, _)| text);
        Self {
            texts: texts.into(),
            contexts: contexts.into(),
            plurals: plurals.into(),
        }
    }
}

/// One entry of a gettext file.
#[derive(Default)]
struct Entry {
    context: Option<&'static str>,
    text: &'static str,
    plural: Option<&'static str>,
    /// `msgstr`, or each `msgstr[n]` in order.
    translations: Vec<&'static str>,
}

/// The entries of a gettext file, its header among them, with empty text.
fn entries(po: &'static str) -> Vec<Entry> {
    /// The quoted strings after one keyword, joined.
    type Parts = Option<Cow<'static, str>>;
    fn kept(parts: Parts) -> &'static str {
        match parts {
            Some(Cow::Borrowed(text)) => text,
            Some(Cow::Owned(text)) => Box::leak(text.into_boxed_str()),
            None => "",
        }
    }
    #[derive(Default)]
    struct Open {
        context: Parts,
        text: Parts,
        plural: Parts,
        translations: Vec<Parts>,
        /// The keyword the next bare string continues.
        last: Option<Field>,
    }
    #[derive(Clone, Copy)]
    enum Field {
        Context,
        Text,
        Plural,
        Translation(usize),
    }
    impl Open {
        fn field(&mut self, field: Field) -> &mut Parts {
            match field {
                Field::Context => &mut self.context,
                Field::Text => &mut self.text,
                Field::Plural => &mut self.plural,
                Field::Translation(index) => {
                    if self.translations.len() <= index {
                        self.translations.resize(index + 1, None);
                    }
                    &mut self.translations[index]
                }
            }
        }

        fn close(&mut self, entries: &mut Vec<Entry>) {
            let open = std::mem::take(self);
            if open.text.is_none() {
                return;
            }
            entries.push(Entry {
                context: open.context.map(|context| kept(Some(context))),
                text: kept(open.text),
                plural: open.plural.map(|plural| kept(Some(plural))),
                translations: open.translations.into_iter().map(kept).collect(),
            });
        }
    }

    let mut entries = Vec::new();
    let mut open = Open::default();
    for line in po.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let (field, quoted) = match line.split_once(' ') {
            _ if line.starts_with('"') => (open.last, line),
            Some(("msgctxt", rest)) => (Some(Field::Context), rest),
            Some(("msgid", rest)) => (Some(Field::Text), rest),
            Some(("msgid_plural", rest)) => (Some(Field::Plural), rest),
            Some(("msgstr", rest)) => (Some(Field::Translation(0)), rest),
            Some((keyword, rest)) => {
                let index = keyword
                    .strip_prefix("msgstr[")
                    .and_then(|index| index.strip_suffix(']'))
                    .and_then(|index| index.parse().ok());
                (index.map(Field::Translation), rest)
            }
            None => (None, ""),
        };
        let quoted = quoted.trim();
        let (Some(field), Some(inner)) = (
            field,
            quoted
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"')),
        ) else {
            // A blank line ends an entry; so does anything unreadable.
            open.close(&mut entries);
            continue;
        };
        // An entry's first keyword after its translation starts the next,
        // where no blank line parts them.
        if !matches!(field, Field::Translation(_))
            && !line.starts_with('"')
            && !open.translations.is_empty()
        {
            open.close(&mut entries);
        }
        open.last = Some(field);
        let part = unescaped(inner);
        let parts = open.field(field);
        *parts = Some(match parts.take() {
            Some(before) if !before.is_empty() => Cow::Owned(before.into_owned() + &part),
            _ => part,
        });
    }
    open.close(&mut entries);
    entries
}

/// A quoted string of a gettext file without its escapes.
fn unescaped(text: &'static str) -> Cow<'static, str> {
    if !text.contains('\\') {
        return Cow::Borrowed(text);
    }
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            plain.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => plain.push('\n'),
            Some('t') => plain.push('\t'),
            Some(other) => plain.push(other),
            None => {}
        }
    }
    Cow::Owned(plain)
}

/// A text in the chosen language, where its file has it: for the context
/// given first, then for any. None in English.
pub fn lookup(text: &str, context: Option<&str>) -> Option<&'static str> {
    let language = chosen();
    if language.is_english() {
        return None;
    }
    let catalog = language.catalog();
    if let (Some(context), false) = (context, catalog.contexts.is_empty())
        && let Ok(index) = catalog
            .contexts
            .binary_search_by_key(&(text, context), |&(key, _)| key)
    {
        return Some(catalog.contexts[index].1);
    }
    let index = catalog
        .texts
        .binary_search_by_key(&text, |&(text, _)| text)
        .ok()?;
    Some(catalog.texts[index].1)
}

/// A text that differs with a count, in the chosen language's form for that
/// count, by its English text for one. None in English, and where the file
/// lacks it.
pub fn lookup_plural(count: u64, one: &str) -> Option<&'static str> {
    let language = chosen();
    if language.is_english() {
        return None;
    }
    let catalog = language.catalog();
    let index = catalog
        .plurals
        .binary_search_by_key(&one, |(text, _)| text)
        .ok()?;
    catalog.plurals[index]
        .1
        .get((language.form)(count))
        .copied()
}

/// A text in the chosen language, or as given.
pub fn text(text: &'static str) -> &'static str {
    lookup(text, None).unwrap_or(text)
}

/// A text in the chosen language as one context has it, for a text that
/// means two things: `text_in("Tray", "Disconnect")` takes the file's entry
/// with that `msgctxt`, or else the plain one.
pub fn text_in(context: &str, text: &'static str) -> &'static str {
    lookup(text, Some(context)).unwrap_or(text)
}

/// A text for a count in the chosen language, or the English given: `one`
/// for 1, `other` for the rest.
pub fn plural(count: u64, one: &'static str, other: &'static str) -> &'static str {
    lookup_plural(count, one).unwrap_or(if count == 1 { one } else { other })
}

/// A text with its placeholders filled: `{}` takes the next of `values`,
/// `{0}` the one at that place, so a translation can order them its own way,
/// and `{n}` the count. `{{` and `}}` are braces.
pub fn fill(template: &str, count: Option<u64>, values: &[&dyn Display]) -> String {
    let mut filled = String::with_capacity(template.len() + 16 * values.len());
    let mut next = 0;
    let mut rest = template;
    while let Some(at) = rest.find(['{', '}']) {
        filled.push_str(&rest[..at]);
        let brace = &rest[at..at + 1];
        rest = &rest[at + 1..];
        if rest.starts_with(brace) {
            filled.push_str(brace);
            rest = &rest[1..];
            continue;
        }
        let Some((name, after)) = rest.split_once('}').filter(|_| brace == "{") else {
            filled.push_str(brace);
            continue;
        };
        rest = after;
        let value = match name {
            "" => {
                next += 1;
                values.get(next - 1)
            }
            "n" => {
                if let Some(count) = count {
                    let _ = write!(filled, "{count}");
                }
                continue;
            }
            place => place
                .parse::<usize>()
                .ok()
                .and_then(|place| values.get(place)),
        };
        if let Some(value) = value {
            let _ = write!(filled, "{value}");
        }
    }
    filled.push_str(rest);
    filled
}

/// Text in the chosen language. `tr!("Mark as read")` is a `&'static str`;
/// with values, `tr!("Kick {}", name)` is a `String`, each `{}` filled as
/// [`fill`] does. The text must be a literal: `scripts/check-translations`
/// reads it from the code.
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::text($text)
    };
    ($text:literal, $($value:expr),+ $(,)?) => {
        $crate::fill($crate::text($text), None, &[$(&$value as &dyn ::core::fmt::Display),+])
    };
}

/// Text for a count, in the chosen language's form for it:
/// `trn!("{n} member", "{n} members", count)` is a `String`, with `{n}` the
/// count and each `{}` one of the values after it.
#[macro_export]
macro_rules! trn {
    ($one:literal, $other:literal, $count:expr $(, $value:expr)* $(,)?) => {{
        let count = $count as u64;
        $crate::fill(
            $crate::plural(count, $one, $other),
            Some(count),
            &[$(&$value as &dyn ::core::fmt::Display),*],
        )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_placeholders_in_order_by_place_and_by_count() {
        assert_eq!(fill("Kick {}", None, &[&"Sam"]), "Kick Sam");
        assert_eq!(fill("{1} от {0}", None, &[&"Sam", &3]), "3 от Sam");
        assert_eq!(fill("{n} of {}", Some(2), &[&5]), "2 of 5");
        assert_eq!(fill("{{}} and }}{{", None, &[]), "{} and }{");
        assert_eq!(fill("lone } and {", None, &[]), "lone } and {");
    }

    #[test]
    fn reads_entries_with_contexts_plurals_escapes_and_wrapped_lines() {
        let read = entries(concat!(
            "msgid \"\"\nmsgstr \"\"\n\"Language: ru\\n\"\n\n",
            "# A comment.\nmsgid \"Open\"\nmsgstr \"Открыть\"\n",
            "msgctxt \"Thread\"\nmsgid \"Open\"\nmsgstr \"Открыта\"\n\n",
            "msgid \"{n} member\"\nmsgid_plural \"{n} members\"\n",
            "msgstr[0] \"{n} участник\"\nmsgstr[1] \"{n} участника\"\nmsgstr[2] \"{n} участников\"\n\n",
            "msgid \"Say \\\"hi\\\"\"\nmsgstr \"\"\n\"Скажи \"\n\"«привет»\\n\"\n",
        ));
        let texts: Vec<_> = read.iter().map(|entry| entry.text).collect();
        assert_eq!(texts, ["", "Open", "Open", "{n} member", "Say \"hi\""]);
        assert_eq!(read[0].translations, ["Language: ru\n"]);
        assert_eq!(read[1].context, None);
        assert_eq!(read[2].context, Some("Thread"));
        assert_eq!(read[2].translations, ["Открыта"]);
        assert_eq!(read[3].plural, Some("{n} members"));
        assert_eq!(read[3].translations.len(), 3);
        assert_eq!(read[4].translations, ["Скажи «привет»\n"]);
    }

    #[test]
    fn russian_takes_its_form_from_the_count() {
        let form = LANGUAGES[1].form;
        let forms: Vec<_> = [1, 2, 5, 11, 12, 21, 22, 25, 101, 111, 0]
            .into_iter()
            .map(form)
            .collect();
        assert_eq!(forms, [0, 1, 2, 2, 2, 0, 1, 2, 0, 2, 2]);
    }

    /// The placeholders of a text, sorted: a translation fills the same ones.
    fn placeholders(text: &str) -> Vec<&str> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find(['{', '}']) {
            let brace = &rest[at..at + 1];
            rest = &rest[at + 1..];
            if rest.starts_with(brace) {
                rest = &rest[1..];
            } else if brace == "{"
                && let Some((name, after)) = rest.split_once('}')
            {
                found.push(name);
                rest = after;
            }
        }
        found.sort_unstable();
        found
    }

    /// Every language's file: each text once, or once more as a plural, each translation with the
    /// placeholders of its text, and each plural in all the language's forms.
    #[test]
    fn the_translations_fit_their_texts() {
        for language in LANGUAGES.iter().filter(|language| !language.is_english()) {
            let forms = (0..200).map(language.form).max().unwrap() + 1;
            let mut seen = std::collections::HashSet::new();
            let mut wrong = Vec::new();
            for entry in entries(language.po) {
                if entry.text.is_empty() {
                    continue;
                }
                if !seen.insert((entry.text, entry.context, entry.plural.is_some())) {
                    wrong.push(format!("twice: {:?}", entry.text));
                }
                if entry.translations.iter().all(|text| text.is_empty()) {
                    continue;
                }
                let Some(plural) = entry.plural else {
                    // Places may change the order, so `{}` and `{0}` both fit.
                    fn unplaced(text: &str) -> Vec<&str> {
                        let mut places = placeholders(text);
                        for place in &mut places {
                            if place.parse::<usize>().is_ok() {
                                *place = "";
                            }
                        }
                        places.sort_unstable();
                        places
                    }
                    if unplaced(entry.text) != unplaced(entry.translations[0]) {
                        wrong.push(format!("placeholders: {:?}", entry.text));
                    }
                    continue;
                };
                if entry.translations.len() != forms {
                    wrong.push(format!("forms: {:?}", entry.text));
                }
                let mut allowed = placeholders(entry.text);
                allowed.extend(placeholders(plural));
                for translation in &entry.translations {
                    if placeholders(translation)
                        .iter()
                        .any(|place| !allowed.contains(place) && place.parse::<usize>().is_err())
                    {
                        wrong.push(format!("placeholders: {:?}", entry.text));
                    }
                }
            }
            assert!(wrong.is_empty(), "lang/{}.po: {wrong:#?}", language.code);
        }
    }

    #[test]
    fn english_is_the_text_itself() {
        assert_eq!(tr!("Not in any file"), "Not in any file");
        assert_eq!(tr!("Kick {}", "Sam"), "Kick Sam");
        assert_eq!(trn!("{n} member", "{n} members", 1), "1 member");
        assert_eq!(trn!("{n} member", "{n} members", 4), "4 members");
    }
}
