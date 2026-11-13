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
  limits (see Sounds), and cursors under theirs (see Cursors). Fonts are `.ttf` or `.otf`,
  parsed whole (inside a guard that refuses a file that panics the parser)
  before anything draws with them. Pictures are PNG, JPEG or WebP, at most
  3840 pixels a side and 2 MiB, and decode whole. SVGs are refused if they hold
  a script, an event handler, `foreignObject`, an animation, a doctype or a
  reference outside the file, and have to render.
- **Protected UI stays stock.** `danger`, `danger-hover` and `on-danger` can't
  be set, so Block, Ban and Delete keep Yapfast's red. While a protected screen
  is open — signing in, a link's confirmation, a plugin's review with its
  permissions, a theme's review — the window shows the stock theme, shape and
  motion and the system's cursors, and the theme comes back when the last one
  closes.
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
effects, how many sounds, icons and cursors it ships (cursors as their
pictures decoded), and what its layout and chrome change. A clock runs only while an animation may
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
  "sounds": { … }, "icons": { … }, "layout": { … }, "content": { … },
  "chrome": { … },
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
`description` is at most 300 characters: `yapfast theme check` refuses a longer
one, naming its length, and so does Yapfast. An installed theme whose
`theme.json` Yapfast refuses is left out of Settings → Themes, and choosing
it, or starting with it chosen, says why.
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
| `row-selected-text` | Text and icons on a selected channel row: with a light `row-selected`, an inverse-video selection | `text` |
| `card-button`, `card-button-hover`, `card-button-icon` | The round buttons over a profile card's banner (add friend, more, close), under the pointer, and their icons | black at 50% and 70%, white |

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
| `card-button` | The round buttons over a profile card's banner: `radius`, `border`, `border_colour`, `shadow` |

`row` also takes `inset`: pixels each band keeps from the column's sides
(0–32), or `[sides, ends]` with the room it keeps from the row's top and
bottom too (0–8), so flat and `irc` rows can light as rounded, inset
strips. Its `radius` follows `message` when left out. Bands follow the
message style: in `cards` they lie on the card, leaning and cornered as it
is; in `bubbles` hover and a mention tint the bubble (hover as a wash of
`overlay`, not `row-hover`); in `flat` and `irc` they run across the row,
or across the column and 8 pixels past it when `chat_max_width` centres
it.

`card` also takes `inset_top` (0–64 pixels): room kept above the banner of a
profile card the person hasn't themed themselves, for chrome its art draws,
such as a window's title bar. The banner, the avatar and the banner's
buttons move down by it; the card's art covers it.

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
| `panes` | Edges between the rail and the sidebar, the navigation and the conversation, and the conversation and the member list. Drawn only when named, `"solid"` included: a terminal's frame lines |

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
`avatar_size` (24–56), `chat_padding` (4–48; the message box's sides
follow it too), `sidebar_width` (200–420),
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
`actions` colour), `settings` (the settings window: its page with its
tiles and fields, and the sections beside it, on `panel`), `embed`, `code`
(code blocks), `row-selected` (the selected channel, on `row-selected`), `profile-pane`
(the profile docked beside a one-to-one DM, on its ground, `panel`) and
`card_over`. Each takes a layer or a list of up to 4, bottom first, drawn
over the surface's ground and under its content, except `card_over`, which
lies over a profile card's content, such as a gloss across its banner, and
takes no clicks:

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
larger than 1024), with their blend, blur and dim put into their pixels
then, so drawing them costs a plain picture. Opacity is drawn instead, so
one picture serves every surface that lays it, whatever its opacity there;
a picture shows in memory once unless surfaces blur, dim, scale or blend it
differently, and a theme's other variant keeps what it shares. Slint has no blend modes
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
a glass account panel's, when there's a gap) and `rim` (`colour` and
`width`, 0.5–4 pixels: a hard line at each pane's very edge, outside the
inner shadow and the highlight). A rail in such a pane (a gap, round
corners, a rim or a glass edge light or inner shadow) keeps its first
server 8 pixels, plus 2 for the edge light or the rim's width, from the
pane's edge, and its pills off the rim.

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
(300–12000 ms), `heavy` (ambient sets are heavy unless they say not),
`from` (where they start: `composer`, the message box; `send-button`, its
send button, or the box's end in Discord's look; `apply-button`, the last
Apply or Try pressed in Settings → Themes; `pointer`, the last reaction or
send button pressed; `window`, each preset's own place across the window,
which a set takes unless it says otherwise, and `message.sent` sets
`composer`) and `z` (`over`, above everything, or `under`, behind the
conversation's messages and message box, so send bubbles rise behind the
text). At most 160 fly at once.

