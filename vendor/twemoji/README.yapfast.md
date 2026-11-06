# Twemoji for Yapfast

This is `assets/72x72/` from Twemoji, the emoji artwork Discord's client draws,
as the Discord organisation keeps it: https://github.com/discord/twemoji, at
commit `187e1b77ee80532eb584bcf62a2266942fa39cf9` (Twemoji v17.0, Unicode 17.0).

Yapfast draws emoji from these pictures instead of the system emoji font, so
they look as Discord's do. `build.rs` makes them one font the program carries,
and `src/twemoji.rs` looks a picture up in it by its file name: the emoji's code
points in lower-case hexadecimal, joined by `-`.

## Licence

The graphics are licensed under **CC-BY 4.0**, as `LICENSE-GRAPHICS` has it
(https://creativecommons.org/licenses/by/4.0/); the upstream `LICENSE` is the
MIT licence of its code, none of which is here. Attribution, as CC-BY 4.0 asks:

- **Title:** Twemoji
- **Authors:** the Twemoji contributors, and the designers who have drawn for it
  since, listed in `README.md` above; the artwork began as Twitter's Twemoji
- **Source:** https://github.com/discord/twemoji at the commit above,
  `assets/72x72/`
- **Licence:** CC-BY 4.0, https://creativecommons.org/licenses/by/4.0/
- **Changes:** the files here are copied as published. When the program is
  built, each picture gets a transparent margin 4 pixels wide, and the program
  holds them in a font instead of shipping them as files.

`scripts/release_package.py` puts this licence and attribution beside the other
dependencies' in each package's `licenses/` folder and `THIRD-PARTY.json`, under
the revision `REVISION` names.

## Updating

Take the new commit's `assets/72x72/`, `LICENSE-GRAPHICS`, `LICENSE` and
`README.md` over these, and update the commit above. Nothing else here is
Yapfast's, so there is nothing to reapply.

A code point these pictures lack stays a system-font glyph (`src/emoji_glyphs.rs`
draws it once, as it did for every emoji), so a new Twemoji release adds look
rather than fixing anything.
