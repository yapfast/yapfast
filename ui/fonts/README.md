# UI fonts

The default presentation uses DM Sans, an open-source
alternative to Discord's custom gg sans. The unmodified variable TTF files
`DMSans[opsz,wght].ttf` and `DMSans-Italic[opsz,wght].ttf` come from
[Google Fonts](https://github.com/google/fonts/tree/5b35b7208dd4100571326fdf37f030b32a524232/ofl/dmsans)
at commit `5b35b7208dd4100571326fdf37f030b32a524232`. They provide upright
and italic faces with variable weights and optical sizes, and are embedded
by Slint. The original copyright and license are preserved in
[LICENSE-OFL-DMSans.txt](LICENSE-OFL-DMSans.txt), the SIL Open Font License 1.1.
Binary packages include the license and this provenance note in `licenses/fonts/`.

No proprietary font files are bundled: Discord's gg sans and Windows' fonts aren't
here, and the display name fonts below are Discord's copies of fonts under the SIL
Open Font License. The earlier presentation mode retains Atkinson Hyperlegible Next and its metrics. DM Sans is an
alternative, not an exact reproduction of gg sans.

Atkinson Hyperlegible Next by the Braille Institute, the UI's typeface: the
static TTF styles Regular, Italic, Medium, Bold, Bold Italic and ExtraBold,
copied unmodified from the googlefonts/atkinson-hyperlegible-next repository at
commit 7925f50f649b3813257faf2f4c0b381011f434f1 (version 2.001, through
nixpkgs' `atkinson-hyperlegible-next`) on 2026-10-07. They're embedded in the
Yapfast binary by the Slint compiler. `LICENSE-OFL.txt` is their SIL Open Font
License 1.1, which permits embedding, modifying and redistributing them,
including in a public repository.

Satoshi by Indian Type Foundry was the second choice. Its ITF Free Font License
2.0 forbids redistributing the files through a public repository, so they
aren't here. To try it, download the static TTF styles Regular, Italic, Medium,
Bold, Bold Italic and Black from Fontshare
(https://www.fontshare.com/fonts/satoshi) into this folder, and follow the
comment at the top of `ui/yapfast.slint`.

## Display name fonts

`DisplayName-*.ttf` are the fonts Discord's client draws display name styles
in, for nine of its eleven: Sakura (Cherry Bomb One), Jellybean (Chicle),
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
client. They are embedded in the binary by Slint.

Two are not bundled, because their licence couldn't be confirmed from the
designer: Medieval (Discord's name for Max Lillo's Néo-Castel) and Vampyre
(Jules Durand's Sinistre, released by Collletttivo). Their name tables link
the OFL, but that is Discord's say-so. A name styled with either shows in the
usual font. Binary packages include these notes in `licenses/fonts/`.
