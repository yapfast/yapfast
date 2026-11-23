# UI fonts

The window's typeface is DM Sans, an open-source alternative to Discord's
custom gg sans, not an exact reproduction of it. The unmodified variable TTF
files `DMSans[opsz,wght].ttf` and `DMSans-Italic[opsz,wght].ttf` come from
[Google Fonts](https://github.com/google/fonts/tree/5b35b7208dd4100571326fdf37f030b32a524232/ofl/dmsans)
at commit `5b35b7208dd4100571326fdf37f030b32a524232`. They provide upright
and italic faces with variable weights and optical sizes, and
`src/fonts.rs` embeds them in the binary. The original copyright and license
are preserved in [LICENSE-OFL-DMSans.txt](LICENSE-OFL-DMSans.txt), the SIL Open
Font License 1.1. Binary packages include the license and this provenance note
in `licenses/fonts/`.

No proprietary font files are bundled: Discord's gg sans and Windows' fonts aren't
here, and the display name fonts below are Discord's copies of fonts under the SIL
Open Font License.

## Display name fonts

`DisplayName-*` are the fonts Discord's client draws display name styles
in, all eleven. Nine are Sakura (Cherry Bomb One), Jellybean (Chicle),
Modern (MuseoModerno), 8Bit (Pixelify Sans), Tempo (Zilla Slab), Monkey Bars
(Playpen Sans), Mainframe (Orbitron), Headbang (New Rocker) and Journal
(Kalam). Each is a Google Fonts family licensed
under the SIL Open Font License 1.1. The files are Discord's own copies, from
the `@font-face` rules of its web client's stylesheet
(`https://discord.com/assets/<hash>.woff2`), whose name tables carry the
copyright lines in [LICENSE-OFL-DisplayNames.txt](LICENSE-OFL-DisplayNames.txt)
and link the OFL; Discord renamed those whose licence reserves their name
(Chicle, Orbitron, New Rocker) and subset them to Latin. They were converted
from WOFF2 to TrueType here, and Tempo, Monkey Bars, Headbang and Journal
subset further to Basic Latin, Latin-1, Latin Extended-A and common
punctuation, which keeps them to 40 to 100 KB each (Monkey Bars, 180 KB). A
name outside that range falls back to the usual font, as it does in Discord's
client. `src/fonts.rs` embeds them in the binary.

The other two come from their designers' releases, not Discord's copies, and
keep their own family names:

- Medieval is Max Lillo's Néo-castel, `DisplayName-Medieval.otf` (17,064
  bytes). It is the WOFF2 `resources/fonts/Neo-castel/webfont/Neo-castel.woff2`
  of [zine_studio](https://hunterirving.com/git/zine_studio) (SHA-256
  `d4b4484ded262cbccd93eb6aa23e50253ac8695cbb3e4dc116cb92d6a3d84a1d`),
  unwrapped to OpenType with fontTools: a change of format only, its CFF
  outlines, glyphs and name table as they were, which is why it is an `.otf`
  and not a TrueType file. Its licence, the `Licence.txt` beside it there, is
  unchanged in [LICENSE-OIFL-Medieval.txt](LICENSE-OIFL-Medieval.txt): the
  OIFL, Max Lillo's French inclusive-language text of the SIL Open Font
  License 1.1, "Copyright (c) 2023 Max Lillo", whose Reserved Font Name is
  Ébullition, not Néo-castel. The family is "Néo-castel", drawn at 400 as
  Discord's client does. It has no `^` or `~`.
- Vampyre is Sinistre by Jules Durand and Collletttivo,
  `DisplayName-Vampyre.ttf` (67,616 bytes). It is a static cut of
  `fonts/SinistreVF.ttf` from
  [collletttivo/sinistre](https://github.com/collletttivo/sinistre) at commit
  `fbf85f1130670ff2e1b944a17ec13872996b4305`, made with fontTools at weight
  548 (between upstream's Regular, 400, and Bold, 673), where the outlines and
  advances of Discord's Vampyre match it best, and subset to the same Latin
  ranges as above; the upstream static Regular is 191 KB and lighter than
  Discord draws it. Its licence, upstream's `LICENSE.txt`, is unchanged in
  [LICENSE-OFL-Vampyre.txt](LICENSE-OFL-Vampyre.txt): the SIL Open Font
  License 1.1, "Copyright © 2023, Collletttivo". It reserves no font name,
  so the modified cut keeps the family name "Sinistre", at weight 548.

A name styled in a script one of these lacks shows in the usual font. Binary
packages include these licences and notes in `licenses/fonts/`.

## Theme fonts

`AtkinsonHyperlegibleNext-Regular.ttf` is Atkinson Hyperlegible Next by the
Braille Institute, one of the faces the theme creator offers, beside the
display name fonts above. It is the static Regular, unmodified, from the
[googlefonts/atkinson-hyperlegible-next](https://github.com/googlefonts/atkinson-hyperlegible-next)
repository at commit `7925f50f649b3813257faf2f4c0b381011f434f1` (version
2.001). Its licence is the SIL Open Font License 1.1, unchanged in
[LICENSE-OFL-Atkinson.txt](LICENSE-OFL-Atkinson.txt). The theme creator embeds
it in the binary and writes it into a theme that uses it.
