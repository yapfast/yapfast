//! The fonts the iced interface draws emoji and styled names with, and the
//! person's own font files: what they are, not how the text engine takes
//! them, which is `anomi_theme::fonts`'s. Nothing here knows iced, so the
//! crate builds beside it.
//!
//! - [`twemoji`]: Twemoji as the colour font the Slint build makes, made by
//!   `build.rs` from `vendor/twemoji/72x72` and kept in `OUT_DIR` until a
//!   picture changes; finding an emoji's glyph, and text with its emoji as
//!   glyphs (the `Twemoji` global).
//! - [`names`]: the eleven display-name faces of `ui/fonts`.
//! - [`han`]: which fonts draw Chinese, Japanese and Korean first, by the
//!   system's language.
//! - [`custom`]: the person's font files: copies, WOFF, what the list says.
//! - [`tables`]: the colour bitmap font builder Twemoji's font comes from.
//!
//! Task F03 (docs/iced/tasks/foundation.md).

pub mod custom;
pub mod han;
pub mod names;
pub mod tables;
pub mod twemoji;

/// The script a fallback list is for, as the text engine names it.
pub use unicode_script::Script;

/// Every script a character is in, each once: every script the text engine
/// can ask a fallback list for. It reads the first two planes, which hold a
/// character of every script; the others hold more Chinese characters (Han),
/// tags and selectors (Common, Inherited) and private use (no script). That's
/// 131 072 characters, 1.8 ms in a release build, where all seventeen planes
/// take 14 ms; a test reads them all to hold it to that.
pub fn scripts() -> Vec<Script> {
    use unicode_script::UnicodeScript as _;
    let mut seen = [false; 256];
    (0..0x2_0000)
        .filter_map(char::from_u32)
        .map(|letter| letter.script())
        .filter(|&script| !std::mem::replace(&mut seen[usize::from(script as u8)], true))
        .collect()
}

#[cfg(test)]
mod tests {
    use unicode_script::UnicodeScript as _;

    use super::{Script, scripts};

    #[test]
    fn the_first_two_planes_hold_every_script() {
        let found = scripts();
        let mut seen = [false; 256];
        let mut every: Vec<Script> = Vec::new();
        for letter in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
            let script = letter.script();
            if !std::mem::replace(&mut seen[usize::from(script as u8)], true) {
                every.push(script);
            }
        }
        assert_eq!(found, every);
        for script in [Script::Arabic, Script::Thai, Script::Adlam, Script::Common] {
            assert!(found.contains(&script), "{script:?}");
        }
    }
}
