//! The few texts the kit draws itself ("Reset", "Save Changes", "LIVE"), in
//! the window's language. The kit knows nothing of the catalog, so the
//! window gives it one lookup, once, at its start; until then, and for a
//! text the lookup has no translation of, the English reads.

use std::sync::OnceLock;

static LOOKUP: OnceLock<fn(&'static str) -> &'static str> = OnceLock::new();

/// Has the kit's texts translated by `lookup`, which answers a text in the
/// current language each time it's asked.
pub fn translate_with(lookup: fn(&'static str) -> &'static str) {
    let _ = LOOKUP.set(lookup);
}

/// `text` in the window's language.
pub(crate) fn say(text: &'static str) -> &'static str {
    LOOKUP.get().map_or(text, |lookup| lookup(text))
}

/// `text` in the window's language, its one `{}` filled with `value`.
pub(crate) fn say_with(text: &'static str, value: impl std::fmt::Display) -> String {
    say(text).replacen("{}", &value.to_string(), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_reads_in_english_till_the_window_translates() {
        assert_eq!(say("Reset"), "Reset");
        assert_eq!(say_with("Too long by {}", 3), "Too long by 3");
    }
}
