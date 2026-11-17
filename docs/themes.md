# Themes

Settings → Appearance shows the window in one of the themes Yapfast comes with,
or one of the person's own. Each is a full set of the colours of `Theme` in
`yapfast_core::theme` (`crates/yapfast-core/src/theme.rs`), plus whether it's
dark, which sets the scrollbars' shade and the colours worked out from the
others. The core holds the built-in ones as `BUILT_IN` and reads custom
themes and hex colours; `crates/yapfast-iced/src/themes.rs` makes the chosen
one the kit's `Theme` (`crates/yapfast-iced-kit/src/theme.rs`), which every
widget paints with; see [the client's theme](client/window.md#theme). The
choice is kept in `layout.json` as `theme`, the theme's id, and applied
before the window's first frame, and again to a window made anew. With none
kept, the window shows Bonfire. An id Yapfast doesn't know, or a deleted
theme's, shows Yap. The window-control command `theme ID` chooses and keeps
one as Appearance does.

## Custom themes

New theme opens an editor on a copy of the theme shown; START FROM swaps it
for a copy of any other. The editor lists every colour by its name
(`Theme::NAMES`, with `-` for `_`), with a swatch and its hex, which takes
`#rgb`, `#rrggbb` or
`#rrggbbaa` and marks anything else. Each change shows at once, so the whole
window is the preview. Save keeps the theme and chooses it; Cancel, leaving
Appearance or closing settings shows the theme from before. A saved theme's
card has a pencil that opens it again to rename, change or delete it;
deleting the chosen one shows Yap. Closing the window to the
[tray](tray.md) closes Settings, so it drops the unsaved edit too.

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
whose id isn't of that form or repeats another's. The editor's fields exist
only while it's open; otherwise a custom theme costs its parsed colours and a
card.

| Theme | Id | Dark | Accent | From |
| --- | --- | --- | --- | --- |
| Yap | `yap` | yes | marigold | Yapfast's own |
| Bonfire | `bonfire` | yes | blurple | Discord's Dark theme, the default |
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
  tooltip light, since its text is the theme's text colour.

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

Settings → Appearance has a FONT section for drawing all of Yapfast in the
person's own font. Choose font files opens the system's file dialog, which
takes several files at once; the list shows each file by name with the faces
it gave ("Regular, Bold Italic") and a Remove button. Choosing again adds to
the files already loaded, and Reset to default removes them all and goes back
to DM Sans. The monospace code font and the emoji font are not changed.

The window draws every text in the family named DM Sans, so the files' faces
are put in the font database under that name, not the names the files carry
(the weights of one typeface often have a family name each), and DM Sans's own
faces leave it while they're in. cosmic-text then picks the face for a bold or
italic request through fontdb's query, from the weight and style each file
declares in its own tables, so a family given as one file per weight and a
variable font both work; a variable font is one face, drawn at its default
weight. A style the files don't have is synthesised from the nearest, as it is
for any font.

The formats are TrueType and OpenType (TTF, OTF), collections (TTC, which
register all of their faces) and WOFF. WOFF2 isn't supported, and says so in
the list. A file is told by its first four bytes, not its extension. A file
that isn't a font, or gives no face, is refused with a short reason and isn't
kept. TTF, OTF and TTC files are memory-mapped by fontdb, not read, so loading
costs no heap and the operating system pages in the glyphs that draw. A WOFF
file's tables are compressed, so it is unwrapped into a TrueType or OpenType
font in memory (`crates/yapfast-iced-kit/src/woff.rs`) and the compressed file
is released; that is the only copy a font makes. At most 64 files load.

`layout.json` keeps the files in `ui_font_files`, as absolute paths in the
order they were added. They are loaded at startup, before the window opens. A
path that is missing or isn't a usable font is skipped with a note under the
list, the rest are used, and when none loads DM Sans stays. A missing file
stays saved, in case its drive comes back; a file refused when chosen is not
saved.

Changing the files takes the earlier set's faces out of the font database and
puts the new set in, then moves the font system's version so iced shapes all
its text again (`fonts::shape_again`); the timeline's rows are measured again,
as when the message text size changes. The unloaded files' mappings are
released once the text engine drops the fonts it holds, which can be a moment
later.

Known limitation: the text keeps the sizes the window's design gives it, so a
font with large or small proportions reads larger or smaller than DM Sans
does, and the emoji font's vertical metrics are tuned to DM Sans, so emoji can
sit a little high or low against the text of a font with other proportions.
