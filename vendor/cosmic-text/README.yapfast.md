# Yapfast's cosmic-text

This is cosmic-text 0.15.0 from crates.io under its original MIT or
Apache-2.0 licence (`LICENSE-MIT`, `LICENSE-APACHE`), without its tests,
benchmarks, sample fonts and screenshots. iced shapes the iced client's text
with it; the root `Cargo.toml` patches it in through `[patch.crates-io]`.

Changes, marked `Yapfast port` in the source:

1. `src/font/fallback/mod.rs`, `src/shape.rs`: `Fallback::missing`, a hook
   called with each cluster of a shaped run that the default, script and
   common fallback fonts have no glyph for. The application names the
   families it chose for the cluster's characters, which are tried next,
   before the rest of the database, and may look for a font for the others.
   Stock, cosmic-text finds fallback fonts only per script, among the fonts
   already loaded, so a character of the Common script (𝔟, ⓒ, ⠃) that none
   of them has drew as a box, and one that some loaded font happened to have
   took that font, whatever it was. The iced client asks the system for a
   font per character (`crates/yapfast-iced-kit/src/fonts.rs`).
2. `src/font/fallback/mod.rs`: the default family, and the script and
   common fallback families, match at the face nearest the text's weight.
   Stock, they matched only a face of exactly that weight, so semibold text
   skipped DM Sans (one variable face, weight 400) and Twemoji as named
   fallbacks and took whichever loaded face came first, such as a Chinese
   font's plain ❤ before Twemoji's.
3. `src/shape.rs`: `set_family_alias`, one family name standing for another
   while shaping. The iced client makes "DM Sans" stand for a theme's body
   face, as Slint's `default-font-family` follows the theme
   (`crates/yapfast-iced-kit/src/fonts.rs`, `set_default_family`).
