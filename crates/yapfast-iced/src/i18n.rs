//! Interface messages shipped with the window.

use std::borrow::Cow;

use anyhow::{Context, Result, anyhow};
use fluent_bundle::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

pub struct Language {
    pub tag: &'static str,
    pub name: &'static str,
    source: &'static str,
}

macro_rules! language {
    ($tag:literal, $name:literal) => {
        Language {
            tag: $tag,
            name: $name,
            source: include_str!(concat!("../assets/locales/", $tag, "/settings.ftl")),
        }
    };
}

const ENGLISH: Language = language!("en", "English");
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

macro_rules! keys {
    ($($key:ident => $id:literal),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub enum Key {
            $($key),+
        }

        impl Key {
            #[cfg(test)]
            const ALL: &[Self] = &[$(Self::$key),+];

            pub fn id(self) -> &'static str {
                match self {
                    $(Self::$key => $id),+
                }
            }
        }
    };
}

keys! {
    SettingsSearch => "settings-search",
    SettingsClose => "settings-close",
    SettingsEditProfile => "settings-edit-profile",
    GroupAccount => "settings-group-account",
    GroupExperience => "settings-group-experience",
    GroupApp => "settings-group-app",
    Account => "settings-account",
    Voice => "settings-voice",
    Messages => "settings-messages",
    Accessibility => "settings-accessibility",
    Notifications => "settings-notifications",
    Discord => "settings-discord",
    Sounds => "settings-sounds",
    Advanced => "settings-advanced",
    LanguageTime => "settings-language-time",
    Appearance => "settings-appearance",
    Themes => "settings-themes",
    Games => "settings-games",
    Keybinds => "settings-keybinds",
    About => "settings-about",
    Language => "language-time-language",
    SystemLanguage => "language-time-system-language",
    TimeFormat => "language-time-format",
    TimeAuto => "language-time-auto",
    TimeHour12 => "language-time-12-hour",
    TimeHour24 => "language-time-24-hour",
    OpenOnStartup => "advanced-open-on-startup",
    StartMinimized => "advanced-start-minimized",
    StartMinimizedDescription => "advanced-start-minimized-description",
    DiscordLinks => "advanced-discord-links",
    DiscordLinksDescription => "advanced-discord-links-description",
    CloseToTray => "advanced-close-to-tray",
    CloseToTrayDescription => "advanced-close-to-tray-description",
    NoTrayDescription => "advanced-no-tray-description",
    MultipleWindows => "advanced-multiple-windows",
    MultipleWindowsDescription => "advanced-multiple-windows-description",
    GpuRendering => "advanced-gpu-rendering",
    GpuRenderingDescription => "advanced-gpu-rendering-description",
    GpuEncoding => "advanced-gpu-encoding",
    GpuEncodingDescription => "advanced-gpu-encoding-description",
    DeveloperMode => "advanced-developer-mode",
    DeveloperModeDescription => "advanced-developer-mode-description",
    IconCredits => "advanced-icon-credits",
    RestartForGpu => "advanced-restart-for-gpu",
    RestartForCpu => "advanced-restart-for-cpu",
    Restart => "advanced-restart",
}

type Catalog = FluentBundle<FluentResource>;

pub struct I18n {
    english: Catalog,
    selected: Option<Catalog>,
}

impl I18n {
    pub fn new(preferred: Option<&str>) -> Self {
        let mut i18n = Self {
            english: load(&ENGLISH).expect("valid embedded English interface messages"),
            selected: None,
        };
        i18n.select(preferred);
        i18n
    }

    pub fn select(&mut self, preferred: Option<&str>) {
        let language =
            preferred.map_or_else(|| system_language(sys_locale::get_locales()), explicit);
        if self.selected.as_ref().unwrap_or(&self.english).locales[0] == language.tag {
            return;
        }
        self.selected = if language.tag == ENGLISH.tag {
            None
        } else {
            match load(language) {
                Ok(catalog) => Some(catalog),
                Err(error) => {
                    tracing::warn!(locale = language.tag, %error, "could not load interface language");
                    None
                }
            }
        };
    }

    pub fn text(&self, key: Key) -> Cow<'_, str> {
        let id = key.id();
        self.selected
            .as_ref()
            .and_then(|catalog| message(catalog, id))
            .or_else(|| message(&self.english, id))
            .unwrap_or(Cow::Borrowed(id))
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

fn load(language: &Language) -> Result<Catalog> {
    let mut catalog = Catalog::new(vec![language.tag.parse()?]);
    let resource = FluentResource::try_new(language.source.into())
        .map_err(|(_, errors)| anyhow!("{errors:?}"))
        .with_context(|| format!("parsing the {} interface catalog", language.tag))?;
    catalog
        .add_resource(resource)
        .map_err(|errors| anyhow!("{errors:?}"))
        .with_context(|| format!("loading the {} interface catalog", language.tag))?;
    Ok(catalog)
}

fn message<'a>(catalog: &'a Catalog, id: &str) -> Option<Cow<'a, str>> {
    let pattern = catalog.get_message(id)?.value()?;
    let mut errors = Vec::new();
    let value = catalog.format_pattern(pattern, None, &mut errors);
    if !errors.is_empty() {
        tracing::warn!(id, ?errors, "could not format interface message");
        return None;
    }
    Some(value)
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
    fn interface_catalogs_parse_and_messages_format() {
        for language in LANGUAGES {
            let catalog = load(language).unwrap();
            for key in Key::ALL {
                let id = key.id();
                if language.tag == ENGLISH.tag || catalog.has_message(id) {
                    assert!(message(&catalog, id).is_some(), "{}: {id}", language.tag);
                }
            }
        }
    }

    #[test]
    fn missing_interface_messages_fall_back_to_english() {
        let mut i18n = I18n::new(Some("en"));
        i18n.selected = Some(Catalog::new(vec!["de".parse().unwrap()]));
        assert_eq!(i18n.text(Key::Advanced), "Advanced");
    }
}
