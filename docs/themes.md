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

Themes have their own page, Settings → Themes, laid out as Settings →
Plugins is: *Marketplace*, *Installed* (as a list or cards) and *Add from
URL*. Appearance keeps the built-in and custom colour themes, says which
installed theme shows, and has *Browse themes*, which opens the page.

A theme installs the way a plugin does (`PLAN.md` §8): *Add from URL* asks the `yapfast-plugins fetch` helper to read the
repository with gix at one commit. The helper reads `theme.json` and only the
files it names (fonts, pictures and sounds by their extension) plus a licence
file, at most 256 files, each at most 4 MiB and 24 MiB together, with no checkout, symlink or LFS
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
cost; the Marketplace tab shows them as a gallery of large previews,
searchable, and decoded only while the page is open.

*Try 30 s* shows an installed theme, or one under review, until *Keep* or
*Revert*, or for 30 seconds, then the theme from before. *Apply* chooses it,
and *Back to …* on Installed returns to the theme chosen before.

A custom colour theme in Appearance's editor has *Export as a package*: it
writes a folder with its `theme.json` (every colour over its base, `id`
`local.<name>`) and a `LICENSE` to fill in, ready to put in a GitHub
repository.

### Safety

- **Readable.** Text under 4.5:1 against a surface it's drawn on is refused,
  naming the pair: `text`, `secondary`, `muted`, `link` and `accent-text` on
  `sidebar`, `panel`, `chat` and `field`; `on-accent` on `accent`; `text` on
  `raised`, `menu`, `composer` and `actions`; `on-tooltip` on `tooltip`. A gradient is
  checked at every stop. A pair the base theme already draws lower only has to
  keep the base's contrast.
- **Bundled, checked files.** Paths are relative, inside the folder, with `/`;
  a URL, `..` or an absolute path is refused. Sounds are decoded whole, under
  limits (see Sounds). Fonts are `.ttf` or `.otf`,
  parsed whole (inside a guard that refuses a file that panics the parser)
  before anything draws with them. Pictures are PNG, JPEG or WebP, at most
  3840 pixels a side and 2 MiB, and decode whole. SVGs are refused if they hold
  a script, an event handler, `foreignObject`, an animation, a doctype or a
  reference outside the file, and have to render.
- **Protected UI stays stock.** `danger`, `danger-hover` and `on-danger` can't
  be set, so Block, Ban and Delete keep Yapfast's red. While a protected screen
  is open — signing in, a link's confirmation, a plugin's review with its
  permissions, a theme's review — the window shows the stock theme, shape and
  motion, and the theme comes back when the last one closes.
- **The way back.** Ctrl+Shift+T shows the default theme until Yapfast starts
  again; `--default-theme` starts with it. A theme that can't be shown at start
  falls back to the default with a notice saying why.

### Performance

Settings → Themes → Installed → *Performance*:

- **Full**: everything, each animation at the rate it asks for (up to 60
  frames a second).
- **Balanced** (the default): animations marked heavy stay still — anything
  on the `window` or `accent` slots always is — and the rest run at up to 30
  frames a second, paused while Yapfast isn't the focused window.
- **Light**: colours, fonts, shapes and spacing only; no animation, effect or
  hover and popup motion.

