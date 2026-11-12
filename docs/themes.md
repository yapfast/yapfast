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

## Installed themes

A theme can also be a package: a folder with a `theme.json` and the files it
names, which changes how Yapfast looks — colours, shapes, type, spacing,
motion, declared animations and text effects — and never what it does. It's
data, never code: there is no selector, no expression and no script, only
names from the closed lists below, and Yapfast applies each value where it
already draws that thing. `crates/yapfast-core/src/themes/` reads and checks
packages; `src/ui/theme_look.rs` applies them through the Slint global `Look`
and the theme-only colours of `Theme` (`ui/theme.slint`). Every default in
`Look` is the stock value, so with no theme the window draws exactly as it did.

Start from [`themes/template/`](../themes/template/theme.json): copy the
folder, change `id` and `name`, and run

```sh
yapfast --theme-dev path/to/your-theme      # shows it, and again on each save
yapfast theme check path/to/your-theme      # the checks an install runs
yapfast theme check path/to/your-theme --marketplace   # and those for listing
```

`--theme-dev` checks the folder on each save; a version that fails leaves the
last good one on the window and says why in the banner.

### Distribution

A theme installs the way a plugin does (`PLAN.md` §8): Settings → Plugins →
Themes → *Add from GitHub* asks the `yapfast-plugins fetch` helper to read the
repository with gix at one commit. The helper reads `theme.json` and only the
files it names (fonts and pictures by their extension) plus a licence file,
each at most 4 MiB and 24 MiB together, with no checkout, symlink or LFS
pointer. Yapfast then checks them (below), shows the review — name, source,
short commit, *Unreviewed source*, what it ships (dark, light or both), its
cost and anything this Yapfast ignores — and on Install writes them to
`<data>/plugins/store/<sha256>/`, named after every file's digest. `lock.json`
names it under `themes`, beside `plugins`, with its source, folder, ref, the
pinned `rev`, its store directory and `theme.json`'s SHA-256, and the version
before an update, which *Roll back* returns to. Neither store drops the
other's entries. A theme has no grants, no data and no enabled switch.

For now only public GitHub repositories are accepted (`https://github.com/
<owner>/<repo>`), for URL installs and listings alike. The marketplace lists
themes in the plugin catalog as entries with `kind: "theme"`, their modes and
cost; the Themes tab shows them beside the installed ones as a gallery of
large previews, decoded only while the tab is open.

*Try 30 s* shows an installed theme, or one under review, until *Keep* or
*Revert*, or for 30 seconds, then the theme from before. *Apply* chooses it;
installed themes also show in Appearance after the built-in and custom ones,
and *Back to …* there returns to the theme chosen before.

### Safety

- **Readable.** Text under 4.5:1 against a surface it's drawn on is refused,
  naming the pair: `text`, `secondary`, `muted`, `link` and `accent-text` on
  `sidebar`, `panel`, `chat` and `field`; `on-accent` on `accent`; `text` on
  `raised`, `menu` and `composer`; `on-tooltip` on `tooltip`. A gradient is
  checked at every stop. A pair the base theme already draws lower only has to
  keep the base's contrast.
- **Bundled, checked files.** Paths are relative, inside the folder, with `/`;
  a URL, `..` or an absolute path is refused. Fonts are `.ttf` or `.otf`,
  parsed whole (inside a guard that refuses a file that panics the parser)
  before anything draws with them. Pictures are PNG, JPEG or WebP, at most
  3840 pixels a side and 2 MiB, and decode whole. SVGs are refused if they hold
  a script, an event handler, `foreignObject`, an animation, a doctype or a
  reference outside the file, and have to render.
- **Protected UI stays stock.** `danger`, `danger-hover` and `on-danger` can't
  be set, so Block, Ban and Delete keep Yapfast's red. While a protected dialog
  is open — a link's confirmation, a theme's review — the window shows the
  stock theme, shape and motion, and the theme comes back when it closes.
- **The way back.** Ctrl+Shift+T shows the default theme until Yapfast starts
  again; `--default-theme` starts with it. A theme that can't be shown at start
  falls back to the default with a notice saying why.

### Performance

Settings → Appearance → *Theme performance*:

- **Full**: everything, each animation at the rate it asks for (up to 60
  frames a second).
- **Balanced** (the default): animations marked heavy stay still — anything
  on the `window` or `accent` slots always is — and the rest run at up to 30
  frames a second, paused while Yapfast isn't the focused window.
- **Light**: colours, fonts, shapes and spacing only; no animation, effect or
  hover and popup motion.

*Reduce motion* keeps every theme animation, text effect and hover motion still
whatever the setting. A theme's cost is shown before it's installed, measured
from its files: the memory its fonts hold once registered, how many
animations it declares and how many are heavy, the most redraws a second
they ask for, and its text effects. A clock runs only while an animation may
move; text effects tick only while one plays.

