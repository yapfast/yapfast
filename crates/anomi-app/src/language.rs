//! The interface's language: Russian until another is chosen, whatever the
//! system's (docs/translations.md, `src/ui/language.rs`). Choosing one in
//! Appearance puts the whole window in it at once: text the screens mark
//! with `tr!` follows at the next view, and text the projection made is made
//! again, as choosing forgets what every area showed; the tray's menu is
//! worded again too.

use yapfast_lang::{DEFAULT, LANGUAGES, Language};

use crate::app::App;

/// Chooses the language a layout names, or the default. Returns its code.
pub fn start(chosen: Option<&str>) -> &'static str {
    yapfast_lang::choose(chosen.unwrap_or(DEFAULT))
}

/// Appearance's list of languages: the default first, then the rest.
pub fn listed() -> impl Iterator<Item = &'static Language> {
    let first = LANGUAGES.iter().filter(|language| language.code == DEFAULT);
    first.chain(LANGUAGES.iter().filter(|language| language.code != DEFAULT))
}

/// The place in [`listed`] of the language a layout names, or the default's;
/// a code no language has is English, as `yapfast_lang::choose` takes it.
pub fn row_of(chosen: Option<&str>) -> usize {
    let locale = chosen.unwrap_or(DEFAULT);
    let code = locale
        .split(['_', '-', '.', '@'])
        .next()
        .unwrap_or_default();
    let at = |code: &str| listed().position(|language| language.code.eq_ignore_ascii_case(code));
    at(code).or_else(|| at(LANGUAGES[0].code)).unwrap_or(0)
}

/// Switches to a language by code, as Appearance's list does, and keeps it.
pub fn choose(app: &mut App, code: &str) {
    let chosen = yapfast_lang::choose(code);
    app.layout
        .edit(|saved| saved.language = Some(chosen.to_owned()));
    app.shown.forget();
    app.refresh();
    crate::platform::tray::reword(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_comes_first_and_a_layout_keeps_its_choice() {
        let names: Vec<_> = listed().map(|language| language.name).collect();
        assert_eq!(names, ["Русский", "English"]);
        assert_eq!(
            (row_of(None), row_of(Some("ru")), row_of(Some("en"))),
            (0, 0, 1)
        );
    }
}
