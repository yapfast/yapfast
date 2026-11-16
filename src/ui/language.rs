//! The interface's language: the one chosen in the settings, or Russian, this
//! fork's own, until one is. `yapfast_lang` holds the translations; the
//! window's `@tr` asks it through Slint's translator. See
//! `docs/translations.md`.

use std::borrow::Cow;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use yapfast_lang::{DEFAULT, LANGUAGES, Language};

use super::{Adapter, YapfastWindow};

/// Gives the window's `@tr` the chosen language's text.
struct Translations;

impl ::tr::Translator for Translations {
    fn translate<'a>(&'a self, text: &'a str, context: Option<&'a str>) -> Cow<'a, str> {
        Cow::Borrowed(yapfast_lang::lookup(text, context).unwrap_or(text))
    }

    fn ntranslate<'a>(
        &'a self,
        count: u64,
        one: &'a str,
        other: &'a str,
        _context: Option<&'a str>,
    ) -> Cow<'a, str> {
        let english = if count == 1 { one } else { other };
        Cow::Borrowed(yapfast_lang::lookup_plural(count, one).unwrap_or(english))
    }
}

/// The settings' list of languages: the default first, then the rest.
fn listed() -> impl Iterator<Item = &'static Language> {
    let first = LANGUAGES.iter().filter(|language| language.code == DEFAULT);
    first.chain(LANGUAGES.iter().filter(|language| language.code != DEFAULT))
}

/// Puts the interface in a language, by its code, or in the default for none,
/// and shows the choice in the settings. The window's own text follows at
/// once; text Rust made before stays until it's made again.
pub(super) fn show(window: &YapfastWindow, code: Option<&str>) {
    // Tests expect the code's own text, which no language gives.
    let unchosen = if cfg!(test) { "" } else { DEFAULT };
    let chosen = yapfast_lang::choose(code.unwrap_or(unchosen));
    if let Some(context) = i_slint_core::SlintContext::current() {
        // Without a language nothing is looked up, so no translator.
        let translator: Option<Box<dyn ::tr::Translator>> = yapfast_lang::translating()
            .is_some()
            .then(|| Box::new(Translations) as _);
        context.set_external_translator(translator);
    }
    // The build's channel was worded before the language was known.
    let channel = crate::build_info::current().channel().label();
    window
        .global::<yapfast_ui::BuildInfo>()
        .set_channel(yapfast_lang::text(channel).into());
    let names = listed().map(|language| SharedString::from(language.name));
    window.set_languages(ModelRc::new(VecModel::from_iter(names)));
    window.set_language(row_of(if code.is_some() { chosen } else { DEFAULT }));
}

/// The place in the settings' list of the language with a code.
fn row_of(code: &str) -> i32 {
    listed()
        .position(|language| language.code == code)
        .map_or(0, |index| index as i32)
}

/// The code of the language at a place in the settings' list.
fn code_at(index: i32) -> Option<String> {
    let index = usize::try_from(index).ok()?;
    Some(listed().nth(index)?.code.to_owned())
}

impl Adapter {
    /// Takes the language chosen in the settings, keeps it, and shows the
    /// window again in it.
    pub(super) fn choose_language(&mut self, window: &YapfastWindow, index: i32) {
        self.layout.language = code_at(index);
        show(window, self.layout.language.as_deref());
        // The font files' rows are Rust's text.
        self.fonts.show(window);
        if let Some(tray) = &self.tray {
            tray.reword();
        }
        self.layout.write(&self.root);
        self.refresh(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_puts_russian_first_and_each_row_names_its_code() {
        let names: Vec<_> = listed().map(|language| language.name).collect();
        assert_eq!(names, ["Русский", "English"]);
        assert_eq!(code_at(0).as_deref(), Some("ru"));
        assert_eq!(code_at(1).as_deref(), Some("en"));
        assert_eq!(code_at(2), None);
        assert_eq!(code_at(-1), None);
        // A layout.json from before keeps its choice.
        assert_eq!((row_of("ru"), row_of("en")), (0, 1));
    }

    /// Choosing a language here would change the text of the tests running
    /// beside it, so only no choice is shown.
    #[test]
    fn the_settings_show_russian_until_another_is_chosen() {
        use slint::Model;
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        show(&window, None);
        assert_eq!(window.get_language(), 0);
        assert_eq!(window.get_languages().row_count(), 2);
        assert!(
            yapfast_lang::translating().is_none(),
            "tests keep the code's own text"
        );
    }
}