### `theme.json`

```json
{
  "theme_api": 1,
  "id": "com.example.tide",
  "name": "Tide",
  "version": "1.0.0",
  "description": "Teal and plum, round and roomy.",
  "authors": ["Ada"],
  "license": "OFL-1.1 AND CC-BY-4.0",
  "source": "https://github.com/ada/tide",
  "based_on": "https://github.com/someone/theme-it-remixes",
  "preview": ["previews/chat.png", "previews/profile.png"],
  "icon": "icon.png",
  "colours": { … }, "shape": { … }, "type": { … }, "spacing": { … },
  "motion": { … }, "animations": [ … ], "effects": { … },
  "variants": {
    "dark": { "base": "yap", "colours": { … } },
    "light": { "base": "sakura", "colours": { … } }
  }
}
```

`theme_api` is 1. Fields are only ever added within a version, and a name
this Yapfast doesn't know is ignored with a note on the theme's page, so a
theme written for a later Yapfast still opens. `id` is reverse-DNS in
lowercase and stays the same forever; `version` is `1.2.0` or `1.2`. `preview`
lists up to six pictures, which the marketplace needs at least one of.
Listing also needs `license`, a licence file (`LICENSE`, `LICENSE.txt`,
`LICENSE.md`, `LICENCE`, `LICENCE.txt` or `OFL.txt`), `source` on GitHub and a
`description`.

**Variants.** A theme ships `dark`, `light` or both. Each names its `base`, a
built-in theme of the same darkness whose colours stand in for any the theme
doesn't set, and may override any layer for itself; the layers at the top
level apply to both. With both, the variant follows the system's light or dark
windows; with one, the theme is that mode only, and the marketplace says so.

**Colours** (`colours`): each `#rgb`, `#rrggbb` or `#rrggbbaa`, by its name in
`ui/theme.slint`: `rail`, `sidebar`, `panel`, `chat`, `stage`, `raised`,
`field`, `sunken`, `hover`, `active`, `row-hover`, `line`, `edge`, `outline`,
`outline-chat`, `outline-field`, `text`, `secondary`, `muted`, `faint`,
`accent`, `accent-hover`, `on-accent`, `accent-soft`, `live`, `loud`, `link`,
`button-default`, `button-default-hover`, `overlay`, `shimmer`,
`reply-spine`, `tooltip`, `avatar`. Only themes set these, each following
another until one does:

| Colour | Paints | Follows |
| --- | --- | --- |
| `accent-text` | The accent as text or an icon on a surface | `accent` |
| `on-tooltip` | The tooltip's text | `text` |
| `selection` | Selected text's ground | `accent` at 35% |
| `focus-ring` | The ring around what has the keyboard | `accent` |
| `scrollbar`, `scrollbar-track` | A scrollbar's thumb, and its track under the pointer | white or black washes |
| `scrim` | The dim behind dialogs | each dialog's own |
| `menu`, `menu-line` | A context menu's ground and outline | `raised`, `line` mixes |
| `composer`, `placeholder` | The message box and its placeholder | `field`, `muted` mixes |
| `mentioned`, `mentioned-hover` | A message that mentions you | marigold at 12% and 16% |

`rail`, `sidebar`, `chat`, `panel`, `raised` and `accent` also take a
gradient, `linear(135deg, #ff71ce, #01cdfe 80%)`: an angle as CSS measures it
(180deg, top to bottom, when left out) and two to eight stops, each at a
percentage or spread evenly. Where one colour is needed, such as a mix, the
first stop stands in.

**Shape** (`shape`): per surface, `radius` (pixels, 999 for a pill),
`border` (0 to 4), `border_colour`, and `shadow` (`blur` 0–48, `x` −16–16,
`y` −16–32, `colour`; `"none"` for none). An avatar's `radius` is a
percentage of its size: 50 a circle, 0 a square.

| Surface | What |
| --- | --- |
| `button` | Labelled buttons |
| `field` | Text fields and search boxes |
| `composer` | The message box |
| `card` | Profile cards and popouts |
| `menu` | Context menus |
| `dialog` | Popups over a dimmed window |
| `tooltip` | The tooltip |
| `chip` | Reactions and other pills |
| `message` | A message's hover and highlight |
| `rail-icon` | Servers in the rail |
| `avatar` | Every avatar |
| `tile` | Cards in settings |

Every other corner in the window takes `radii`: `small` for stock corners of
4 pixels or less, `medium` up to 8, `large` above. Every other shadow takes
`shadows`, one shadow for all (`"none"` drops them).

