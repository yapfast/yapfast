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

## Sharing themes

Import, beside New theme, opens a file picker and keeps the theme in the file
among the person's own, then chooses it, as Save does. Export, in the editor
beside Save and Cancel, saves the theme as the editor shows it to a file, so
a new theme can be shared before it is saved, and a built-in theme through a
copy of it. A line under the title says how either went, or why a file wasn't
taken. The editor owns the window's colours while it is open, so a file
picked meanwhile (the picker can outlast opening the editor) is turned away
with a note.

A theme file is JSON, the format `layout.json` already uses for a custom
theme, so there is nothing new to learn or parse, and people can edit one in
any text editor. It is named after the theme, `Plum.yapfast-theme.json`; the
picker lists every `.json` file, so a renamed file loads as well.

```json
{
  "format": "yapfast-theme",
  "version": 1,
  "name": "Plum",
  "base": "yap",
  "dark": true,
  "colours": { "rail": "#111215", "chat": "#2a1a3a", "row-hover": "#1a1b20" }
}
```

It differs from a custom theme in `layout.json` in two ways. `format` and
`version` come first: a file without `"format": "yapfast-theme"` is turned
away, so that no other JSON, `layout.json`'s own entries included, loads as
a theme by accident, and one with a higher `version` than the app knows is
turned away with a note to update. `version` grows only for a change an older
Yapfast would read wrongly; a colour added later doesn't need it, as a missing
colour takes the base's. And there is no `id`: Yapfast gives the imported
theme the next free `custom:` id, so importing a file twice makes two themes
and a shared file never collides with the reader's.

Reading follows the same rules as `layout.json`. A colour that is missing or
isn't a colour takes the base's value, and a name Yapfast doesn't know is
ignored, so a file from a newer or older version loads; the note after the
import counts the colours that weren't colours. A file with no colour name
Yapfast knows isn't a theme, and is turned away. The name loses control
characters and stops at 48 characters; a file without one imports as
"Imported theme". A file over 64 KiB (a theme is about 2 KiB) isn't read. A
theme's colours are the only data in it, and nothing in a file is run or
fetched.

| Theme | Id | Dark | Accent | From |
| --- | --- | --- | --- | --- |
| Yap | `yap` | yes | marigold | Yapfast's own, the default |
| Bonfire | `bonfire` | yes | blurple | Discord's Dark theme |
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
