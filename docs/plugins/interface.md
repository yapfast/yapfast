# Plugins

Plugins are behind the `plugins` Cargo feature, which is on by default, so a
plain build carries them. This document is the interface's side of the system:
reading and checking what a plugin's manifest changes in Yapfast's own
interface, and what a running plugin draws in it
([`settings.md`](settings.md) is the Plugins page).

## Trying a plugin

Installing a plugin from a repository is what ships: Settings → Plugins has a
marketplace and *Add from URL*, the page's request turns into
`yapfast-plugins fetch`, which is the helper's other subcommand, and what it
reads is reviewed, installed and then turned on from the Installed list
([the marketplace](marketplace.md)). The Advanced tab also installs a local
plugin from its `.wasm` file through the system's file picker, or loads a local
plugin repository through its folder picker. The latter is the development
loop: the plugin is enabled and reloads when its manifest or plugin changes.

Turning a plugin on, off, updating or removing it changes the window at once:
the application notices that the enabled plugins changed (a hash of their ids
and installs, checked when a status arrives, so a plugin going idle reads no
manifest), reads the enabled manifests from the store, and builds their colours,
parts, buttons and names again. What the plugins still on drew stays drawn.

For trying a manifest without installing it, `YAPFAST_PLUGINS_DIR` names a
folder that a build with plugins reads beside the store's plugins, ahead of
them: one subfolder each, with its
`yapfast-plugin.json`, in the order of the subfolders' names, which is the order
their changes win in. Nothing of it is installed and no module of it runs, so it
is the loop for trying a manifest's changes of Yapfast's own interface, and a
`slots.json` beside the manifest for trying what a plugin would draw. A manifest
that can't be used is skipped and logged, and so is each change refused; run
with `YAPFAST_LOG=info` to see them.
[`testing/plugins`](../../testing/plugins) has examples:

```sh
YAPFAST_PLUGINS_DIR=testing/plugins cargo run --profile fast -- --preview
```

`yapfast --no-plugins` starts in safe mode: no plugin's changes apply, and
which plugins are enabled stays as it was. It's the way back from an
interface a plugin made unusable. A restart, such as after the GPU fails,
keeps it.

## The manifest's interface changes