*Reduce motion* keeps every theme animation, text effect and hover motion still
whatever the setting. A theme's cost is shown before it's installed, measured
from its files: the memory its fonts hold once registered and its sounds
once decoded (with its icons' files), how many animations it declares and
how many are heavy, the most redraws a second they ask for, its text
effects, how many sounds and icons it ships, and what its layout changes. A clock runs only while an animation may
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
  "motion": { … }, "animations": [ … ], "effects": { … }, "art": { … },
  "sounds": { … }, "icons": { … }, "layout": { … },
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
windows, and switches when they do; with one, the theme is that mode only,
and the marketplace says so.

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
| `composer-focus` | The message box's outline while it has the keys | `muted` at 55% |
| `row-selected` | A selected channel's or settings page's ground | `active` |
| `pill` | The pill beside a selected or unread server | `text` |
| `toggle-off`, `knob-off` | A switch's track and knob while it's off | `field`, `secondary` |
| `code`, `embed` | The grounds of code blocks and of embeds | `panel`, `sidebar` |
| `members` | The member list's ground; it may be see-through (`#00000000`), showing the conversation's ground and art beneath, and its text is then checked over what shows | `chat` (`sidebar` without Discord's look) |
| `app-badge` | The APP tag after a bot's name | Discord's blurple |
| `account` | The account panel's ground at the bottom left (avatar, name, mic, headphones, settings) | `panel` |
| `actions` | The ground of the bar of actions over a message under the pointer | `raised` |
| `actions-hover` | A button's ground under the pointer in that bar, and in the quick reactions atop a message's menu | the stock wash |

A theme that sets `hover` also lights the hovered row of a context menu with
it, instead of the stock white or black wash, and `scrim` dims the window
behind every dialog.

`rail`, `sidebar`, `chat`, `panel`, `raised` and `accent` also take a
gradient, `linear(135deg, #ff71ce, #01cdfe 80%)`: an angle as CSS measures it
(180deg, top to bottom, when left out) and two to eight stops, each at a
percentage or spread evenly. Where one colour is needed, such as a mix, the
first stop stands in; everywhere the token is a ground, the gradient is
drawn.

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
| `code` | Code blocks: corners and outline (stock 1 px of `line`) |
| `embed` | Embeds: corners and outline (stock none) |
| `toggle` | Switches: a `radius` in pixels (999, a pill, as stock), and their border |
| `actions` | The bar of actions over a message under the pointer: quick reactions, Reply, Forward, More |
| `row` | A message row's hover, mention and jump bands: `radius`, and `inset` (below) |

`row` also takes `inset`: pixels each band keeps from the column's sides
(0–32), or `[sides, ends]` with the room it keeps from the row's top and
bottom too (0–8), so flat and `irc` rows can light as rounded, inset
strips. Its `radius` follows `message` when left out. Bands follow the
message style: in `cards` they lie on the card, leaning and cornered as it
is; in `bubbles` hover and a mention tint the bubble (hover as a wash of
`overlay`, not `row-hover`); in `flat` and `irc` they run across the row,
or across the column and 8 pixels past it when `chat_max_width` centres
it.

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

`outline_width` (0–4 pixels) sets every hairline outline and rule the stock
window draws a pixel wide; a surface's own `border` wins. `borders` dashes or
dots an outline, as a "cut here" line. Each place takes `"dashed"`,
`"dotted"` or `"solid"`, or an object: `style` (dashed when left out),
`dash` and `gap` (1–24 pixels; a dot is as long as the line is wide),
`width` (1–4) and `colour`. Width and colour otherwise follow the surface's
`shape` border, then the stock outline. Corners of a dashed outline are
square. A profile card or a server card whose border, outline width or
dashes a theme sets keeps its banner, art and glass inside that border,
under corners rounded to its inner edge, so the border always shows whole;
the stock card keeps its banner to the edge.

| Place | Outline |
| --- | --- |
| `composer` | The message box |
| `card` | Profile cards and popouts |
| `menu`, `dialog`, `tooltip` | Those |
| `message` | A card or bubble around messages |
| `embed`, `code` | Embeds and code blocks |
| `tile` | Cards in settings |
| `divider` | The day and "New" lines across the messages |
| `actions` | The bar of actions over a message |

```json
"shape": {
  "outline_width": 2, "toggle": { "radius": 0 },
  "borders": { "card": "dashed", "divider": { "style": "dotted", "gap": 3 } }
}
```

**Type** (`type`): per role, `font` (a `.ttf` or `.otf` in the folder),
`size`, `weight` (100–900, a variable font's weight axis follows it) and
`letter_spacing` (−2 to 4 pixels; not yet drawn for body text); headings also
take `uppercase`, `header_size` (the open conversation's and the server's
names) and `title_size` (page and dialog titles), which otherwise follow
`size`. Each font is registered under a family name of its
own, once per run.

| Role | Where | Sizes |
| --- | --- | --- |
| `body` | Messages and nearly everything | 12–20 |
| `headings` | The channel's name in its header, the server's name, settings titles | 12–32 |
| `names` | Authors of messages (ransom names too), the member list, the profile card; a message's author follows `body` when unset, and scales with the person's text size | 12–24 |
| `code` | Code blocks and inline code | 11–20 |
| `labels` | Small capital headings, such as THEME | 10–16 |

`names` also takes `style`: `"ransom"` cuts people's names from several
fonts, letter by letter, each letter on a scrap of its own, as a ransom note,
beside messages, on profile cards and in the member list, with `fonts` and
`papers`.
`style` is `ransom`, the only one; the `irc` message style keeps names plain,
so its columns line up. `fonts` lists 2 to 8 font files the
letters cycle through (the names and body faces when left out), checked and
registered as the type faces are; `papers` lists up to 6 `[ground, ink]`
colour pairs the scraps cycle through. Only the first 12 letters are cut,
the rest following as one plain piece; a name with letters outside Latin
script, such as CJK, Arabic or emoji, shows whole. Letters never squeeze: a
name wider than its room is cut at the edge under an ellipsis. A person's own name
style wins, and Settings → Appearance has *Plain names*, which draws every
name plainly.

```json
"type": { "names": { "style": "ransom", "fonts": ["fonts/a.ttf", "fonts/b.otf"],
           "papers": [["#ffffff", "#141414"], ["#ff71ce", "#141414"]] } }
```

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
(`reverse`, `none`), and `hover_in` and `hover_out` (milliseconds a
labelled button's hover takes to come and to go, each following `quick`).
`curve` also takes `cubic-bezier(x1, y1, x2, y2)`; Slint's popups take only
named curves, so it picks the nearest of them there. Hover and press apply to labelled buttons and servers in
the rail (a button's face moves and scales inside the space the layout gives
it); open and close to every popup, menu and dialog.

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
| `property` | `opacity` (0–1), `x`, `y` (−24 to 24 px), `scale` (0.5–1.5), `rotation` (−360 to 360°: `[0, 360]` spins a record), `tint`, `colour` |
| `values` | 2 to 32 numbers, or colours for `tint` and `colour` |
| `period` | Milliseconds for one pass, 100 to 60000 |
| `steps` | Jumps between values in this many steps, as CSS's `steps()`, instead of moving |
| `easing` | A curve from the list above, or `cubic-bezier(x1, y1, x2, y2)` as CSS writes it, across each span between two values |
| `repeat` | `loop`, `alternate` or `once` |
| `delay` | Milliseconds before it starts |
| `heavy` | Stays still in Balanced |
| `phase` | `rail.icon` only: milliseconds each server runs behind the one above it (0–5000) |

| Slot | Moves |
| --- | --- |
| `window` | `tint`, `opacity`: a layer over the whole window that takes no input. Always heavy |
| `accent` | `colour`: the accent itself. Always heavy |
| `home` | The home button at the top of the rail |
| `unread` | The pill beside a server with something new |
| `mention` | The red count of mentions on a server |
| `typing` | The dots of someone typing |
| `channel.name`, `server.name`, `profile.name` | Those names |
| `own.avatar` | Your avatar in the account panel |
| `rail.icon` | Every server in the rail, each `phase` behind the last |

Several `window` tints lay over one another, the later on top.

Text in the `channel.name`, `server.name` and `profile.name` slots moves by
`x` and `y` inside the space its layout gives it, so a moving name never moves
what's beside it.

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


**Art** (`art`): pictures and textures on surfaces, overlays over the
window, glass panes, lit text, particles, effects on fixed events, foil and a
channel change. Each key replaces the one before it, as other layers do; a
variant's `surfaces` and `text` replace the shared ones surface by surface.

```json
"art": {
  "surfaces": {
    "window": [{ "image": "art/sky.jpg" },
               { "image": "art/clouds.png", "fit": "tile", "drift": [14, 0] }],
    "chat": { "texture": "paper", "colour": "#5a4630", "opacity": 0.35, "blend": "multiply" }
  },
  "overlays": [{ "kind": "scanlines", "size": 3, "opacity": 0.12 },
               { "kind": "lcd", "on": "chat", "ghost": true }],
  "glass": { "surfaces": ["sidebar", "chat", "members", "menu", "dialog"],
             "blur": 18, "opacity": 0.55, "highlight": "#ffffff66", "gap": 8, "radius": 14 },
  "text": { "names": { "outline": { "width": 1, "colour": "#000" }, "glow": { "colour": "#ff71ce" } } },
  "particles": [{ "preset": "bubbles", "on": "message.sent", "count": 14 }],
  "events": { "click": "ripple", "hover": "gloss", "select": "highlighter", "reaction": "stamp" },
  "foil": { "surfaces": ["card", "tile"], "colours": ["#ff9ad5", "#9af0ff", "#fff6a8"] },
  "transition": { "kind": "dissolve", "duration": 420 }
}
```

*Surfaces*: `window` (under everything, and what shows between panes with a
gap), `rail`, `sidebar`, `chat`, `members`, `account` (the account panel at the
bottom left, on the `account` colour; `panel`, its first name, is read as it),
`card` (a profile card the person hasn't themed themselves), `dialog`,
`composer`, `menu`, `actions` (the bar of actions over a message, on the
`actions` colour) and `profile-pane` (the profile docked beside a one-to-one
DM, on its ground, `panel`). Each takes a layer or a list of up to 4, bottom
first, drawn over the surface's ground and under its content:

| Field | |
| --- | --- |
| `image` | A PNG, JPEG or WebP in the folder (the checks above apply) |
| `texture` | Or one Yapfast makes, 256 px and tiling: `grain`, `paper`, `kraft`, `noise`, `halftone`, in `colour` |
| `fit` | `cover` (pictures' default), `contain`, `tile` (textures' default), `stretch` |
| `position` | Where it sits when it doesn't fill: `center`, `top left`, … or `[x, y]` from 0 to 1 |
| `opacity` | 0–1 |
| `blend` | `normal`, `multiply`, `screen`, `overlay`, mixed with the surface's own colour (over a gradient, its first stop) |
| `blur` | 0–64 px |
| `dim` | 0–1, toward black |
| `scale` | A tile's size against the picture's, 0.25–4 |
| `colour` | A texture's colour |
| `drift` | `[x, y]` or `{ "x", "y" }`, logical pixels a second (−200 to 200): the layer slides and loops, a tile seamlessly, any other fit as two copies end to end along x (or y when x is 0) |
| `frames` | A sprite sheet: `{ "count", "columns", "period" }`, frames left to right then down, `period` milliseconds for all of them (not with `tile`) |
| `heavy` | Stays still in Balanced (drift and frames run there otherwise) |
| `reserve` | `{ "top", "bottom", "left", "right" }`, 0–1200 logical pixels: the pane's content keeps that clear of its edges, so a decoration drawn there (keys at the foot of the sidebar, a bezel) isn't covered. The content is laid out inside what's left and scrolls there, scrollbars too; the largest any layer asks wins. `sidebar` (its header and channel list; the account panel below stays), `members`, `chat` (header, messages and composer) and `composer` (inside the message box) take it; another surface's is ignored with a note. `yapfast theme check` lists it, and warns when it leaves a pane little room in a small window |
| `size` | `[width, height]`, 4–1600 logical pixels: the layer is drawn at that size, where `position` puts it, instead of over the whole surface, and decoded at no more than twice it; for a decoration such as a D-pad on a shell. Its `fit` is within that size |

Pictures are decoded once per theme, no larger than 2560 × 1600 (a tile no
larger than 1024), with their blend, blur, dim and opacity put into their
pixels then, so drawing them costs a plain picture. Slint has no blend modes
of its own: a blend mixes with the surface's colour, not with what another
layer drew beneath.

*Overlays* (up to 6) lie over everything and take no input: `scanlines`
(`size`, pixels from line to line), `grain` (`period`: milliseconds before it
jumps, 0 still), `lcd` (a pixel grid of red, green and blue stripes, `size`
pixels a pixel, and `ghost`: a brief faint afterimage of the conversation when
the channel changes), `vignette`, `band` (a band of light, `size` tall,
rolling down every `period`), `flare` (`size` across, `at` a position,
pulsing every `period` when set), or a picture (`image` and any layer field).
Each takes `colour`, `opacity`, and `on`: `window` (the default) or `chat`,
over the conversation only. One that moves is heavy unless it says
`"heavy": false`.

*Glass*: Slint can't blur what's behind a pane, so a glass pane draws a
blurred copy of the window's art (kept at a quarter of the window's size)
lined up with where the pane sits, lays its own ground over it at `opacity`
(0–1), and the pane's own layers above that. What shows through is the
window's art, not other panes or messages. `surfaces` (any but `window`;
`sidebar` and `chat` when left out), `blur` (0–64), `highlight` (a 1 px light
on the top and left inner edge), `inset` (a shadow inside the edges, as
`shape` writes one: `blur` is its depth), `gap` (0–64 px between the window's
edge and its two panes, the navigation and the conversation, where the
window's art shows; a glass `account` panel floats the same distance from the
navigation above it and the window's edge) and `radius` (their corners, and
a glass account panel's, when there's a gap).

*Text*: `names` (authors, the member list, the profile card), `headings` (the
conversation's and the server's names, settings titles) and `labels` (small
capital headings) each take `shadow` (as `shape` writes one; its blur isn't
drawn), `glow` (`colour` and `size` 1–12, drawn as two widening outlines at a
third of the colour's alpha), `outline` (`width` 0.5–4 and `colour`) and
`emboss`. Settings titles and labels take the outline only.

*Particles* (up to 4 sets): `preset` `bubbles` (rise and wobble), `sparkles`
(burst and twinkle), `dust` (drift) or `scraps` (paper that tumbles and
falls), `on` `ambient`, `message.sent`, `reaction.added` or `theme.applied`,
`colours` (1–8, picked in turn), `sprite` (a picture drawn instead of the
shape), `count` (1–64: how many fly on an event, or most at once when
ambient), `rate` (ambient, how many start a second), `size` (2–48 px), `life`
(300–12000 ms) and `heavy` (ambient sets are heavy unless they say not).
At most 160 fly at once.

*Events*, one effect each: `click` `ripple` (labelled buttons), `hover`
`gloss` (a sweep of light over labelled buttons and the rail's servers),
`select` `highlighter` (a marker stroke that swipes under the chosen channel),
`reaction` `stamp` (your reaction lands with a thunk), `row` `highlighter`
(a marker stroke swipes under the message under the pointer, in place of
the flat `row-hover` tint; Light and reduced motion keep the tint). Each is the effect's
name, `none`, or `{ "effect", "colour", "duration" }` (80–3000 ms).

*Foil*: a holographic band of three `colours` over `card` and `tile`
surfaces at `opacity`, following the pointer while it's on one and otherwise
sweeping across every `period` milliseconds.

*Transition*: when another channel opens, the conversation as it was
`fade`s or `dissolve`s in blocks over `duration` (100–2000 ms). Yapfast takes
a picture of the window for it, held only while it plays.

Performance: Full runs all of it at up to 60 frames a second. Balanced runs
art at up to 30, paused while Yapfast isn't focused, and holds heavy layers
and overlays still, drops heavy ambient particles, and plays no transition.
Light keeps pictures, textures, glass and lit text, drops heavy overlays,
particles, event effects and transitions (a highlighter stays as a plain
colour), and moves nothing. Reduce motion holds everything still at every
setting. The art clock pauses while the window is minimised or hidden, and
doesn't run at all while nothing moves.

`yapfast theme check` checks text where it sits over art: each text colour
on each surface with art or glass is refused under 4.5:1 (`secondary` and
`muted` under 3:1; either keeps a lower bar its plain ground gives it) against the art's average laid over the surface, and warned
about when a tenth of it reads under 3:1. The cost adds the pictures'
decoded memory, art layers, overlays and particle sets, and counts moving
art among the animations.

**Sounds** (`sounds`): a theme replaces any of Yapfast's own sounds by
name, and gives a sound to any of a closed list of interface events. Each is
a file in the folder, a list of up to eight that take turns at random, or
`{ "file": …, "volume": 0.6 }` (`files` for a list; volume 0 to 1, against
the file as it is). `null` in a variant takes a shared slot's sound away.

```json
"sounds": {
  "message": "sounds/drop.ogg",
  "mention": "sounds/chime.wav",
  "send": ["sounds/bubble-1.wav", "sounds/bubble-2.wav", "sounds/bubble-3.wav"],
  "hover": { "file": "sounds/tick.wav", "volume": 0.5 },
  "toggle-on": "sounds/pop-high.wav", "toggle-off": "sounds/pop-low.wav"
}
```

| Slot | Plays | At most |
| --- | --- | --- |
| `join`, `leave`, `mute`, `unmute`, `deafen`, `undeafen`, `message`, `mention`, `viewer-join`, `viewer-leave`, `stream-start`, `stream-stop` | In place of Yapfast's sound of that name (Settings → Voice → Sounds), at its volume there | 4 s |
| `ring`, `ringback` | In place of a DM call's ring, and the ringback heard while ringing someone; repeated, so a theme's own pause is part of the file | 6 s |
| `hover`, `press` | The pointer comes onto a button, a button is clicked (labelled and icon buttons) | 1.5 s |
| `menu-open`, `menu-close` | A context menu opens; it closes on a choice or Escape. A message's menu plays only its opening | 1.5 s |
| `popup-open`, `popup-close` | A popup, card or dialog opens and closes | 1.5 s |
| `toggle-on`, `toggle-off` | A switch in settings turns on or off | 1.5 s |
| `send`, `reaction-add` | This account sends a message; clicks a reaction | 1.5 s |
| `theme-applied` | The theme is applied, or its trial starts | 1.5 s |

Files are `.wav` (PCM 8–32-bit or 32-bit float, mono or stereo, 8–96 kHz),
`.ogg` (Vorbis or Opus), `.opus` or `.mp3`, at most 1 MiB each. Each is
decoded whole when the theme is checked, inside a guard that refuses a file
the decoder panics on, and refused if it doesn't decode, is silent or lasts
longer than its slot allows. Showing the theme decodes each file once, to 48
kHz stereo, and every play shares those samples; one louder than Yapfast's
own sounds (12 dB under full scale) is lowered to them.

What the person chose always wins. A replaced sound plays at that sound's own
volume and the Sounds volume, so one turned down to 0 stays silent, and
deafening still silences all but deafening's; a message's sound still doesn't
play in Do Not Disturb, or for the conversation in front. Interface sounds
play only while Yapfast is the focused window and never in Do Not Disturb,
at the Sounds volume times the theme's own; hovers at most every 70 ms, and
no more than four of a theme's sounds at once. The person picks which of a
theme's sounds play: its review, before Install or Try, shows *Notification
sounds* (the replaced ones; on) and *Interface sounds — clicks, hovers,
menus, send* (off) for a theme that ships them, and Settings → Themes →
Installed → *Sounds* has the same two switches for the theme shown, with the
interface sounds' volume (50%). They're kept per theme in `layout.json`'s
`theme_sounds`; a theme that was never reviewed in this Yapfast, such as one
under `--theme-dev`, starts with the same defaults. A protected screen plays
none of a theme's sounds. Sounds run in every Performance
setting: they cost nothing until something plays.

**Icons** (`icons`): a theme replaces any of the window's icons, the files
in `ui/icons/`, by name without `.svg` — `hash` for text channels, `logo`
for the home button at the top of the rail, `send-horizontal`, `settings`,
`moon` and so on — with an SVG in its folder. An icon it doesn't name stays
stock. Each is a file, or `{ "file": …, "colour": … }`:

| `colour` | Drawn |
| --- | --- |
| `tint` (the default) | In one colour, the one its place draws the stock icon in, as hover and selection change it |
| `keep` | In the SVG's own colours |
| a colour's name, such as `accent-text` | In that colour of the theme, wherever it shows |
| `#rrggbb` | In that colour |

```json
"icons": {
  "hash": "icons/hash.svg",
  "logo": { "file": "icons/home.svg", "colour": "keep" },
  "pin": { "file": "icons/pin.svg", "colour": "accent-text" }
}
```

An icon is checked as every SVG in a theme is (see Safety) and drawn at the
size it shows, so it should be square and read at 16 pixels. A theme's icons
show everywhere Yapfast draws that icon, channel lists and menus included,
and go back to stock while a protected screen is open. Status dots are drawn
shapes, not icons, except Idle's `moon`. Pointer cursors stay the system's:
Slint offers only the standard named cursors, the same on Windows and Linux,
and no picture of a theme's own.

The window draws every icon through the Slint global `IconPack`
(`ui/media.slint`), one image property per icon that starts as the stock
file, and every icon's colour through `IconPack.tint`, which returns the
place's colour without asking Rust until a theme paints an icon its own
way. `src/ui/theme_media.rs` sets them, and the interface events' sounds
come from the `Cue` global. A new icon in `ui/icons/` needs a property in
`IconPack`, a line in `theme_media.rs`'s list and its name in
`yapfast_core::themes::media::ICONS`; a test checks the three agree.

**Layout** (`layout`): where the panes sit and how messages are laid out,
each from a closed list. Yapfast moves what it already draws; it never adds
or takes away a control.

| Field | Values | Stock |
| --- | --- | --- |
| `rail` | `left`, `right`, `top`: the edge the server rail runs along | `left` |
| `members` | `right`, `left`, `hidden`: where the member list docks, or that it doesn't; `collapsed`: on the right as a 56-pixel strip of avatars that opens to the whole list, over the conversation, while the pointer is on it (at once with reduced motion or Light) | `right` |
| `composer` | `bottom`, `top`: the message box under or over the messages | `bottom` |
| `messages` | `flat`, `bubbles`, `irc`, `cards` (below) | `flat` |
| `chat_max_width` | 480–1600 pixels: the messages and the composer grow no wider, centred | none |
| `chat_avatars` | `false` hides the avatars beside messages | `true` |
| `irc`, `bubbles`, `cards` | Settings for those message styles | |

| Messages | What |
| --- | --- |
| `flat` | Avatar, name and time over the text, as stock |
| `bubbles` | Each message in a bubble; your own on the right, without avatar or name, as a phone's messenger shows them |
| `irc` | One line a message, as a terminal's chat log: a time column, the name right-aligned in a column of its own, a separator, then the text |
| `cards` | Each run of messages from one person on a card of its own, which may lean a little, as scraps pasted on a page |

Every style keeps what a message carries: replies, reactions, embeds,
attachments, threads, edits, the day and "New" dividers, the hover bar and
the keyboard's message. A kind of message a style doesn't lay out, such as a
notice that someone pinned a message or joined, draws the stock row, one
message at a time, and so does any kind a later Yapfast adds. Focus order,
the focus ring and every accessible name stay as stock.

- `irc`: `time_width` (0–120 pixels, 0 hides the time), `nick_width` (48–240
  pixels; a longer name ends in an ellipsis, and pointing at it shows the
  whole name) and `separator` (at most 3 characters,
  such as `│` or `>`).
- `bubbles`: `max_width` (40–95, a percentage of the column), `radius`
  (pixels), and the grounds `own` and `other` with `own_text`, the text on
  your own. Unset, your own are `accent` with `on-accent` text (`text` on a
  ground the theme sets), and others' `raised` with `text`; either pair has
  to read at 4.5:1, at each stop of a gradient. `own` and `other` each take
  a colour, a gradient (`linear(…)`, as `colours` writes one), or `{
  "ground": …, "art": […] }`: up to 4 layers laid on every such bubble over
  its ground and under its text, as a surface's are (still: no `drift` or
  `frames`; a `size` places a small one, such as a gel's highlight), each
  decoded once and shared by every bubble; the text has to read over them
  as over a surface's art. A reply's quote, "(edited)" and "Forwarded" on
  your own bubble take its text's colour. The bar of actions over a message
  sits beside the bubble under the pointer: after someone else's, before
  your own.
- `cards`: `tilt` (0–4 degrees; each card leans its own way within it, from
  who wrote it and when), `gap` and `padding` (0–24 pixels), and `ground`
  (`raised` unset), which `text` has to read on. Only the card leans: text
  never rotates. `shadow`, as `shape` writes one (`blur` 0 for a hard
  offset), lies under each card and leans with it. `art` pastes one or two
  up to four pictures on each card, such as a strip of tape, leaning with
  it: each a layer (`image` or `texture`, `opacity`, `blend` with the
  card's ground, `blur`, `dim`, `colour`, `fit` within its size; never
  `drift` or `frames`) with `position` (`top`, `bottom right`, … or `[x,
  y]`: across the card, and its top edge, or its bottom with y over 0.5),
  `size` (`[width, height]`, 4–1600 by 4–240 pixels, or `"full"` for the
  card's width; never wider than the card; the picture's own when left
  out), `offset` (`[x, y]`, −64–64 pixels from its place, centred on the
  edge) and `rotation` (−45–45 degrees beyond the card's lean). `image` may
  list up to 6 pictures: each card picks one by who wrote it and when, the
  same every time it's drawn. A picture is decoded once, at no more than
  twice its `size` (960 × 480 without one), and every card shares it.
  `stagger` (0–48 pixels) steps each card that far in from its left or its
  right, by the same seed as its lean, so cards down the page fall to
  either side. `tint_art: true` lays a hovered or mentioned card's tint
  over its pictures too, where they're drawn.

```json
"layout": {
  "messages": "cards",
  "cards": { "tilt": 1.2, "shadow": { "blur": 0, "x": 5, "y": 5, "colour": "#14141440" },
             "art": [{ "image": "art/tape.png", "position": "top left", "size": [84, 22],
                       "offset": [18, -2], "rotation": -5 }] }
}
```

```json
"layout": {
  "messages": "bubbles", "members": "collapsed",
  "bubbles": { "own": { "ground": "linear(180deg, #7fd4ff, #2a7fff)",
                        "art": [{ "image": "art/gloss.png", "fit": "stretch", "opacity": 0.7 }] },
               "other": "linear(180deg, #ffffff, #e8eef8)", "own_text": "#06213f" }
}
```

```json
"layout": {
  "rail": "top", "messages": "irc", "chat_avatars": false, "chat_max_width": 960,
  "irc": { "time_width": 56, "nick_width": 120, "separator": "│" }
}
```

**Parts.** A later Yapfast lets a theme rebuild a few named parts —
`message-row`, `member-row`, `account-panel`, `server-header`,
`profile-card` — from a widget tree. Each part will have a contract version,
the fields it receives; minor versions only add fields, and a part built for
another major version draws the stock part with a note on the theme's page.
This Yapfast offers none yet: a theme that declares `parts` shows the stock
parts, and its page says it needs a later Yapfast.
