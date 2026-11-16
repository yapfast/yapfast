# Themes

Settings → Appearance shows the window in one of the themes Yapfast comes with,
or one of the person's own. Each is a full set of the colours of the Slint
global `Theme` (`ui/theme.slint`), plus whether it's dark, which sets the
std-widgets' colour scheme and the scrollbars'. `src/ui/theme.rs` holds the built-in ones as
`BUILT_IN`; `ui/theme.slint` says what each colour paints. The choice
is kept in `layout.json` as `theme`, the theme's id, and applied before the
window's first frame, and again to a window made anew. An id Yapfast doesn't
know, or a deleted theme's, shows Yap.

## Custom themes

New theme opens an editor on a copy of the theme shown; START FROM swaps it
for a copy of any other. The editor lists every colour by its name in
`ui/theme.slint`, with a swatch and its hex, which takes `#rgb`, `#rrggbb` or
`#rrggbbaa` and marks anything else. Each change shows at once, so the whole
window is the preview. Save keeps the theme and chooses it; Cancel, leaving
Appearance or closing settings shows the theme from before. A saved theme's
card has a pencil that opens it again to rename, change or delete it;
deleting the chosen one shows Yap. A window rebuilt while the editor is open,
as closing to the [tray](tray.md) does, drops the unsaved edit.

`layout.json` keeps them in `custom_themes`, in the order Appearance shows
them after the built-in ones:

```json
{
  "id": "custom:1",
  "name": "Plum",
  "base": "yap",
  "dark": true,
  "colours": { "rail": "#111215", "chat": "#2a1a3a", "row-hover": "#1a1b20" }
}
```

`base` is the built-in theme the first copy came from (a copy of a custom
theme takes its base). Yapfast writes every colour, but a colour that's missing,
such as one added in a later version, or that isn't a colour, takes the
base's value, and a name Yapfast doesn't know is ignored, so themes survive new
colours. Ids are `custom:` and a number, and no built-in id has a colon, so
they can't collide; on reading, Yapfast gives a fresh one to any custom theme
whose id isn't of that form or repeats another's. The editor's rows are a
Slint model only while it's open; otherwise a custom theme costs its parsed
colours and a card.

