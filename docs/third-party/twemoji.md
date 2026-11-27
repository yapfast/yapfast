# Twemoji for Yapfast

This is `assets/72x72/` from Twemoji, the emoji artwork Discord's client draws,
as the Discord organisation keeps it: https://github.com/discord/twemoji, at
commit `187e1b77ee80532eb584bcf62a2266942fa39cf9` (Twemoji v17.0, Unicode 17.0).

Yapfast draws emoji from these pictures instead of the system emoji font, so
they look as Discord's do. `crates/yapfast-iced/build.rs` makes them one font
the program carries, each picture the glyph of the emoji its file name spells:
the emoji's code points in lower-case hexadecimal, joined by `-`.

## Licence

The graphics are licensed under **CC-BY 4.0**. The full text is in
`assets/licenses/twemoji/LICENSE-GRAPHICS` and ships in releases as
`licenses/twemoji-LICENSE-GRAPHICS`.

- **Title:** Twemoji
- **Authors:** Twitter and the Twemoji contributors and designers listed in the
  [upstream credits](https://github.com/discord/twemoji/blob/187e1b77ee80532eb584bcf62a2266942fa39cf9/README.md#committers-and-contributors)
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

Copy the new revision's `assets/72x72/` into `assets/twemoji/` and
`LICENSE-GRAPHICS` into `assets/licenses/twemoji/`. Update
`assets/twemoji/REVISION` and the revision and credits link on this page.

A code point these pictures lack stays a system-font glyph, so a new Twemoji
release adds look rather than fixing anything.
