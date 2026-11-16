//! The interface's language: Russian until another is chosen, whatever the
//! system's (docs/translations.md). Text the screens mark with `tr!` follows
//! a change at the next view; text the projection made is made again, as
//! choosing forgets what every area showed.

use crate::app::App;

/// Chooses the language a layout names, or the default. Returns its code.
pub fn start(chosen: Option<&str>) -> &'static str {
    yapfast_lang::choose(chosen.unwrap_or(yapfast_lang::DEFAULT))
}

/// Switches to a language by code, as Appearance's list does, and keeps it.
pub fn choose(app: &mut App, code: &str) {
    let chosen = yapfast_lang::choose(code);
    app.layout.set("language", chosen);
    app.shown.forget();
    app.refresh();
}