```json
"shape": {
  "button": { "radius": 999 },
  "card": { "radius": 20, "border": 2, "border_colour": "#3fd6b8",
            "shadow": { "blur": 0, "x": 6, "y": 6, "colour": "#3fd6b855" } },
  "avatar": { "radius": 30 },
  "radii": { "small": 2, "medium": 6, "large": 10 },
  "shadows": { "blur": 0, "x": 3, "y": 3, "colour": "#141414" }
}
```

**Type** (`type`): per role, `font` (a `.ttf` or `.otf` in the folder),
`size`, `weight` (100–900) and `letter_spacing` (−2 to 4 pixels); headings
also take `uppercase`. Each font is registered under a family name of its
own, once per run.

| Role | Where | Sizes |
| --- | --- | --- |
| `body` | Messages and nearly everything | 12–20 |
| `headings` | The channel's name in its header, the server's name, settings titles | 12–32 |
| `names` | Authors of messages, the member list, the profile card | 12–24 |
| `code` | Code blocks and inline code | 11–20 |
| `labels` | Small capital headings, such as THEME | 10–16 |

**Spacing** (`spacing`), in pixels: `message_gap` (0–16), `message_group_gap`
(0–40), `channel_row_height` (24–48), `member_row_height` (32–56),
`avatar_size` (24–56), `chat_padding` (4–48), `sidebar_width` (200–420),
`members_width` (200–400). `density` (`compact`, `default` or `roomy`) sets
several at once; values named beside it win. A plugin's size wins over a
theme's.

**Motion** (`motion`): `quick` (milliseconds a hover's colour change takes,
stock 120), `popup` (milliseconds a popup takes, stock 100), `curve` (`linear`,
`ease`, `ease-in`, `ease-out`, `ease-in-out`, `spring`, `bounce`, `elastic`),
`hover` (`fade`, `none`, `lift`, `glow`, `scale`), `press` (`none`, `scale`,
`sink`), `open` (`grow`, `none`, `fade`, `zoom`, `pop`) and `close`
(`reverse`, `none`). Hover and press apply to buttons and servers in the rail;
open and close to every popup, menu and dialog.

**Declared animations** (`animations`, at most 16): each moves one property
of a slot through values. They're values Yapfast computes on one clock; nothing
runs.

```json
"animations": [
  { "slot": "unread", "property": "scale", "values": [1, 1.6, 1],
    "period": 1400, "easing": "ease-in-out" },
  { "slot": "window", "property": "tint", "values": ["#00ff4100", "#00ff410a"],
    "period": 120, "steps": 2 },
  { "slot": "home", "property": "rotation", "values": [-6, 6],
    "period": 1800, "repeat": "alternate", "delay": 500 }
]
```

| Field | |
| --- | --- |
| `slot` | Where: below |
| `property` | `opacity` (0–1), `x`, `y` (−24 to 24 px), `scale` (0.5–1.5), `rotation` (−45 to 45°), `tint`, `colour` |
| `values` | 2 to 32 numbers, or colours for `tint` and `colour` |
| `period` | Milliseconds for one pass, 100 to 60000 |
| `steps` | Jumps between values in this many steps, as CSS's `steps()`, instead of moving |
| `easing` | A curve from the list above, across each span between two values |
| `repeat` | `loop`, `alternate` or `once` |
| `delay` | Milliseconds before it starts |
| `heavy` | Stays still in Balanced |

| Slot | Moves |
| --- | --- |
| `window` | `tint`, `opacity`: a layer over the whole window that takes no input. Always heavy |
| `accent` | `colour`: the accent itself. Always heavy |
| `home` | The home button at the top of the rail |
| `unread` | The pill beside a server with something new |
| `mention` | The red count of mentions on a server |
| `typing` | The dots of someone typing |
| `channel.name`, `server.name`, `profile.name` | Those names (opacity, scale and rotation) |
| `own.avatar` | Your avatar in the account panel |

**Text effects** (`effects`): a built-in effect that plays when a slot's text
changes: `decrypt` (letters scramble and settle left to right) or
`typewriter` (letters appear behind a cursor), with a `duration` of 100–3000
milliseconds (600 when left out). Slots: `channel.name`, `server.name`,
`profile.name`, `settings.title`, `welcome`. The text's accessible name stays
the plain text.

```json
"effects": { "channel.name": "decrypt",
             "profile.name": { "effect": "typewriter", "duration": 900 } }
```

**Parts.** A later Yapfast lets a theme rebuild a few named parts —
`message-row`, `member-row`, `account-panel`, `server-header`,
`profile-card` — from a widget tree. Each part will have a contract version,
the fields it receives; minor versions only add fields, and a part built for
another major version draws the stock part with a note on the theme's page.
This Yapfast offers none yet: a theme that declares `parts` shows the stock
parts, and its page says it needs a later Yapfast.
