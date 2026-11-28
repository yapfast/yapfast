//! Source strings and GNU gettext catalogs for the window.

use std::collections::HashMap;

use polib::po_file;
use unic_langid::LanguageIdentifier;

/// Translate a literal using the window's selected language.
macro_rules! tr {
    ($locale:expr, $source:literal $(,)?) => {
        ($locale).text($source)
    };
}
pub(crate) use tr;

/// Mark a source string stored in a constant for gettext extraction.
macro_rules! tr_mark {
    ($source:literal) => {
        $source
    };
}
pub(crate) use tr_mark;

pub struct Language {
    pub tag: &'static str,
    pub name: &'static str,
    source: Option<&'static str>,
}

macro_rules! language {
    ($tag:literal, $name:literal) => {
        Language {
            tag: $tag,
            name: $name,
            source: Some(include_str!(concat!("../assets/i18n/", $tag, ".po"))),
        }
    };
}

const ENGLISH: Language = Language {
    tag: "en",
    name: "English",
    source: None,
};
pub static LANGUAGES: &[Language] = &[
    ENGLISH,
    language!("ru", "Русский"),
    language!("de", "Deutsch"),
];

/// The dropdown's first choice follows the system.
pub fn choice(preferred: Option<&str>) -> usize {
    preferred.map_or(0, |tag| {
        let language = explicit(tag);
        LANGUAGES
            .iter()
            .position(|entry| entry.tag == language.tag)
            .unwrap_or_default()
            + 1
    })
}

type Catalog = HashMap<Box<str>, Box<str>>;

pub struct I18n {
    tag: &'static str,
    translations: Catalog,
}

impl I18n {
    pub fn new(preferred: Option<&str>) -> Self {
        let mut i18n = Self {
            tag: ENGLISH.tag,
            translations: Catalog::new(),
        };
        i18n.select(preferred);
        i18n
    }

    pub fn select(&mut self, preferred: Option<&str>) {
        let language =
            preferred.map_or_else(|| system_language(sys_locale::get_locales()), explicit);
        if language.tag == self.tag {
            return;
        }
        self.tag = language.tag;
        self.translations = language.source.map_or_else(Catalog::new, |source| {
            catalog(source).unwrap_or_else(|error| {
                tracing::warn!(locale = language.tag, %error, "could not load interface language");
                Catalog::new()
            })
        });
    }

    pub fn text<'a>(&'a self, source: &'a str) -> &'a str {
        self.translations.get(source).map_or(source, AsRef::as_ref)
    }
}

fn explicit(tag: &str) -> &'static Language {
    supported(tag).unwrap_or(&ENGLISH)
}

fn system_language(system: impl IntoIterator<Item = impl AsRef<str>>) -> &'static Language {
    system
        .into_iter()
        .find_map(|tag| supported(tag.as_ref()))
        .unwrap_or(&ENGLISH)
}

fn supported(tag: &str) -> Option<&'static Language> {
    if let Some(exact) = LANGUAGES.iter().find(|language| language.tag == tag) {
        return Some(exact);
    }
    let locale: LanguageIdentifier = tag.parse().ok()?;
    LANGUAGES
        .iter()
        .find(|language| locale == language.tag)
        .or_else(|| {
            LANGUAGES
                .iter()
                .find(|language| locale.language.as_str() == language.tag)
        })
}

fn catalog(source: &str) -> Result<Catalog, po_file::POParseError> {
    let po = po_file::parse_from_reader(source.as_bytes())?;
    Ok(po
        .messages()
        .filter(|message| message.msgctxt().is_none() && !message.is_fuzzy())
        .filter_map(|message| {
            let translation = message.msgstr().ok().filter(|value| !value.is_empty())?;
            Some((message.msgid().into(), translation.into()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_language_follows_supported_system_preferences_by_default() {
        assert_eq!(system_language(["de-AT", "en-US"]).tag, "de");
        assert_eq!(system_language(["es-ES", "ru-RU"]).tag, "ru");
        assert_eq!(system_language(["invalid!", "es-ES"]).tag, "en");
        assert_eq!(system_language([] as [&str; 0]).tag, "en");
        assert_eq!(explicit("ru").tag, "ru");
        assert_eq!(explicit("es-ES").tag, "en");
    }

    #[test]
    fn interface_catalogs_parse_and_return_translations() {
        for language in LANGUAGES {
            if let Some(source) = language.source {
                let translations = catalog(source).unwrap();
                assert!(
                    translations.contains_key("Language & Time"),
                    "{}",
                    language.tag
                );
            }
        }
    }

    #[test]
    fn translated_catalogs_cover_audit_filters_and_member_sort_options() {
        use yapfast_core::application::{AUDIT_ACTIONS, MEMBER_SORTS};

        for language in LANGUAGES {
            let Some(source) = language.source else {
                continue;
            };
            let translations = catalog(source).unwrap();
            for label in AUDIT_ACTIONS
                .iter()
                .map(|(_, label)| *label)
                .chain(MEMBER_SORTS)
            {
                assert!(
                    translations.contains_key(label),
                    "{}: {label}",
                    language.tag
                );
            }
        }
    }

    #[test]
    fn translated_catalogs_cover_role_and_channel_permissions() {
        use yapfast_core::{
            application::{ROLE_PERMISSIONS, channel_groups, channel_permission},
            chat::ChannelKind,
        };

        for language in LANGUAGES {
            let Some(source) = language.source else {
                continue;
            };
            let translations = catalog(source).unwrap();
            for group in ROLE_PERMISSIONS {
                let labels = std::iter::once(group.heading).chain(
                    group.permissions.iter().flat_map(|info| {
                        [Some(info.title), Some(info.description), info.warning]
                            .into_iter()
                            .flatten()
                    }),
                );
                for label in labels {
                    assert!(
                        translations.contains_key(label),
                        "{}: {label}",
                        language.tag
                    );
                }
            }
            for kind in [ChannelKind::Text, ChannelKind::Voice, ChannelKind::Category] {
                for &(heading, bits) in channel_groups(kind) {
                    assert!(
                        translations.contains_key(heading),
                        "{}: {heading}",
                        language.tag
                    );
                    for &bit in bits {
                        for everyone in [true, false] {
                            let (title, description) = channel_permission(bit, kind, everyone);
                            for label in [title, description] {
                                assert!(
                                    translations.contains_key(label),
                                    "{}: {label}",
                                    language.tag
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn interface_messages_fall_back_for_missing_empty_and_fuzzy_translations() {
        let source = r#"msgid ""
msgstr "Content-Type: text/plain; charset=UTF-8\n"

msgid "Known"
msgstr ""
"Über"
"setzt"

msgid "Empty"
msgstr ""

#, fuzzy
msgid "Draft"
msgstr "Entwurf"
"#;
        let i18n = I18n {
            tag: "de",
            translations: catalog(source).unwrap(),
        };
        assert_eq!(i18n.text("Known"), "Übersetzt");
        assert_eq!(i18n.text("Missing"), "Missing");
        assert_eq!(i18n.text("Empty"), "Empty");
        assert_eq!(i18n.text("Draft"), "Draft");
    }
}
