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

No proprietary Discord or Windows font files are bundled. The earlier presentation
mode retains Atkinson Hyperlegible Next and its metrics. DM Sans is an
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