The iced window applies everything below: the menu rows a manifest adds, its
rail items and its header buttons, and `ui`'s colours, its hidden and replaced
parts, its hidden, relabelled and moved menu rows, and every size, with the
limits a theme has ([In the window](#in-the-window)).

[`crates/yapfast-plugins/src/lib.rs`](../../crates/yapfast-plugins/src/lib.rs)
reads a manifest, `yapfast-plugin.json`. It's strict JSON, and must name
`"api": 1`, a reverse-DNS `id` and a `name` of up to 48 characters, on one
line, with no character that hides or turns text around, and not spelling
"Yapfast" (`crates/yapfast-plugin-api/src/text.rs`). The
interface reads only `id`, `name`, `wasm`, `permissions`, `menus`, `rail_items`,
`header_buttons`, `ui`, `settings` and `tweaks`, and leaves the rest of the
manifest to the runtime. Inside `menus`, `rail_items`, `header_buttons`, `ui` and
each setting a field Yapfast doesn't know is an error, so a misspelt change fails
loudly. `wasm` is optional: a plugin
without it only changes what its manifest names. Its own settings, the
`settings` array, are in [`settings.md`](settings.md#a-plugins-own-settings).

```json
{
  "api": 1,
  "id": "com.example.quiet",
  "name": "Quiet",
  "permissions": { "capabilities": ["ui.modify"] },
  "menus": [
    { "target": "user", "label": "Wave", "action": "wave", "when": { "network": "discord" } }
  ],
  "ui": {
    "menu": [
      { "target": "user", "row": "copy-id", "label": "Copy user ID", "before": "mention" },
      { "target": "server", "row": "hide-muted", "hide": true }
    ],
    "parts": { "hide": ["timeline.typing", "message.actions.react"] },
    "colors": { "accent": "#7aa2ff" },
    "sizes": { "message-group-gap": 8 }
  }
}
```

[`plugins/ui.rs`](../../crates/yapfast-plugins/src/ui.rs) checks each
change with `check`, which keeps what applies and says why it refused the
rest, in words a plugin's page can show:

- **`ui` needs `ui.modify`.** Without the capability, nothing in `ui`
  applies. Rows and buttons added through `menus`, `rail_items` and
  `header_buttons` need no capability.
- **Rail items and header buttons** are the plugin's own: a `label` of 1 to 64
  characters and an `action`, drawn at the foot of the server rail
  ([`rail.rs`](../../crates/yapfast-iced/src/rail.rs)) once the iced window
  draws them, and at the end of the conversation's header
  ([`header.rs`](../../crates/yapfast-iced/src/header.rs)). Choosing one is a
  menu row's path exactly: the action goes to the runtime as `Input::Menu`,
  with the open conversation as its target.
- **Menu rows** are named by `target` and the row's id. `target` is a closed
  list (`message`, `user`, `channel`, `thread`, `group_dm`, `server`,
  `server_header` and the rest), and Yapfast applies the rows of a menu it has.
  Only rows in `EDITABLE_ROWS` change, so a new
  built-in row is protected until it's listed there. Rows drawn in the
  danger tone (`DANGER_ROWS`: Block, Ban, Leave Server and the others),
  rows that show or control a call, and the friend and block rows aren't
  listed. A row is hidden, relabelled, or moved `before` or `after` another
  editable row; not both. Labels are plain text on one line, 1 to 64
  characters.
- **`when`** is a closed set of fields, all of which must match: `network`
  (`discord` or `matrix`), `own`, `dm`, `has_attachment`, `is_bot` and
  `role`, a role id the person the menu is about has.
- **Parts** are a closed list of IDs. `sidebar.banner`, `timeline.typing`,
  `message.actions.reply`, `message.actions.react` and
  `message.actions.more` can be hidden. `sidebar.banner`,
  `account.identity` and `timeline.intro` can be replaced, which needs
  `wasm`, as the plugin draws the replacement. An ID that no longer exists
  is skipped with *this part no longer exists*; one naming something no
  plugin can touch, such as `composer.send` or anything under `call.`, is
  refused as such.
- **Colours** are the token names of `ui/theme.slint` (`COLOURS`).
  `danger`, `danger-hover` and `on-danger` can't be set, and a text colour
  must be opaque. Readability depends on the theme, so it's checked when
  the colours are laid over one; see [Colours](#colours).
- **Sizes** are a closed list in logical pixels, each clamped to a range:
  `text-size` 12–20, `sidebar-width` 200–420, `members-width` 200–360,
  `channel-row-height` 24–48, `message-gap` 0–12, `message-group-gap` 0–32.
- **Caps:** 64 changes to built-in UI and 64 added rows per plugin. One
  plugin changing the same thing twice keeps the first.

`derive` builds one `Option<Arc<UiOverrides>>` from the enabled plugins'
checked changes, in order. It's `None` when no plugin changes anything, so
the interface tests one `Option` where a change could apply. Two plugins
hiding the same part, or setting a colour or size to the same value, agree.
Any other overlap is a `Conflict`: the first plugin keeps the change unless
the choices passed in name the other, and the plugin that loses keeps its
other changes. Nothing stores those choices yet; `grants.json` will.

## Setting points

A setting point is a behaviour of Yapfast a manifest switches by name, in
`tweaks`, with no code: `{ "tweaks": { "messages.attachments.mosaic": false } }`.
[`tweaks.rs`](../../crates/yapfast-plugins/src/tweaks.rs) is their registry: the
`points!` list in it declares each point once, as a field of `Tweaks` with
its type (`bool`, or a whole number in a range), its default and a doc
line, and the point's name. Code reads the field where the behaviour
applies.

`check` keeps a manifest's points that the registry lists with a value of
their type, up to 64, and refuses the rest with the reason: a point this
Yapfast doesn't have, perhaps one a newer one does, or a value of the wrong
type. `derive` lays the enabled plugins' points over the defaults, in their
order, into one `Option<Arc<Tweaks>>`, `None` while no plugin sets a point,
so nothing is held for them otherwise. Two plugins setting a point to the
same value agree; to different values they're a `Conflict` naming both
plugins, and the first keeps it unless the choices passed in name the
other. The window logs refusals and conflicts as it loads plugins.

Points are added one at a time, as a behaviour is worth exposing, and
Yapfast lists none yet, so every point a manifest sets is refused as
unknown.

## In the window

[`state.rs`](../../crates/yapfast-core/src/plugins/state.rs) holds a session's
plugins as `Option<Box<PluginState>>`: `None` without any, so no model, timer
or thread exists for them. The window reads what they want through the
application, and a change applies where Yapfast already builds that part:

- **Menus.** A menu is built in
  [`menus.rs`](../../crates/yapfast-iced/src/menus.rs),
  [`place_menus.rs`](../../crates/yapfast-iced/src/place_menus.rs) and
  [`person_menu.rs`](../../crates/yapfast-iced/src/person_menu.rs) as rows,
  then `plugins_ui::edit_rows` hides, relabels and moves the rows the
  manifests name. A row is found by `row_name`, which names only the rows in
  `EDITABLE_ROWS`, so a danger row, a call row or a friend row is never
  touched; a relabelled row says *Renamed by the Name plugin* under its label,
  and a row moved before or after one this menu doesn't have stays where it
  was. Plugins' own
  rows follow, under a line, after the built-in rows and before a last
  group holding a danger row. Each shows its plugin's name under its label,
  so no plugin's row passes for Yapfast's own, and its id is
  `plugin/<index>/<action>`, by the plugin's place in `UiOverrides`. A
  server's menu is `server`, its header's dropdown `server_header`; a
  person's, someone's in a call, and a one-to-one DM's menu are `user`; a
  group DM's is `group_dm`, a thread's `thread`, and any other channel's
  `channel`. A message's menu is `message`. A running plugin also adds rows
  from what it draws in the `menu.message` and `menu.user` slots
  ([Slots](#slots)).
- **Choosing a plugin's row.** `choose` checks for the `plugin/` prefix
  before anything else and sends `Action::PluginInput(Input::Menu)`, carrying
  the row's target and its action. No `Effect` carries it back: `plugin_input`
  in [`application.rs`](../../crates/yapfast-core/src/application.rs) hands it
  to `PluginState::deliver`, which sends `Request::Input` down the pipe, and
  the helper activates the plugin if it was evicted, then calls `yf_input`
  with it ([the ABI](abi.md)). Nothing answers, so nothing waits on it: a
  plugin that wants to be seen draws again or calls a host method.
- **Parts.** [`ui.rs`](../../crates/yapfast-plugins/src/ui.rs) names the parts
  that can be hidden, and each is drawn behind a test of
  `plugins_ui::hides`: the server banner over the channel list (the header is
  then as tall as without one), who's typing, and the hover bar's Reply, Add
  Reaction and More. Shift's Delete takes More's place whether or not More is
  hidden. A part a manifest replaces is drawn by its plugin, through the slot
  of the part's own name (`sidebar.banner`, `account.identity`,
  `timeline.intro`), in the box Yapfast's own part takes, so nothing around it
  moves; until the plugin draws, Yapfast's own part shows. The banner and the
  welcome carry the small *Name plugin* line over what it drew, and the
  account's name, which has no room for one, says so in its tooltip; the
  picture, the status menu and the call buttons beside it stay Yapfast's, and
  a DM's welcome, which holds Block, isn't replaced.
- **Sizes.** `text-size` is the timeline's text size, in place of the one
  Messages chose, and the rows are measured again when it changes.
  `sidebar-width` and `members-width` are the sidebar's and the member list's
  widths before the person drags them, and what a double click returns them
  to (`Layout::plugin_sidebar_width`, kept by the window and never written to
  `layout.json`). `message-group-gap` is the gap above a group of messages,
  and `message-gap` the gap between two messages of a group, half above and
  half under each, in measuring and in drawing. `channel-row-height` is a
  channel row's height in the sidebar, a thread's line included; a DM's row
  keeps its own.
- **Links.** A web link the person clicks on a host some plugin's `link-hosts`
  lists asks those plugins (`link_open`, [the ABI](abi.md)), and the leaving
  dialog shows even for a plain link, so the person chooses: each offer is a
  button beside Visit site with *Offered by the Name plugin* beside it, and
  opens the address the plugin gave, whose scheme its manifest lists. A link
  no plugin is asked about opens as it always did.
- **The quick switcher.** A plugin with `switcher.rank` is asked about each
  search, and the list takes its order when it answers, with *Sorted by the
  Name plugin* under it.
- **The title.** `window.set_title`'s words go after Yapfast's own title,
  after a dot.
- **A plugin's sound.** While a plugin's `audio.play` sound plays, a line over
  the conversation says *The Name plugin is playing a sound*.
- **Colours.** [`plugins_ui.rs`](../../crates/yapfast-iced/src/plugins_ui.rs)
  keeps the person's theme and the same theme with the plugins' colours laid
  over it (`Looks`), made again when the theme or the plugins change, so they
  follow every change of theme and the theme editor's preview. A colour that
  would bring text under 4.5:1 on a surface it's drawn on is left out
  ([Colours](#colours)) and logged.
- **Protected screens.** Signing in and its security question, a plugin's
  question about a site, the question before following a link, and Settings →
  Plugins show the person's theme without any plugin's colours
  (`plugins_ui::protected`), so no plugin can restyle the screens that decide
  what it may do, or hide the switch that turns it off. The theme shown is
  worked out after every update, and swapped only when what shows changes.
- **Turning them off.** A plugin's page lists each change it makes to
  Yapfast's look in words, and each refused with why, under *Apply its changes
  to Yapfast's look*. Off (kept in `layout.json` as `plugin_looks_off`), the
  plugin keeps running and drawing in its slots, and its colours, sizes and
  hidden parts go. `yapfast --no-plugins` still turns every plugin's off.

### Colours

`unreadable` takes the theme a person chose and a plugin's colours, and
leaves out any colour that would bring text under 4.5:1 against a surface
it's drawn on (`TEXT_ON`): `text`, `secondary`, `muted`, `faint` and `link`
on the sidebar, panel, chat, raised and field surfaces, and `on-accent` on
`accent`. A surface colour can be refused the same way. Where a theme's own
pair is already under 4.5:1, as muted text often is, the overlay only has
to keep the theme's own contrast; 4.5:1 throughout would
refuse every muted colour on those themes.

### Widget trees

What a plugin draws in a slot is a `Tree`: rows of cells, and a few kinds of
cell hold rows or cells of their own. It is data, never code: a plugin says
what to show, and the window draws it with the theme's tokens and the kit's
widgets, so a theme styles a plugin's drawing as it styles Yapfast's.

| Kind | What it draws | Its fields |
| --- | --- | --- |
| `text`, `muted`, `heading` | Markdown: bold, italic, strikethrough, inline code and links | `text` |
| `title` | A dialog's or page's title, 20 px bold | `text` |
| `caption` | A group's heading as Settings draws one (THEME, PROFILES): 12 px bold capitals in the muted ink, whatever case it was written in | `text` |
| `code` | Monospaced text on the sunken ground | `text` |
| `badge` | A small mark in its tone | `text`, `tone` |
| `icon`, `image` | A picture, 16 px square or up to 240 × 135, round when `round` | `src`, `text` while it isn't there, `round` |
| `settings_form` | The manifest's settings form, drawn and saved as the plugin's page draws it, where this row is: only on the plugin's own settings page (`settings.page`), nothing anywhere else | `options`: the settings' keys it shows, every shown one when empty |
| `tile` | A picture over its label in one card of a fixed width (96 px unless `width`), so a grid of them lines up; a button when it has an `id` | `src`, `text`, `id`, and the picture fields below |
| `button`, `link` | A button, or a link's words | `id`, `text`, `enabled`, and a button's `icon`, `variant` and `small` |
| `toggle` | The kit's switch, its label before it; with a `description`, Settings' own switch row: the label, the description under it, the switch at the row's end, and a click anywhere on the row flips it | `id`, `text`, `checked`, `description` |
| `empty` | What a list or page shows with nothing in it, centred: one of Yapfast's icons (`icon`, 40 px and faint), what isn't there (`text`) and what to do about it (`description`) | `icon`, `text`, `description` |
| `item` | One row of a list as Settings draws one: a picture (`src`, 32 px with a `description`, else 24; round when `round`), its name (`text`) with the `description` under it, and its `cells`, its actions, at the end, a button among them drawn small; with an `id` the whole row is pressed, lit while `selected` | `src`, `round`, `text`, `description`, `cells`, `id`, `selected` |
| `setting` | A setting's row as Settings draws one: `text` (15 px) with `description` (13 px, muted) under it at the start, and its `cells`, the controls, at the end, a button among them drawn small | `text`, `description`, `cells` |
| `select` | Its options as a row of buttons, the chosen one lit | `id`, `options`, `value` |
| `segmented` | Settings' own row of choices: its options side by side on the sunken ground, the chosen one raised and bold, for two to four short options worth seeing at once; its label over it when `text` has one | `id`, `options`, `value`, `text` |
| `dropdown` | The kit's drop-down, its label over it; with `icons`, a round picture before each option's name (a server's icon, a person's avatar), one `src` per option by its place, an empty one none | `id`, `text`, `options`, `value`, `icons` |
| `input` | A one-line field | `id`, `text` (its placeholder), `value` |
| `textarea` | A field of four lines, its label over it | `id`, `text`, `value` |
| `number` | A field with − and + beside it | `id`, `text`, `value`, `min`, `max`, `step` |
| `color` | A swatch, the `#rrggbb` field and eight colours to pick | `id`, `text`, `value` |
| `slider` | The kit's slider, showing the plugin's own number | `id`, `text`, `value`, `min`, `max` |
| `progress` | A bar, its label over it | `text`, `value`, `min`, `max`, `tone` |
| `spinner` | Eight dots turning, beside its label | `text`, `tone` |
| `list` | Its items, numbered when `ordered`, each markdown | `items`, `ordered` |
| `table` | Its column names over its rows, each cell markdown | `header`, `table` |
| `grid` | Its cells in `columns` (1 to 8) of equal width | `cells`, `columns` |
| `tabs` | Its tabs' labels, and the chosen tab's rows | `id`, `tabs`: `{ "label", "rows" }` |
| `section` | A heading that folds its rows open and shut | `id`, `text`, `rows`, `collapsed` |
| `divider`, `spacer` | A line, or a gap | |

A button's `variant` is its face, as Yapfast's own buttons have them:
`primary` (the accent, for the one main action of a dialog or page),
`secondary` (the neutral face, and what a button without one has), `danger`
(red, for what deletes or can't be undone), `ghost` (no face until the pointer
is on it, as a list row's Remove), `link` (the accent's words) or `success`. A
`small` button is 28 px tall rather than 36, as Settings' buttons in a row are.

A button may name one of Yapfast's own icons in `icon` — `previous`, `play`,
`pause`, `next`, `stop`, `volume`, `volume-off`, `music`, `star`, `bell`,
`pin`, `link`, `download`, `refresh`, `search`, `settings`, `inbox`, `users`, `user`, `message`, `image`, `clock`, `eye`, `eye-off`, `shield`, `file`, `sticker`, `gif`, `trash`, `copy`, `pencil`, `plus`, `check`, `close`, `mic`, `hash`, `palette`, `history`, `ban`, `at`, `calendar`, `globe`, `smile` or `forward` — and is drawn as
an icon button with its `text` as what the tooltip and a screen reader say, so
it looks like the buttons beside it; a name this build doesn't have draws the
text. In the voice panel each button is a tile of the panel's own row, its
tooltip naming the plugin (*Next song · Controls plugin*).

Any cell takes a `tooltip`, plain text shown when the pointer rests on it, and
`end`: the row's cells with `end` sit together at its end, in their order, and
the rest wrap from its start, so a dialog's Cancel and Delete sit at its right
as Yapfast's own do; in a grid, an `end` cell sits at its column's end. A
cell's `tone` is `normal`, `muted`, `accent`, `success`, `warning` or `danger`,
never a colour. A row's cells sit side by side while they fit the slot and wrap
onto lines of their own when they don't, each at its natural width, so a plugin
needn't size a row for the narrowest window; a row holding a container, a table,
a list or a text area stacks its cells and gives each the row's width.

`Tree::checked_within` holds every tree to its slot's limits before anything
draws it:

- **Size.** A small slot (a toast, a decoration, a row of buttons) takes 64 rows
  and cells together; a banner, a dialog, a profile section, a picker's tab and
  a settings page take 256 (`Limits`). Rows, cells, tabs, a list's items and a
  table's cells each count as one.
- **Depth.** Containers nest four levels at most, the tree's own rows being the
  first: a section in a tab in a section, and no deeper.
- **Counts.** A `tabs` cell has 1 to 8 tabs, each with a label; a table or a
  grid has 8 columns at most; a select or a drop-down 64 options.
- **Ids.** Every interactive cell, and every `tabs` and `section`, has a plain
  id (letters, digits, `-`, `_`, `.`, up to 64).
- **Text.** Characters that hide themselves or turn a line around (N5,
  `yapfast_plugin_api::text`) are taken out of every text, value, item, cell,
  label and tooltip; a cell's text is cut to 4 KiB, a label or an option to 64
  characters and a tooltip to 256.
- **Values.** A `color` whose value isn't `#rgb`, `#rrggbb` or `#rrggbbaa`, and a
  `number` whose value isn't one, start empty rather than refusing the drawing.
  A cell holds only what its kind draws: rows given to a `text` are dropped.
- **Pictures.** `src` is `asset:<path>`, a picture inside the plugin's own
  package (`png`, `jpg`, `gif` or `webp`, no `..`, no absolute path), or an
  `https://` address. Anything else is cleared.

```json
{ "rows": [
  { "cells": [{ "kind": "heading", "text": "Daily digest" }, { "kind": "spinner", "text": "Syncing" }] },
  { "cells": [{ "kind": "tabs", "id": "pages", "tabs": [
    { "label": "Overview", "rows": [{ "cells": [{ "kind": "table", "header": ["Channel", "New"],
      "table": [["#general", "14"], ["#art", "3"]] }] }] },
    { "label": "Advanced", "rows": [{ "cells": [{ "kind": "number", "id": "retries", "text": "Retries",
      "value": "3", "min": 0, "max": 10, "tooltip": "How often a failed digest is tried again" }] }] }
  ] }] },
  { "cells": [{ "kind": "button", "id": "open", "text": "Open" }] }
] }
```

**Pictures.** An `asset:` picture is one of the plugin's own assets, named as
its manifest's `assets` names it, and the window reads it from the store the way
`assets.get` does (`Bridge::picture`): checked again as it is read, within the
asset caps, and only when it is a picture. A `YAPFAST_PLUGINS_DIR` preview has no
store entry, so its pictures are read from its folder, at most 1 MiB and never
through a link out of it (`plugin_slots::read_picture`). A picture from the web
is not fetched by the window: its cell keeps its room and shows its `text`.

**What the window keeps.** A drawing's [`Local`](../../crates/yapfast-iced/src/plugin_tree.rs)
is what the window holds of it between the plugin's draws, by plugin and slot:
what was typed into a field, a text area's editor, which tab shows, whether a
section is open and whether a drop-down is. A field shows what was typed at
once, and the plugin is told each keystroke; when the plugin draws again with
its own text for that field, its text shows from then on. Choosing a tab,
folding a section or opening a drop-down tells the plugin nothing.

## Decorating messages

A plugin that adds to how a message renders declares `decorate` in its
activation:

```json
{ "activation": { "on": ["decorate"] } }
```

When a row comes on screen, the parent asks the plugin for that row with
`yf_decorate`. The answer may carry four things, each optional:

| Part | What it does | Needs |
| --- | --- | --- |
| `tree` | A small card under the message, drawn from the widget set, under the plugin's credit line | `decorate` |
| `annotation` | One line under the text, such as a translation or a tag: `{ "text": "hello (French)", "tone": "muted" }`, at most 280 characters, in `normal`, `muted`, `accent`, `success` or `warning` | `messages.read` |
| `marks` | Styles over words the text already says: `[{ "text": "bonjour", "style": "highlight" }]`, at most 16, each naming 1 to 200 characters, styled `bold`, `italic`, `strike`, `highlight`, `spoiler` (covered until clicked, as a spoiler is), `link` (with `"url"`, an `http` or `https` address, which asks before it leaves as a masked link does), `wiggle` or `rainbow` (the letters bob, or run through the rainbow, 20 frames a second while the row is on screen, and still when pictures don't play); any mark may carry a `"tooltip"` shown while the pointer is on its words | `messages.read` |
| `pictures` | Small pictures set into the text, before (or with `"after": true` after) words it says or a person's mentions: `[{ "src": "https://cdn.discordapp.com/avatars/1/a.png", "mention": "1", "round": true }]` or `{ "src": "emoji:<id>", "text": "France", "after": true }`; `src` is a Discord CDN address (`cdn.discordapp.com`, `media.discordapp.net`, fetched through Yapfast's own session) or `emoji:<id>`; at most 16, each with an optional `tooltip` | `messages.read` |
| `tooltips` | A line shown while the pointer is on a link the message has, or a person's mentions: `[{ "link": "https://bit.ly/x", "text": "goes to example.com" }, { "mention": "1", "text": "Ada, the admin" }]`, at most 16 | `messages.read` |
| `emoji` | Emoji hidden from the text, or shown as something else: `[{ "emoji": "<custom id>" }, { "emoji": ":kekw:", "with": "lol" }, { "emoji": "\u2764", "with_emoji": "<custom id>" }]`; `emoji` is a custom emoji's id or `:name:` or a typed emoji; `with` is up to 32 characters, `with_emoji` another custom emoji's picture; at most 64 | `messages.read` |
| `hide`, `hide_media` | Keep the row, or its pictures and files, out of view | `messages.filter` ([below](#keeping-a-deleted-row-and-hiding-one)) |
| `blur_media` | Cover the row's pictures, blurred under the spoiler's cover, until the person clicks one, as BlurNSFW does; a room's `nsfw` (in `rooms.get`, `rooms.list` and `room_selected`) says the channel is age-restricted | `messages.filter` |

A bare widget tree is still an answer, so a plugin that only draws a card says
what it always said.

**What it can never touch.** The author's line, the time, the reply line and
Yapfast's own badges are drawn above the text, and nothing in the answer reaches
them: a plugin's line and card are drawn under the text, under one small credit
line, *Translator plugin*, with the plugin's id in its tooltip, and a mark
names words rather than offsets, so it styles what the message says and can't
say anything it doesn't. A mark never applies to a link, a mention, a custom
emoji, a spoiler or a code block, so it can't dress one up as another, and it has
no tone at all, so nothing a plugin styles looks like Yapfast's warning or
danger. A `link` mark's words always ask before they leave, since they aren't
the address, and a `spoiler` mark is counted apart from the message's own
spoilers, so revealing one never reveals the other. A message a plugin styled without adding a line says so under the text,
*Styled by Translator*. An annotation in a tone Yapfast keeps for itself, a mark
list past its cap, or text with a control or direction character is dropped
where it arrives and the rest of the answer still draws.

**What it costs.** Only the rows the person can see are asked about, a pass is
capped at a screenful (64 rows), and the row is drawn plain until the answer
comes, which is bounded at 16 ms in the guest. Each answer is kept by the
parent under the plugin, its version and module, the message and a hash of its
author and text (`bridge/decorate_cache.rs`), 2048 answers in all, the least
recently asked going first. A row that stays on screen is looked at again at
most once a second while its text stays the same: a scroll reports the screen
twenty times a second, and only the row that came on screen is new. A row that scrolls back into view is answered from
there without waking the guest; an edit, an update of the plugin or a change of
its settings is a key nothing was kept under, so the plugin is asked again. A
question that got no answer in two seconds — a deadline, a refused tree, a
helper that went away — is asked again on the next pass rather than leaving the
row plain for good. The interface lays the row's text out again only when an
answer about it really changed.

What the plugin is told is the row's own summary — the room, the message's id,
who wrote it, what it says and whether this account wrote it. What it says is
message content, so a plugin without `messages.read` is told the rest with the
body empty, and its `annotation` and `marks` are dropped: it has no words to
style. A plugin that wants the whole message reads it with `messages.get`.
Nothing a plugin answers here changes the message: it is display only.

A tree the widget set refuses is dropped where it arrives, and the row stays as
it was. Turning the plugin off takes its decorations with it, and so does
suspending it.

## How people look

A plugin that changes how people appear declares `identity` in its activation,
and needs `people.read`:

```json
{ "activation": { "on": ["identity"] },
  "permissions": { "capabilities": ["people.read"] } }
```

The parent asks with `yf_identity` **once per (person, server)** and not once
per message, because a channel shows one author twenty times and a member list
shows a hundred people. What comes back is a description of a person: a display
name, a name colour, an avatar, a banner, a decoration, a pronouns line and
badges, every one of them optional. The fields and their caps are in
[`abi.md`](abi.md#the-payloads); a field Yapfast won't draw is dropped and the
rest of the answer still applies.

[`plugins/identity.rs`](../../crates/yapfast-core/src/plugins/identity.rs)
keeps the answers, and is the whole cost of the feature when nothing is drawn
from it:

- **One answer per (person, server)**, keyed by the pair, so the second message
  from one author costs nothing. A DM and a room no server holds are one pair,
  with no server.
- **Asked on a miss, never on a timer.** A person is asked about when a surface
  that shows them finds no answer for them, or finds one older than five
  minutes: the cache is what decides, so a person nobody shows is never asked
  about.
- **Capped at 256 people**, the least lately shown forgotten first, so a long
  session in a large server keeps the people it has shown lately.
- **One answer that changes nothing is kept too**, because "nobody changed this
  person" is an answer: without it every surface would ask again.
- **The chain's order decides, not the arrival order.** Every plugin whose
  activation says `identity` and whose manifest says `people.read` is asked, and
  the answer from the earlier plugin in the chain wins while it is fresh. A
  plugin that declares the trigger without the capability is never asked at all.
- **A change stamp**, `Identities::changes`, so the interface redraws on an
  answer that changed how somebody looks and on nothing else.
- **Turning a plugin off takes the answers with it**, so what a plugin that no
  longer runs said about somebody doesn't stay on screen.

**Which surfaces use it.** Every one that shows a person:

- **The profile card**, opened from a message, the member list, a DM or the
  friends list: name, name colour, avatar, banner, avatar decoration and
  pronouns.
- **The timeline**: each message's author line and avatar (name, colour, avatar,
  decoration) and the people it mentions (a mention says `@` and the plugin's
  name in the plugin's colour), from
  [`plugins/dress.rs`](../../crates/yapfast-core/src/plugins/dress.rs), which
  copies only the rows a plugin changed.
- **The member list's rows**, the **voice list** and the **typing strip**: the
  same name, colour, avatar and decoration.
- **The `badges` field** is drawn on the author line, after the name, as small
  chips.

The people on screen are asked about as they come: the last sixty authors of the
open conversation and the first two hundred of its member list, each a lookup
when a fresh answer is kept. What Yapfast made is kept beside what is shown, so
a plugin turned off puts Yapfast's own back, and a plugin reading `messages.get`,
`people.get` or the events sees people as Yapfast made them, never as another
plugin dressed them.

**The author line.** A decoration may also answer `author`, up to three small
badges after the author's name: `[{ "text": "MOD", "tone": "accent", "tooltip":
"Moderator" }]`, each 1 to 24 characters in one of the annotation tones, saying
whose it is on hover. It needs no capability: it says something about the
author, not the text. `people.badges` reads a person's own Discord badges
([the ABI](abi.md)) for a plugin that draws them there.

## Badging people in the member list

A plugin that puts a mark beside people's names in the member list declares
`member.decorate`, and needs no capability of its own: what comes back is a mark
beside a name in a list it already shows.

```json
{ "activation": { "on": ["member.decorate"] } }
```

The parent asks with `yf_member_decorate` for the rows the list is showing as it
scrolls, one person at a time, and what comes back is a few characters and a
tone — a mark rather than a tree. [`plugins/slots.rs`](../../crates/yapfast-plugins/src/slots.rs)
keeps them under `BADGES`: at most 640 people, 3 badges a person and 16
characters a badge, so a name stays a name. `ui/people.slint` draws a member
row's badges beside Discord's own, each saying whose it is to a screen reader,
and a member list whose snapshot changed — a role, a name, someone leaving —
forgets them all and asks again, because what a plugin said about a person may
no longer hold.

## Keeping a deleted row, and hiding one

A plugin that declares `messages.retain` keeps a deleted row on screen. When a
backend's timeline no longer holds a message the timeline had, the parent keeps
the row it had, in place, flagged `deleted`, with its text and without its
reactions or its actions
([`plugins/retention.rs`](../../crates/yapfast-core/src/plugins/retention.rs)).
The merge is one pass over the messages already in memory, and nothing at all
while no plugin declares the capability. A plugin stores what a message said from
the `message_delete` event it is already sent, and draws it as a decoration.

A plugin that declares `messages.filter` may answer a `yf_decorate` question with
`hide` or `hide_media`, which keeps the row, or its pictures and files, out of
view. `Slots::hidden(message)` says what is hidden for one message across every
plugin that asked: a row any of them hides is hidden.

**Which surfaces draw it.** The kept row is drawn by the timeline like any other,
with the `deleted` flag the row already had. What a plugin hides is kept in
[`plugins/slots.rs`](../../crates/yapfast-plugins/src/slots.rs) and consumed
where the channel's timeline is built. A hidden row is filtered out of every
message whose `hidden(message).row` is set, for the list the window shows and
for the one it compares against, so a hidden row takes its place away rather
than leaving a gap, and the rows are built again when what is hidden changes.
For media, the same row build hides the galleries, stickers and files of a row
whose `hidden(message).media` is set and keeps its text.
The thread panel beside a channel builds its rows from its own timeline and
applies neither, so a hidden message is still a row there.

## Colouring code blocks

A plugin that colours code blocks declares `code` in its activation, and needs
`messages.read`, because a block is message content:

```json
{ "activation": { "on": ["code"] },
  "permissions": { "capabilities": ["messages.read"] } }
```

The parent asks with `yf_code_block` for **one block at a time**, named by the
hash of its content and language, and only for the blocks of the messages on
screen. That is what "on-screen blocks only" means here: the same action that
reports the visible rows is the one that reads their markdown for fences
(`code_block::fences`), and a block whose answer is already kept is never asked
about again.

[`plugins/code_block.rs`](../../crates/yapfast-plugins/src/code_block.rs)
keeps the answers:

- **Keyed by the content hash**, language included, so a block never goes stale
  and two messages quoting the same snippet are one answer.
- **Capped at 128 blocks**, the least lately asked forgotten first, so scrolling
  a channel of code keeps what has been on screen lately.
- **An answer that colours nothing is kept too**, so a block nobody coloured is
  asked about once and not once per frame.
- **The earlier plugin in the chain wins**, whatever order two answers arrive
  in, and the same spans again change nothing.
- **A change stamp**, `CodeBlocks::changes`, for an interface that redraws a
  block that was coloured.
- **The spans must spell the block exactly** (see
  [`abi.md`](abi.md#the-payloads)): a plugin colours a block and never changes
  what it says, so the check is in the helper, which holds the text, and a set
  that doesn't add up is refused where it arrives.

**Which surfaces draw it.** A message's text, in the timeline and in the thread
beside it. `markdown::colour_code` cuts a fenced block into the spans kept for
it as the message's text is laid out, and the theme colours each tone, so a
theme colours a plugin's code without knowing a tone: a comment in the muted
ink, a keyword in the accent, a string in the live one, a number in the loud, a
function in the link's, a type between a keyword's and a number's, a variable in
the secondary, an operator or punctuation in the faint, an error in the danger
one, and `plain` in the text's. The block's ground is drawn around the whole of
it, not by each span.

A row keeps the code block's key as its text is laid out, so whether it still
shows what the plugins answer is a question it can answer: a block coloured
since the row was drawn, one coloured differently and one whose colours went are
all laid out again, which `Effect::CodeBlocks` asks the interface for. A set of
spans that doesn't spell its block is dropped here as the helper drops it.

## Slots

[`plugins/slots.rs`](../../crates/yapfast-plugins/src/slots.rs) keeps what
plugins draw, in `AppState::plugins`, which is `None` with no plugin enabled.
The plugin runtime changes it by sending `Event::PluginSlot` with an `Output`:
`SetSlot` and `ClearSlot` for a slot, and `Decorate` for what a plugin draws
under a message. The same tree again changes nothing. `Reach::of` sends that
event to `show_plugin_slots` alone, which builds again only the slots whose
count of changes moved, and only the message rows whose decorations changed.

| Slot | Id | Where | Draws | Plugins shown | Tree |
| --- | --- | --- | --- | --- | --- |
| Banner | `banner` | The chat's top banner area: under Yapfast's own notices, above the conversation, with Dismiss | Any widget | 4 | 256 |
| Toast | `toast` | Over the window's bottom right, gone after 6 s while the window is focused and the pointer isn't on it | Any widget | 3 | 64 |
| Composer | `composer` | Beside the composer's GIF, sticker and emoji buttons | 3 buttons | 4 | 64 |
| Hover bar | `message.actions` | On a message's hover bar, before More, in the open channel | 2 buttons | 2 | 64 |
| Message | `message` | Under a message's text, attachments and cards, before its reactions | Any widget | — | 64 |
| Dialog | `dialog` | Over the window, modal, with Dismiss; one per plugin, so another replaces it | Any widget | 3 | 256 |
| Profile badges | `profile.badges` | On the open person's card, under their own details and above its notes | Any widget | 4 | 64 |
| Message menu | `menu.message` | A message's right-click menu: each button a row, after Yapfast's, under a line, its plugin's name under its label | 4 buttons | 4 | 64 |
| Person menu | `menu.user` | A person's menu, the same way | 4 buttons | 4 | 64 |
| Profile section | `profile.section` | On a person's card and full profile, under the badges, headed by the plugin's name | Any widget | 4 | 256 |
| Voice panel | `voice.panel` | In the voice panel over the account while in a call, on a line under Yapfast's buttons | 3 buttons | 4 | 64 |
| Account panel | `account.panel` | Over the account at the sidebar's foot, always shown, in or out of a call, under a line crediting the plugin: a small player, a status, buttons | Any widget | 2 | 64 |
| Call controls | `call.actions` | In a call's controls, after a line, before Leave | 2 buttons | 2 | 64 |
| Picker tab | `picker.tab` | A tab of the composer's emoji and sticker picker, named by the plugin, after Yapfast's three | Any widget | 4 | 256 |
| Settings page | `settings.page` | The plugin's own page under Settings → Plugins ([settings](settings.md#a-plugins-own-page)) | Any widget | its own | 256 |
| Sidebar banner | `sidebar.banner` | In place of a server's banner over its channels, for the plugin whose manifest replaces it | Any widget | 1 | 64 |
| Account | `account.identity` | In place of the account's name over the voice panel, the same way | Any widget | 1 | 64 |
| Welcome | `timeline.intro` | In place of a channel's or server's welcome at the top of its history, the same way | Any widget | 1 | 64 |

*Plugins shown* is how many plugins a slot shows at once (`Slot::most_plugins`):
the first to draw there show, and the others' trees wait until one of them goes,
so no crowd of plugins pushes Yapfast's own controls away. A slot of buttons
keeps that many of one plugin's buttons (`Slot::buttons_each`) and drops the
rest of its tree. *Tree* is the most rows and cells the slot takes. Every surface
names the plugin: a card says *Name plugin* over what it drew, with the id in its
tooltip, a button and
a picker's tab say it in their tooltip, a menu row under its label and a
profile section in its heading. With no plugin enabled `AppState::plugin_slots`
is `None` and each surface pays one test of it.

The header buttons and rail items of a manifest, and the member list's badges
(`member.decorate`), are slots of their own kind, above; the slots' ids are
dotted names of where they are, as the theme system's named parts will be, so a
slot and the part around it can share one.

### How a plugin draws

A guest asks with `ui.set_slot` and `ui.clear_slot`; `ui.notice` and
`ui.toast` are the same thing in one line, and `ui.open_dialog` is
`ui.set_slot` on the `dialog` slot. A toast that names a `room` or an `id`
(`ui::Click` on its tree, set by the bridge) takes a click anywhere its own
widgets and Dismiss don't: the card is a `mouse_area` sending
`Input::Clicked`, which opens the room, clears the toast and hands the plugin
`clicked`. `notify.send` takes the same two; its `Alert` carries a
`PluginAlert`, and the shell hands the notifier a name of its own
(`yapfast:plugin-alert:<n>`) in place of the room, so the click comes back to
the plugin that sent it and to no other. None of them needs a capability: a slot is a
plugin's own surface, and only changes to Yapfast's own interface need
`ui.modify`.

The bridge in [`plugins/bridge/`](../../crates/yapfast-plugins/src/bridge/)
answers the call, not the interface, so a guest never waits on a frame:

- **The tree is checked first.** `Tree::parse` refuses one over the cap or an
  interactive cell without an id, and the answer says why in words a plugin's
  author can act on: *"the tree can't be drawn: it has 80 rows and cells, over
  64"*.
- **A slot this build has no place for is refused, not dropped.** `panel`,
  a side panel that isn't built, answers *"Yapfast has no slot called `panel`;
  the slots are banner, toast, composer, …"*, naming them all, and the
  `message` slot answers that what a plugin draws under a message is a
  `message.decorate` answer, not a slot. A tree is checked against its slot's
  limits (`Slot::limits`).
- **Drawings are coalesced to a frame and capped at ten a second per plugin.**
  A plugin that draws on every event costs one update per frame: the last tree
  for a slot (or for one decorated message) replaces the one before it, and a
  plugin that keeps drawing waits out a 100 ms window between frames. Nothing
  reaches the interface while nothing is queued, so an idle plugin wakes no
  timer.
- **The interface touches only what changed.** The application keeps the trees
  in `Slots`, which counts changes per slot, and `show_plugin_slots` builds
  again only the slots whose count moved; the same tree again changes nothing
  and draws nothing.
- **What a plugin drew goes when it stops running.** Turning it off, suspending
  it after three faults, and installing or uninstalling anything take its trees
  away, because a plugin that isn't running shows nothing. An instance evicted
  for being idle keeps them: it is still enabled, and draws them again — or
  doesn't — the next time it runs.

A message's decorations are in `MessageRow::plugin`, one field: an empty
model unless a plugin decorated that row, so a timeline without plugins
carries one empty model per row. `Slots` keeps the decorations of the last
512 messages decorated and drops the least recent, and a decoration is keyed by
the plugin and the message and by nothing else.

Every time the set of rows on screen changes, the plugin is asked again about
the rows that are there: a row that scrolls away and comes back is asked about
anew, and so is one the 512 dropped. A row the plugin was asked about under a
second ago, with the same text and the same enabled plugins, is skipped, so a
scroll asks about the row it brought on screen rather than the forty that
stayed; the bridge keeps that second per question, by a hash. What no pass asks about is a message that
changed while it stayed on screen — an edit, or a plugin's own retention of a
deleted one — so a row keeps the decoration it was first given until something
moves the rows under it. Rows of the thread beside a channel aren't decorated
at all: the thread's rows are built from their own timeline, which nothing asks
the runtime about.

Settings → Plugins is
[`plugins.rs`](../../crates/yapfast-iced/src/settings/pages/plugins.rs): the
helper's state, a row per installed plugin and the switch that runs it, from
`Application::plugins_page` and back as a `page::Request`. Every surface a
plugin draws is in that window: its settings form and page, each of the
fourteen slots ([`plugin_slots.rs`](../../crates/yapfast-iced/src/plugin_slots.rs),
drawn with `plugin_tree`), its rail items and header buttons, and its menu rows. The
colours it gives code blocks are drawn in its tones (above). The core keeps what
they all draw in
[`slots.rs`](../../crates/yapfast-plugins/src/slots.rs) and
[`ui.rs`](../../crates/yapfast-plugins/src/ui.rs).

**Pictures that are buttons, frames and pixel art.** An `image` or a `tile`
takes `crop: [x, y, width, height]`, the part of its picture shown in the
picture's own pixels, so one atlas holds a whole sprite sheet; `pixelated:
true`, drawn pixel-sharp; `width` and `height`, 8 to 512; `selected: true`,
drawn with an accent ring; and `hover_src` or `hover_crop`, another picture or
another part of the same while the pointer is on it. With an `id` it is a
button: a press reports `{ id, value: id }` as a button's does.

An `image` cell's `src` may be an `https://` address on one of the hosts the
manifest's `network` lists, for a plugin that holds `images.web`. The window
never fetches it: it asks the bridge, which makes the request through that
plugin's network gate, as `http.fetch` would and never with a question to the
person, at most 752 KiB of it (what one answer carries), and decodes it on a
blocking thread to at most 512 px a side. The core keeps the last 48 pictures
(`WebPictures`), each asked for once, and forgets a plugin's when it stops. The
cell shows *Loading picture…*, or its own words, until it arrives, and its
words if it never does.

A `src` on Discord's own CDN (`cdn.discordapp.com`, `media.discordapp.net`: an
avatar `people.get` gave, a server's icon) needs neither: the window asks the
core's picture store for it with the avatars on screen (`plugin_slots::cdn_pictures`),
through Yapfast's own session, and shows it while the store holds it
(`Source::discord_cdn`).

What the person does goes back as `Message::PluginAct`, which keeps what only
the window keeps ([What the window keeps](#widget-trees)) and hands the rest to
the runtime as `Input::Widget`: the plugin, the slot, the cell's id, its value
(`true` or `false` for a toggle, the option, the text, the number, `#rrggbb` for
a colour; nothing for a button) and its subject: the message a widget is drawn
on or a message menu's row is about, the person of a person menu or a profile,
the call's channel for a voice panel or call button, or the open conversation. Closing a notice, a toast or a dialog sends
`Input::Dismissed`, which takes it away at once and then reaches the plugin.
Both go to `yf_input` the way a menu row's choice does, and neither comes back.

## The overlay

A plugin with `overlay` draws a free-floating layer of its own pictures over the
whole window, as a cat that follows the pointer is drawn: `overlay.set` says
what the layer shows, each sprite one of the package's picture assets at a place
in the window's logical pixels, with a size, the part of a sheet of frames to
show and an opacity if it wants, and how many frames a second it wants to be
asked for, up to 60.

```json
{ "fps": 12, "sprites": [
    { "id": "neko", "image": "cat", "x": 412, "y": 300, "crop": [64, 0, 32, 32],
      "width": 32, "height": 32 } ] }
```

Each frame comes to `yf_input` as `{ "kind": "overlay_frame", "at_ms": …,
"pointer": [x, y], "width": …, "height": … }`, the pointer left out while it is
outside the window, and the plugin answers it with its next `overlay.set`. A
frame isn't sent while the one before is unanswered (for a second at most), so
a plugin slower than its pace gets frames at its own rather than a queue of
them. It costs little by construction:

- **Nothing is drawn while nothing shows.** With no plugin showing an overlay
  there is no layer at all, and `overlay.clear`, or the plugin stopping, takes
  one away.
- **Nothing ticks while nothing moves.** `fps: 0`, the default, asks for no
  frames: a layer that stands still costs no redraw and no call. A cat that
  falls asleep sets it to 0 and back when the pointer moves.
- **The pace is the window's.** An empty widget at the top of the layer hands
  each plugin its frame when a redraw reaches it and asks for the next redraw
  at the fastest pace asked; handing a frame publishes no message, so it
  rebuilds nothing, and a window that isn't shown draws nothing and ticks
  nothing. The pictures are read once each.
- **Clicks go through.** The layer takes no event, so everything under it
  works as if it weren't there, except a sprite the plugin made
  `"clickable": true`, which comes back as `{ "kind": "overlay_click", "id": … }`.

The layer is over everything plugins draw and under Yapfast's own questions:
the leaving dialog, a plugin's site question and the MFA prompt are never
covered. At most 64 sprites a layer.

### Named parts

A tree is drawn as a part when its plugin's manifest names one for its slot
([manifest](manifest.md#named-parts)). The bridge decides which, not the tree:
`ui::part_id` takes the name `ui.set_slot` asked for, or the first the manifest
gives the slot, and refuses a name the manifest doesn't give that slot with the
reason, as it refuses a tree. The name rides on the tree as `Tree::part`,
`<plugin-id>/<name>`, which a tree's JSON can't set, and `Slots::part` answers
which slot a named part is drawn in, or `None` while none is on screen. The
same tree under another name is a change, since a theme may draw it
differently.

In the window, `PluginPart` carries it as `part-id`, empty for a tree no
manifest named, and `PluginWidgets`, `PluginBanner`, `PluginToast` and
`PluginDialog` expose it as an `out` property of the same name. That is the
whole hook: nothing draws differently for a name, so a part looks exactly as an
unnamed tree does until a theme styles it, and a plugin that names nothing
costs one empty string per tree.

`slots.json` beside a manifest in `YAPFAST_PLUGINS_DIR` stands in for the
runtime, for trying widgets without writing a `.wasm`: an object from a slot's
id, or `message:` and a message's id, to a tree, shown as the plugins load.
Its trees go through the same checks, and each is the first part its manifest
names for its slot. [`testing/plugins/widgets`](../../testing/plugins/widgets)
shows each widget that way.
