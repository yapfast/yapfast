//! The interface's language: the one chosen in the settings, or the system's.
//! `yapfast_lang` holds the translations; the window's `@tr` asks it through
//! Slint's translator. See `docs/translations.md`.

use std::borrow::Cow;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use yapfast_lang::{LANGUAGES, tr};

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

/// Puts the interface in a language, by its code, or in the system's for
/// none, and shows the choice in the settings. The window's own text follows
/// at once; text Rust made before stays until it's made again.
pub(super) fn show(window: &YapfastWindow, code: Option<&str>) {
    // Tests expect the code's own text whatever the machine's language.
    let system = || match cfg!(test) {
        true => String::new(),
        false => sys_locale::get_locale().unwrap_or_default(),
    };
    let chosen = yapfast_lang::choose(&code.map_or_else(system, str::to_owned));
    if let Some(context) = i_slint_core::SlintContext::current() {
        // English is the text itself: without a translator, nothing looks it up.
        let translator: Option<Box<dyn ::tr::Translator>> =
            (!yapfast_lang::chosen().is_english()).then(|| Box::new(Translations) as _);
        context.set_external_translator(translator);
    }
    // The build's channel was worded before the language was known.
    let channel = crate::build_info::current().channel().label();
    window
        .global::<yapfast_ui::BuildInfo>()
        .set_channel(yapfast_lang::text(channel).into());
    let names = std::iter::once(SharedString::from(tr!("Same as the system")))
        .chain(LANGUAGES.iter().map(|language| language.name.into()));
    window.set_languages(ModelRc::new(VecModel::from_iter(names)));
    let index = code.and_then(|_| {
        LANGUAGES
            .iter()
            .position(|language| language.code == chosen)
    });
    window.set_language(index.map_or(0, |index| index as i32 + 1));
}

/// The code of the language at a place in the settings' list, whose first
/// row follows the system.
fn code_at(index: i32) -> Option<String> {
    let index = usize::try_from(index).ok()?.checked_sub(1)?;
    Some(LANGUAGES.get(index)?.code.to_owned())
}

impl Adapter {
    /// Takes the language chosen in the settings, keeps it, and shows the
    /// window again in it.
    pub(super) fn choose_language(&mut self, window: &YapfastWindow, index: i32) {
        self.layout.language = code_at(index);
        show(window, self.layout.language.as_deref());
        self.layout.write(&self.root);
        self.refresh(window);
    }
}