| Theme | Id | Dark | Accent | From |
| --- | --- | --- | --- | --- |
| Yap | `yap` | yes | marigold | Yapfast's own, the default |
| Bonfire | `bonfire` | yes | blurple | Discord's Dark theme |
| Midnight | `midnight` | yes | blurple | Yapfast's own: near black, each surface a step lighter |
| Black | `black` | yes | blurple | Midnight on true black, for OLED screens |
| Catppuccin Mocha | `catppuccin-mocha` | yes | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Dracula | `dracula` | yes | purple | [Dracula](https://draculatheme.com/contribute) |
| Nord | `nord` | yes | frost nord8 | [Nord](https://www.nordtheme.com/docs/colors-and-palettes) |
| Gruvbox Dark | `gruvbox-dark` | yes | orange | [Gruvbox](https://github.com/morhetz/gruvbox) |
| Tokyo Night | `tokyo-night` | yes | blue | [Tokyo Night](https://github.com/tokyo-night/tokyo-night-vscode-theme), Night style |
| Catppuccin Latte | `catppuccin-latte` | no | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Rosé Pine Dawn | `rose-pine-dawn` | no | rose | [Rosé Pine](https://rosepinetheme.com/palette/) |
| Sakura | `sakura` | no | blossom pink | Yapfast's own |

The palettes of Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night and Rosé Pine
are published under the MIT licence; thanks to their authors. For Tokyo
Night, the steps between its grounds and text (bg_highlight, fg_gutter, dark3,
dark5) are named as folke's tokyonight.nvim names them. Yapfast takes their
colours, not their names for UI parts, so each mapping is Yapfast's:

- The ground steps go rail, sidebar, chat from the deepest to the one text
  sits on, with raised surfaces (popovers, reactions, stage tiles) and the
  composer one step off the chat. Where a palette has no shade below its
  background (Nord's nord0, Tokyo Night's bg_dark), the rail, panel and edge
  darken it, and hover and some row grounds are halfway mixes of two palette
  colours.
- Text, secondary, muted and faint take the palette's text ramp; faint keeps
  at least 2.5:1 against the sidebar, near Yap's 2.8:1.
- Content on the accent and on red (on-accent, on-danger) takes the palette's
  darkest or lightest colour, whichever reads; on pastel accents, as in
  Catppuccin and Tokyo Night, that's dark.
- Light themes wash hovers with a dark overlay (black, or the palette's text
  colour) instead of white, sweep skeletons with a white band, and draw the
  tooltip light, since its text is the theme's text colour. Their raised
  surfaces (popovers, dialogs, cards) are a step lighter than the chat, near
  white, as a light theme lifts things toward the light; Sakura's are a step
  pinker instead, its blossom on the paper. Their wells (fields, segmented
  controls) are a step darker than the chat and the settings pages, and
  their green is dark enough to read as 12px text on a light card.

Bonfire maps Discord's Dark theme from the CSS variables of its web client
(`.theme-dark` with `.theme-darker`, which is what its settings call Dark;
`.theme-dark` alone is Ash, `.theme-midnight` Onyx), read from the public
bundle: the rail and sidebar are `--background-base-lowest` (#121214), the chat
`--background-base-lower` (#1a1a1e), the account panel and composer
`--background-base-low` (#202024), raised surfaces and the tooltip
`--background-surface-high` (#242429), text `--text-default`, muted
`--text-muted`, links `--text-link`, the accent
`--control-primary-background-default` (blurple, #5865f2, hover #4452bb, white
on it), red `--control-critical-primary-background-default` (#d22d39), live
`--status-positive`, and hovers, selections, lines and secondary buttons the
`--interactive-background-*`, `--border-subtle` and
`--control-secondary-background-*` washes laid over their grounds. It carries
no Discord name or logo.

Discord role colours stay as servers set them, and some read faintly on a
light theme; Yapfast doesn't adjust them yet.

## Fonts

Settings → Appearance has a Font section for drawing the whole interface in
the person's own font. Add font files, at the end of its heading, opens a file
dialog that takes several files at once; it says Adding… and waits while
they're copied in. The files show in a rimmed card, one row each: a sample,
"Aa", in the file's first face, the name the file had, its faces ("Regular,
Bold Italic", or how many past four) and a button that takes it out. Adding
again adds to the files there, and Back to the bundled font, under the card,
takes them all out. The monospace code font and the emoji font aren't
changed, and the text keeps its size: the layouts are fitted to the size, not
to a font.

Every file is registered in one family of Yapfast's own naming, not the names
the files carry, because the weights of one typeface often have a family name
each. The text engine then picks the face for a bold or italic request from
the weight and italic each file declares in its own tables, so a family given
as one file per weight and a variable font both work. A style the files don't
have is synthesised from the nearest, as it is for any font. Files of the
same style share it: a letter the first file added lacks is drawn from the
next of that style that has it, before any system font, so a typeface given
as one file per script, as web fonts often are, draws as one.

Russian needs Cyrillic letters, which many Latin typefaces don't have, and
DM Sans doesn't either. A file whose styles get Cyrillic from no file loaded
says so in its row, after a sign in the theme's warning colour: "No Cyrillic:
Russian letters are drawn in another font", which is the system's. Its words
are in the secondary text colour, which every theme reads at that size, as the
warning colour isn't on a light theme.

The formats are TrueType and OpenType (TTF, OTF), collections (TTC, which
register all of their faces) and WOFF. WOFF2 isn't supported; the dialog
offers it, and choosing one says so. A file is told by its first four bytes,
not its extension. A file that isn't a font, gives no face or is larger than
64 MiB is refused, and the note under the card names it and says why until
the settings close; it isn't kept. At most 64 files load.
`scripts/ui-session do 'pick font PATH'` adds a file as the dialog would.

A file chosen is copied into the data folder's `fonts/`, named by the first
half of its SHA-256 and its kind (`….ttf`, `….otf`, `….ttc`), and drawn from
the copy. TrueType, OpenType and collection files are streamed into it; a
WOFF file's tables are compressed, so it's unwrapped into a TrueType or
OpenType font as it's copied (`src/woff.rs`), and the copy is the plain font.
The copying runs off the window's thread. The copy is memory-mapped, not read:
loading costs no heap, and the operating system pages in the glyphs that
draw. Mapping the person's own file would crash Yapfast with SIGBUS the
moment a font manager or a sync rewrote it in place, or its drive went away;
a copy is written once, to a temporary file renamed into place, and
afterwards only deleted, which leaves a mapping whole. The same font chosen
twice, under any name, has one copy. Taking a file out, or going back to the
bundled font, deletes the copies no longer chosen, and the folder with the
last one; Windows won't delete a file still mapped, and a later change does.

`layout.json` keeps the files in `ui_fonts`, each by the name it had and its
copy's name, in the order they were added, omitted while there are none.
They're registered at startup, from the copies, before the window's first
frame. A copy that has gone shows in its row as missing until taken out; the
rest are used, and when none loads the note says the interface keeps its own
font.

Changing the files unregisters the faces of the earlier set and registers the
new one under a new family name (`Yapfast Custom Font` and a number). The
window's font family therefore changes with every load, which is what makes
every text on screen be measured again; the text engine caches a measured
size until a property it read changes, and a new face in a family of the same
name would not have changed one. A name isn't used twice, since Slint keeps
the font a name resolved to until its next frame; each change leaves an empty
family name of a few dozen bytes in the font collection, which can't forget
one. An unregistered copy stays mapped while the Skia renderer's cache of the
last 64 typefaces it drew with holds it, so a few earlier sets can stay
mapped, deleted, until newer faces push them out or Yapfast quits. Like the
set in use, they're pages of a file, which the system can drop, not heap.

Known limitation: the custom emoji font's vertical metrics are tuned to
Atkinson Hyperlegible Next, so emoji can sit a little high or low against the
text of a font with other proportions.