*Events*, one effect each: `click` `ripple` (labelled buttons), `hover`
`gloss` (a sweep of light over labelled buttons and the rail's servers),
`select` `highlighter` (a marker stroke that swipes under the chosen channel),
`reaction` `stamp` (your reaction lands with a thunk), `row` `highlighter`
(a marker stroke swipes under the message under the pointer, in place of
the flat `row-hover` tint; Light and reduced motion keep the tint),
`name_hover` `decrypt` (an author's or a member's name scrambles and
settles once as the pointer comes onto it, as the `decrypt` text effect;
screen readers keep the plain name, and Light and reduced motion play
nothing). Each is the effect's
name, `none`, or `{ "effect", "colour", "duration" }` (80–3000 ms).

*Foil*: a holographic band of three `colours` over `card` and `tile`
surfaces at `opacity`, following the pointer while it's on one and otherwise
sweeping across every `period` milliseconds.

*Transition*: when another channel opens, the conversation as it was
`fade`s or `dissolve`s in blocks over `duration` (100–2000 ms). Yapfast takes
a picture of the window for it, held only while it plays.

Showing a theme never waits on its files: choosing one in Settings, Apply
and Try check its folder, read its fonts, decode its sounds and bake its
pictures on another thread while the window keeps the theme it shows, then
set the new one's values in one go. Starting, `--theme-dev` reloads and a
change between light and dark do the same work where they are.

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
shapes, not icons, except Idle's `moon`. Mouse cursors are their own layer:
see Cursors.

The window draws every icon through the Slint global `IconPack`
(`ui/media.slint`), one image property per icon that starts as the stock
file, and every icon's colour through `IconPack.tint`, which returns the
place's colour without asking Rust until a theme paints an icon its own
way. `src/ui/theme_media.rs` sets them, and the interface events' sounds
come from the `Cue` global. A new icon in `ui/icons/` needs a property in
`IconPack`, a line in `theme_media.rs`'s list and its name in
`yapfast_core::themes::media::ICONS`; a test checks the three agree.

**Cursors** (`cursors`): a theme gives any kind of mouse cursor a PNG of its
own, by the kind's CSS name: `default`, `pointer` (links and buttons),
`text`, `grab`, `grabbing`, `not-allowed`, `no-drop`, `wait`, `progress`,
`help`, `crosshair`, `move`, `copy`, `alias`, and the resize kinds
`col-resize`, `row-resize`, `ew-resize`, `ns-resize`, `nesw-resize`,
`nwse-resize`, `n-resize`, `e-resize`, `s-resize`, `w-resize`,
`ne-resize`, `nw-resize`, `se-resize` and `sw-resize`. A kind it doesn't name
keeps the system's cursor, and `null` in a variant gives one back.

```json
"cursors": {
  "default": { "image": "cursors/arrow.png", "hotspot": [2, 1] },
  "pointer": { "image": ["cursors/hand.png", "cursors/hand@2x.png"], "hotspot": [9, 2] },
  "text": { "image": "cursors/beam.png", "hotspot": [16, 16] }
}
```

`image` is one picture, or the same picture at up to 4 sizes, smallest
first and each the same shape; on a screen scaled above 100% the window
shows the smallest at least as large as the first times the scale, or the
largest. `hotspot` is the point that clicks, `[x, y]` in pixels from the
first picture's top left (`[0, 0]` if left out), scaled for the larger
sizes. A cursor is a PNG of at most 128 pixels a side and 256 KiB, decoded
whole by the check and again, under the same limits, when the theme shows;
a picture that doesn't load leaves that kind the system's. 32 pixels, with
64 for HiDPI, matches the system's cursors. Keep a contrasting outline so
it shows on every ground, and keep `text` thin: an I-beam that hides the
caret is a bug. A PNG shows still: an animated picture shows its first
frame. Cursors animate from frames instead:

- A kind gives `frames` in place of `image`: a list (each a path, or a
  list of its sizes) or a pattern with `{nn}` and a `count` (1–64), as
  `"cursors/wait/{nn}.png"` for `00`, `01`, …; `ms`, one number (16–5000)
  for every frame or one each (100 unset); `loop` (true unset for a kind);
  and `hotspot`. `hidpi`, beside the kinds, is a suffix such as `"@2x"` that
  gives every frame its larger size, `wait/03@2x.png` beside `wait/03.png`.
  `frames` may instead be a sheet, one picture holding the frames side by
  side: `{ "sheet": "cursors/wait.png", "cell": [64, 64], "count": 12 }`
  (and `first`, the cell to start at), cells running left to right and
  then down, as many to a row as the sheet is wide; `hidpi` gives the
  sheet its larger size. A sheet is at most 2048 pixels a side and 2 MiB,
  decoded once a size and cut into its frames, and is one file however
  many frames it holds.
- `transitions` play once, by name: `"default>pointer"` as the cursor
  changes from one kind to another, `"press"` and `"release"` as the left
  button goes down and up over the `default` cursor, and
  `"press:pointer"`, `"release:pointer"` (any kind after the colon). Each
  takes the fields above, `loop` false unset.
- `motion`, by kind: `move`, a pattern with `{dir}` (`e`, `se`, `s`,
  `sw`, `w`, `nw`, `n`, `ne`, as the pointer heads on screen) and
  `{speed}` (`slow`, `fast`): the frame that shows while the pointer moves
  that way at that speed, or a sheet of 16 cells, each direction's slow
  frame and then its fast one; `settle`, frames played once as it stops, their
  path holding `{dir}` for the way it was moving (and `{speed}` for a
  settle of each speed; a sheet without `{dir}` holds every direction's
  frames, one direction after another, from `e`), with `slow_start`, the
  frame a slow stop starts from; `idle`, a loop once it's been still
  `idle_after` milliseconds (600 unset); and `hotspot`. `slow` and `fast`,
  beside the kinds, are the speeds in logical pixels a second (60 and 700
  unset) the pointer counts as moving and as fast from, measured as an
  average over about 40 ms. A press, a release or a transition plays over
  motion, and a kind's own frames over all of them.
- `trail`: sprites cut from one `sheet` (a PNG of at most 1024 pixels a
  side, with its `hidpi` size) that fade, shrink and fall behind the
  pointer while it moves: `cell` (pixels a sprite is across on the first
  size), `sprites` (up to 64 `[x, y]` corners on it, picked at random;
  list one more often for it to show more often), `life` (100–5000 ms),
  `every` (logical pixels of motion between spawns), `per_step` (1–8),
  `jitter` (0–64 px), `count` (the most alive at once, 1–160), `press`
  (how many burst out on a click), `opacity` (what a new one starts at) and
  `corrupt`, a glitch now and then while the trail is alive: `every` and
  `lasts` (milliseconds, or `[least, most]` for a random time between,
  2500–7000 and 140–300 unset), `sprites` the trail's glyphs turn into,
  `boost` (their opacity times 1–4, 1.8 unset), and `tear`, how far each
  band `band` pixels tall slides sideways (18 and 10 unset). The glitch
  moves only the trail's own sprites.

```json
"cursors": {
  "hidpi": "@2x",
  "default": { "image": ["cursors/default.png", "cursors/default@2x.png"], "hotspot": [16, 16] },
  "wait": { "frames": "cursors/anim/wait/{nn}.png", "count": 12, "ms": 80, "hotspot": [32, 32] },
  "transitions": {
    "default>pointer": { "frames": "cursors/anim/to-pointer/{nn}.png", "count": 12, "ms": 35, "hotspot": [32, 32] },
    "press": { "frames": "cursors/anim/press/{nn}.png", "count": 5, "ms": [40, 50, 60, 90, 180], "hotspot": [32, 32] }
  },
  "motion": {
    "default": { "move": "cursors/anim/move/{dir}-{speed}.png", "hotspot": [32, 32],
                 "settle": { "frames": "cursors/anim/settle/{dir}/{nn}.png", "count": 6, "ms": 50, "slow_start": 2 },
                 "idle": { "frames": "cursors/anim/idle/{nn}.png", "count": 16, "ms": 150 } }
  },
  "trail": { "sheet": "cursors/anim/trail.png", "cell": 16, "sprites": [[0, 0], [16, 0], [0, 16]], "count": 96 }
}
```

The window swaps the system cursor's picture itself, so frames show
wherever the pointer is on the window, at the screen's scale. Each frame is
decoded the first time it shows, at the one size the scale needs, and made
into the system's cursor once, then reused. A timer wakes Yapfast at the
next frame's time only while one plays: with the pointer still and no idle
loop (or the pointer off the window), nothing runs. The trail is a layer
over the window that takes no input, moved only while some of it is alive:
60 times a second in Full, 30 in Balanced, however often the pointer
reports a move. Reduced motion and Light show each kind's still
picture (an animated kind's first frame) and no trail; Balanced and Full
play them. `theme check` decodes every frame and the sheet, refuses a
hotspot off its frame, a sprite off the sheet, or more than 1024 frame
files, and counts their decoded size in the cost. Every frame is a file of
the theme's, each size too: an install from a URL takes at most 256 files
(see Distribution), and finds those a pattern names as the window does.
Sheets keep a large set under that: each holds a sequence, or a whole
motion set.

Settings → Themes → Installed → *Cursors* → *Use the theme's cursors* (on)
turns them off for the theme shown, kept in `layout.json`'s
`theme_cursors_off`. A protected screen shows the system's cursors. Cursors
cost their pictures decoded, every size, and nothing per frame: the window
hands one to the system when the kind under the pointer changes.

Underneath, Slint's core (`vendor/i-slint-core/yapfast_cursors.rs`) asks
`src/ui/theme_cursor.rs` for a picture before it gives the window a named
cursor, and gives it a custom one in its place, which Slint's winit backend
shows as an image cursor on Windows, X11, Wayland and macOS alike. Animated
frames go round Slint: `theme_cursor::Handler`, a winit application handler
registered with the backend, sees the window's pointer events first and,
after each turn of the event loop, hands winit the frame that shows.

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
  as over a surface's art. The same object may draw a gel, as a glossy
  button's, in shapes rather than pictures, so it stays crisp on a
  one-line bubble and a tall one alike; each part is optional and nothing
  is drawn unless set:
  - `gloss`: a rounded band over the bubble's top, its `colour` (white
    unset) fading from `from` to `to` (alphas, 0.6 and 0.1) downward and
    ending in a clean edge. `height` is a share of the bubble's height
    (0.1–0.8, 0.5), never more than `max` pixels (4–96, 24); `inset`
    (0–16, or `[across, down]`; 2 and 1) keeps it in from the sides and
    the top, and `radius` sets its corners (the bubble's less the inset).
  - `glow`: a light rising from the bottom edge, as through a gel:
    `colour` at the edge (with its alpha), gone `height` pixels up (2–48,
    12).
  - `rim`: `light`, a 1-pixel line just inside the edge, brightest along
    the top and again, fainter, along the bottom; `dark`, a line on the
    edge.
  - `shadow`, as `shape` writes one, under the bubble.

  The gloss and the glow are held to the text the way a ground is: at
  the most of them that reaches the first line, past the bubble's top
  padding, on the tallest band `max` allows (and the glow at the last
  line), over each colour of the ground, the text has to read at 4.5:1.
  A bright gloss that should stay clear of the text ends above it: a
  `max` under the padding, which `padding` (4–24 pixels, or `[across,
  down]`; 12 and 6 unset) can widen. A reply's quote, "(edited)" and
  "Forwarded" on your own bubble take its text's colour. The bar of actions over a message
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
  "messages": "bubbles",
  "bubbles": { "padding": [14, 9],
               "own": { "ground": "linear(180deg, #1d62d6, #0a3f9e 55%, #1559c8)",
                        "gloss": { "from": 0.55, "to": 0.08, "max": 9, "inset": [4, 1] },
                        "glow": { "colour": "#7cc8ff55", "height": 8 },
                        "rim": { "light": "#ffffffb0", "dark": "#0a3478" },
                        "shadow": { "blur": 6, "y": 2, "colour": "#0b3a8040" } },
               "own_text": "#ffffff" }
}
```

```json
"layout": {
  "rail": "top", "messages": "irc", "chat_avatars": false, "chat_max_width": 960,
  "irc": { "time_width": 56, "nick_width": 120, "separator": "│" }
}
```

**People's content** (`content`): how the theme draws what people bring —
their pictures, avatars, names, profiles, embeds and activity — rather than
Yapfast's own surfaces. It shows only when the person lets it: Settings →
Themes → Installed → *Let the theme style people's profiles and media*, and
the same switch in the install review. It's off unless turned on, and kept for
each theme (`theme_content` in `layout.json`); off, everyone's pictures,
colours and cosmetics show as they made them. Every field is optional, and
`keep` draws that thing as stock. `crates/yapfast-core/src/themes/content.rs`
reads it; `src/ui/theme_content.rs` and the `ThemeContent` global
(`ui/theme_content.slint`) draw it.

- `media`: people's pictures — avatars, server icons, emoji pictures in the
  picker and on reactions, stickers, attachments, embed thumbnails, link
  previews, GIFs and spoilers' blurs. `"normal"`, `"mono"` (greys),
  `{ "duotone": { "dark": "#03110a", "light": "#5cff85" } }` (each pixel's
  luminance mapped from `dark` to `light`, as a two-ink print) or
  `{ "tint": { "colour": "#ff71ce", "amount": 0.35 } }` (washed towards the
  colour by `amount`, 0–1, keeping light and shade). Pictures are recoloured
  once, as they're handed to the interface, through a 256-entry table; an
  animation's frames are recoloured on its decoding thread as they're
  decoded. Nothing is redone per frame. While `media` is `normal` a picture is
  shared with its decoder as stock; recoloured, each picture showing is held
  once more, as its copy, within the same bounded caches. Emoji drawn inside
  message text are glyphs of a font made as they arrive, and Twemoji is
  Yapfast's own art: both stay as they are.
- `avatars`: `shape` — `"keep"`, `"circle"`, `"square"`, `"rounded"` (8
  pixels) or `{ "rounded": 6 }` (0–24 pixels at a 40-pixel avatar, scaled
  with its size) — for people's avatars, not servers' icons;
  `decorations` — `"keep"` or `"hide"` (neither drawn nor fetched);
  `ring` — `{ "colour": "#5cff85", "width": 2 }` (1–4 pixels), a ring in
  the theme's colour around every person's avatar in place of decorations;
  and `mode` — `"picture"` (stock) or `"initial"`: illuminated initials,
  the first letter of the person's display name, as a capital, in place of
  their avatar beside messages. A name that starts with something without
  case, such as an emoji, CJK or a symbol, keeps its picture. `initial`
  shapes it: `font`, the type role whose face draws it (`headings` unset,
  so a theme's blackletter headings make blackletter initials); `colour`
  (the theme's `accent-text` unset), which has to read 3:1 on its ground;
  `size` against the avatar's (0.4–1.2, 0.8 unset); `ground` and `border`
  (`{ "colour", "width" }`, 1–4 px), cut to the avatar's shape; `hover`,
  `"picture"` (the default: the avatar shows while the pointer is on the
  message) or `"initial"`; and `members`, `true` for initials in the member
  list too. Profile cards always show the picture.
- `names`: `colours` — role colours as the server gives them (`"keep"`), every
  name in `colour` (`"theme"`; the theme's `text` when `colour` isn't set,
  which has to read 4.5:1 on `chat`, `sidebar` and `members`), or each
  keeping its hue with its saturation and lightness pulled into
  `saturation` and `lightness` (`[low, high]`, 0–1 each; muted mid tones
  that suit the theme's darkness unset) and then lightened or darkened until
  it reads 4.5:1 on `chat` (`"clamp"`); `styles` — display name styles
  (fonts, gradients, glows) `"keep"` or `"theme"` (the theme's names
  type); `nameplates` — `"keep"`, `"hide"` (neither drawn nor fetched) or
  `"tint"` (the plate's picture as a shape in `accent`, over an `accent`
  wash).
- `profiles`: `themes` — profile themes, Nitro colours and the banner colour,
  `"keep"` or `"theme"` (the card is drawn on the theme's own card surface
  and art; a banner picture still shows, recoloured as `media` says);
  `effects` — profile effects `"keep"` or `"hide"`.
- `embeds`: `"keep"` or `"theme"`: the embed's and a component card's colour
  bar in `accent`, and the links in embeds in `accent-text`.
- `activity`: `"keep"` or `"theme"`: activity and game pictures take the
  theme's card corners and outline, a song's progress bar `accent`, and the
  LIVE pill `accent` with `on-accent` text.

```json
"avatars": { "mode": "initial", "initial": { "font": "headings", "colour": "#9e263b",
             "size": 0.86, "hover": "picture", "members": true } }
```

Protected dialogs, danger colours and reduce motion are as without it. `theme
check` lists what `content` changes in the cost line, as `content (if
allowed): duotone pictures, square avatars, …`.

```json
"content": {
  "media": { "duotone": { "dark": "#03110a", "light": "#5cff85" } },
  "avatars": { "shape": "square", "decorations": "hide" },
  "names": { "colours": "theme", "styles": "theme", "nameplates": "hide" },
  "profiles": { "themes": "theme", "effects": "hide" },
  "embeds": "theme", "activity": "theme"
}
```

**Chrome** (`chrome`): the fixed words and small marks around the window,
so a theme can read as a terminal, a ledger or a zine. It is plain text
from closed lists: nothing is read as markup, labels are at most 48
characters on one line (no control characters or marks that turn text
around), and a label takes only the placeholders listed for it; any other
`{word}` refuses the theme. Screen readers keep hearing the stock names and
labels, and every row keeps its keyboard focus, menu and truncation (a long
name ends in an ellipsis). Text runs in the reader's direction, right to
left included.

| Field | What | Stock |
| --- | --- | --- |
| `labels` | Words in place of fixed strings (below) | |
| `prefix`, `suffix` | Marks before and after names, at most 8 characters each, by place: `channel` (a channel's row), `server` (a server in the text rail), `member` (a member list row), `author` (a message's author), `header` (the channel's name over the messages) | none |
| `time` | A pattern around a message's time, holding `HH:MM` where it goes, such as `[HH:MM]`, at most 16 characters; the time follows the person's clock setting | `HH:MM` |
| `hide` | What to leave out (below) | none |
| `rail` | `mode`: `icons`, or `text` for the servers' names as rows of text; `width`: 96–320 pixels of the rail in text mode | `icons`, 150 |
| `composer` | `caret`: `line` or `block` (as wide as a letter); `prompt`: at most 4 characters drawn before the text, such as `$` (24 in the console style); `style`: `box` or `console` (below) | `line`, none, `box` |
| `font` | The type role whose face the chrome's words take where a theme sets them: its labels, the status column, the text rail's lines, the top bar and the console composer — `code` for monospaced `//` labels | each place's own |
| `topbar` | A thin bar across the window's top (below) | none |
| `folders` | Server folders in the text rail and channel categories as directories: `open` and `closed`, the mark before an open or closed one, and `suffix`, after its name, at most 4 characters each, such as `▾ studio/` and `▸ games/`. Clicking still opens and closes it | `▾`, `▸`, `/` once set |
| `tree` | `lists`: lists drawn as file trees, as the `tree` command prints them (below); `colour`: their lines' colour, `faint`, `muted`, `secondary`, `text`, `accent` or a colour such as `#2e7d45` | none, `faint` |
| `members` | `columns`: a status column at the end of each member row, by presence: `online`, `idle`, `dnd`, `offline`, `streaming`, at most 12 characters, `{n}` the row's place in its group from 0 | none |

| Label | Replaces | Placeholders |
| --- | --- | --- |
| `members_title` | A line over the member list, such as `$ who` (none stock) | |
| `members_group` | Each group heading, `Online — 3` | `{role}`, `{n}` |
| `category` | A channel category's heading | `{name}` |
| `composer` | The composer's hint in a channel, `Message #general` | `{channel}` |
| `new` | The "New" divider's word | |
| `typing` | Who is typing, `Sam is typing...` | `{name}` (one name or several joined), `{n}` |
| `welcome` | The big line at a channel's start, `Welcome to #general!` | `{channel}` |
| `start` | The small line at the start of a channel's history | `{channel}` |
| `rail_title`, `rail_home` | In the text rail: a line over it, and the home row's words (`Direct messages`) | |
| `rail_footer` | In the text rail: a line under the servers while one is open | `{server}` |
| `tree_root` | A root line over a tree of channels, such as `~/{server}` | `{server}` |
| `reaction` | A reaction's count while `reaction-chips` are hidden | `{n}` (stock `(+{n})`) |

| `hide` | What goes | What stays |
| --- | --- | --- |
| `member-avatars` | Avatars in the member list | The name, its colour and the status column |
| `channel-icons` | The icon before a channel's name, in the list and over the messages | The channel's name and its accessible name |
| `header-buttons` | The icon buttons and the search box over the messages, at rest | They show while the pointer is on the bar, and each shows while it has the keyboard; Tab reaches them as stock |
| `server-icons` | Servers' icons in the rail: the same as `rail.mode: "text"` | Every server, folder, unread DM, mention count and menu, as text rows |
| `account-avatar` | Your avatar in the account panel | Your name and status, which open the status menu |
| `activity-cards` | The Activity cards over the member list | Each person's activity under their name |
| `reaction-chips` | Reactions' chips: each is its emoji then its count as text, such as `(+3)` | Clicking, the tooltip of who reacted, and the screen reader's label |

The **top bar** (`topbar`), as a tiling desktop's or a terminal's, runs
across the window's top above everything else, `height` (18–40, 26 unset)
pixels tall with its words at `size` (9–16, 12 unset) in `font`'s face
(monospaced unset). On the left are `workspaces` (0–10, 5 unset): the
first servers in the rail's order, numbered from 1, or with `from:
"folders"` each folder of the rail and each server outside one. The open
one is in `current` with an `underline`, an unread one bold. Clicking one
opens it (a folder opens its open server, or its first); each is a
button Tab reaches and Enter or Space presses, named for its server. In
the middle `centre`, and at the right `right`, are labels taking
`{name}` (the account's name), `{server}` (the open server's name),
`{channel}`, `{now_playing}` (what the account listens to on Spotify, as
`title · artist`, empty when nothing) and `{time}` (the person's clock
setting, kept each minute only while a label shows it). `effect:
"decrypt"` scrambles `{now_playing}` into place when it changes (not under
reduced motion or Light). Its colours take `ground` (`rail` unset),
`colour` (`muted`), `current` (`text`), `underline` (`accent`) and
`line`, the hairline under it (`line`); on the ground `colour` has to
read 3:1, as `muted` does, and `current` 4.5:1.

The **console** composer (`composer.style: "console"`) has no box: a
hairline (`line`, the theme's `line` unset) lies under the field, and
while it has the keys a second, `focus_line` (`accent`), draws across it
from the left over `draw` milliseconds (0–2000, 420 unset; at once under
reduced motion). The prompt, the placeholder and the text take the
chrome's face (monospaced unset); the placeholder types itself, `typing`
milliseconds a letter (0 for never, 38 unset), each time the empty field
takes the keys; and `caret_colour` paints the caret, which blinks as the
stock one does. Every button, the reply bar, files waiting to go and
their progress stay as stock.

```json
"chrome": {
  "font": "code",
  "topbar": { "workspaces": 6, "centre": "{name}@{server}", "right": "{now_playing}  {time}",
              "underline": "#9e263b", "effect": "decrypt" },
  "composer": { "style": "console", "prompt": "guest@hurt:~$", "caret": "block",
                "caret_colour": "#9e263bcc", "focus_line": "#9e263b" }
}
```

A tree draws `├──` before each row of a list and `└──` before its
group's last, in the code font, with `│` carrying on past rows nested under
a row that isn't last. Each connector belongs to its row, so it follows the
row's height and indent and never covers its text, which still ends in an
ellipsis; screen readers skip connectors. Rows keep Yapfast's left-to-right
order, so a right-to-left name reads in its own direction after its branch.

| `tree.lists` | What |
| --- | --- |
| `channels` | Channels under their categories (with `folders` each category reads as a directory; without, give `category` a label such as `{name}/`, and a closed one ends in `…`), threads and who is in a voice channel nested under the channel, and `tree_root` over the list. A thread's line looks past at most three threads to the next channel |
| `members` | Members under their groups (`members_group` such as `{role}/`) |
| `dms` | The direct messages list |
| `rail` | The text rail: servers, with a folder's servers nested under it |
| `replies` | A reply's quote after `└─>`, without the curved spine or the quoted avatar, and a thread under its first message as `├─ thread: name` instead of a card |

In the text rail each server is a row of its name, the open one in `accent`
with `on-accent` text, an unread one bold, mentions after it as `(3)`, a
call as `*`, a folder as `+ name` closed and `- name` open (or as `folders`
says) with its servers indented under it, or on the tree's branches. Its keys, menus and tooltips are the stock rail's.

```json
"chrome": {
  "labels": { "members_title": "$ who", "members_group": "{role} — {n}",
              "composer": "message #{channel}", "new": "-- new --", "typing": "{name} is typing",
              "rail_title": "~/servers $ ls", "rail_footer": "~ $ cd {server}" },
  "prefix": { "channel": "./", "author": "<", "header": "#" },
  "suffix": { "author": ">", "server": "/" },
  "time": "[HH:MM]",
  "hide": ["member-avatars", "channel-icons", "header-buttons", "account-avatar", "reaction-chips"],
  "rail": { "mode": "text", "width": 150 },
  "composer": { "caret": "block", "prompt": "$" },
  "members": { "columns": { "online": "pts/{n}", "idle": "idle", "dnd": "busy", "offline": "off" } },
  "tree": { "lists": ["channels", "members", "rail", "replies"], "colour": "faint" },
  "folders": { "open": "▾", "closed": "▸", "suffix": "/" }
}
```

**Parts.** A later Yapfast lets a theme rebuild a few named parts —
`message-row`, `member-row`, `account-panel`, `server-header`,
`profile-card` — from a widget tree. Each part will have a contract version,
the fields it receives; minor versions only add fields, and a part built for
another major version draws the stock part with a note on the theme's page.
This Yapfast offers none yet: a theme that declares `parts` shows the stock
parts, and its page says it needs a later Yapfast.
