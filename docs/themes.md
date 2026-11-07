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
| Bonfire Ash | `bonfire-ash` | yes | blurple | Discord's Ash theme |
| Bonfire Dark | `bonfire-dark` | yes | blurple | Discord's Dark theme |
| Bonfire Onyx | `bonfire-onyx` | yes | blurple | Discord's Onyx theme |
| Bonfire Light | `bonfire-light` | no | blurple | Discord's Light theme |
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

The four Bonfire themes map Discord's base themes from the [public web
client](https://discord.com/app). Its CSS calls Ash
`.theme-dark`, Dark `.theme-darker`, Light `.theme-light`, and Onyx
`.theme-midnight`. Yap is Yapfast's default theme in both layouts.
The old `bonfire` id is an alias for
`bonfire-dark`, including as a custom theme's base.

The rail and sidebar take `--background-base-lowest`, chat and stage
`--background-base-lower`, the account panel `--background-base-low`, and
raised surfaces and tooltips `--background-surface-high`. The composer uses
`--chat-background-default`, with `--channel-text-area-placeholder` for its
placeholder. Both are editable theme colours, rather than shades derived
from the field and muted text. Plain fields composite
`--input-background-default` over the raised surface. Older custom themes
inherit missing colours from their base.

Text, secondary text, muted text and disabled controls take `--text-default`,
`--text-subtle`, `--text-muted` and `--interactive-muted`. Links use
`--text-link`, and mentions use each base theme's `--mention-foreground`.
Primary and destructive buttons use `--control-primary-background-*` and
`--control-critical-primary-background-*`. Hovers, selections, borders and
secondary buttons composite Discord's translucent neutral washes over their
surfaces. Onyx uses its stronger border washes, 20% for subtle outlines
and the composer border, and 24% for normal separators. Only Onyx adds an outline around the account card and a permanent divider
between the sidebar and chat in the Discord layout. The divider sits inside
the sidebar edge so it does not cover the 2px mention marker in chat. These use the editable
`outline-panel` and `outline-sidebar` colours, which are transparent in the
other built-in themes. The account outline uses Onyx's 16% `--border-muted`
wash. The `mentioned` background uses a shared 8% yellow wash in every
theme, rendering `#130d00` over Onyx's black chat, and fades to 4% on
hover. Mentioned messages have a solid 2px bar in the theme's `loud` colour,
in both chat and threads. Their hover background uses only the mention
wash, without the normal message-hover highlight underneath. Light uses dark overlays,
and Onyx has black base surfaces. The Bonfire themes carry no Discord name
or logo.

Discord role colours stay as servers set them, and some read faintly on a
light theme; Yapfast doesn't adjust them yet.
