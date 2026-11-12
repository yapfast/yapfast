# Plugins

Plugins are behind the `plugins` Cargo feature, which is on by default, so a
plain build carries them. This document is the interface's side of the system:
reading and checking what a plugin's manifest changes in Yapfast's own
interface, and what a running plugin draws in it
([`settings.md`](settings.md) is the Plugins page).

## Trying a plugin

Git install isn't built yet. Until it is, a build with plugins loads every
plugin in the folder `YAPFAST_PLUGINS_DIR` names, one subfolder each with
its `yapfast-plugin.json`, in the order of the subfolders' names, which is
the order their changes win in. A manifest that can't be used is skipped and
logged, and so is each change refused; run with `YAPFAST_LOG=info` to see
them. [`testing/plugins`](../../testing/plugins) has an example:

```sh
YAPFAST_PLUGINS_DIR=testing/plugins cargo run --profile fast --features plugins -- --preview
```

`yapfast --no-plugins` starts in safe mode: no plugin's changes apply, and
which plugins are enabled stays as it was. It's the way back from an
interface a plugin made unusable. A restart, such as after the GPU fails,
keeps it.

## The manifest's interface changes

[`crates/yapfast-core/src/plugins.rs`](../../crates/yapfast-core/src/plugins.rs)
reads a manifest, `yapfast-plugin.json`. It's strict JSON, and must name
`"api": 1`, a reverse-DNS `id` and a `name` of up to 64 characters. The
interface reads only `id`, `name`, `wasm`, `permissions`, `menus`, `ui`,
`settings` and `tweaks`, and leaves the rest of the manifest to the runtime.
Inside `menus`, `ui` and each setting a field Yapfast doesn't know is an
error, so a misspelt change fails loudly. `wasm` is optional: a plugin
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

[`plugins/ui.rs`](../../crates/yapfast-core/src/plugins/ui.rs) checks each
change with `check`, which keeps what applies and says why it refused the
rest, in words a plugin's page can show:

- **`ui` needs `ui.modify`.** Without the capability, nothing in `ui`
  applies. Rows added through `menus` need no capability.
- **Menu rows** are named by `target` and the row's id. `target` is the
  closed list of plan §17.2 (`message`, `user`, `channel`, `thread`,
  `group_dm`, `server`, `server_header` and the rest); Yapfast applies the
  ones it has a menu for. Only rows in `EDITABLE_ROWS` change, so a new
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
[`tweaks.rs`](../../crates/yapfast-core/src/tweaks.rs) is their registry: the
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

[`src/ui/plugins.rs`](../../src/ui/plugins.rs) loads the plugins as the window
is made, before the theme applies, and keeps them in the adapter as
`Option<Box<Plugins>>`: `None` without any, so no model, timer or thread
exists for them. Each change applies where Yapfast already builds that part:

- **Menus.** `menu` in [`src/ui/menus.rs`](../../src/ui/menus.rs) builds its
  rows, then `edit_menu` hides, relabels and moves rows, skipping any drawn
  in the danger tone, and a row moves only within its group. Plugins' own
  rows follow, under a line, after the built-in rows and before a last
  group holding a danger row. Each shows its plugin's name under its label,
  so no plugin's row passes for Yapfast's own, and its id is
  `plugin/<index>/<action>`, by the plugin's place in `UiOverrides`. A
  server's menu is `server`, its header's dropdown `server_header`; a
  person's, someone's in a call, and a one-to-one DM's menu are `user`; a
  group DM's is `group_dm`, a thread's `thread`, and any other channel's
  `channel`. The message menu is drawn in Slint and takes no changes yet.
- **Choosing a plugin's row.** `choose` checks for the `plugin/` prefix
  before anything else, and sends `Action::PluginInput(Input::Menu)`, which the
  application returns as `Effect::PluginInput`. The adapter hands it to the
  `Runtime` it holds, which is `None` until the runtime exists.
- **Parts.** The `UiParts` global in
  [`ui/yapfast/parts.slint`](../../ui/yapfast/parts.slint) holds one switch per
  part that can be hidden, and each part sits behind an `if` on it: the
  server banner over the channel list, who's typing, and the hover bar's
  Reply, reactions and More, in the channel and the thread beside it.
  Shift's Delete takes More's place whether or not More is hidden.
- **Sizes.** `ClientStyle` in
  [`ui/yapfast/style.slint`](../../ui/yapfast/style.slint) takes one input per
  size, `text-size-plugin` and the rest, used in place of Yapfast's own
  value when it isn't negative. `sidebar-width` is the sidebar's width
  before the person drags it, and what a double click returns it to.
- **Colours.** `theme::set_overlay` keeps the plugins' colours, and
  `Theme::apply` lays them over whichever theme is shown, so they follow
  every change of theme, and the theme editor's preview. While a dialog no
  plugin may restyle is open, `theme::protect` shows the theme without
  them; today that's the question before following a link whose text shows
  another address.

### Colours

`unreadable` takes the theme a person chose and a plugin's colours, and
leaves out any colour that would bring text under 4.5:1 against a surface
it's drawn on (`TEXT_ON`): `text`, `secondary`, `muted`, `faint` and `link`
on the sidebar, panel, chat, raised and field surfaces, and `on-accent` on
`accent`. A surface colour can be refused the same way. Where a theme's own
pair is already under 4.5:1, as muted text often is, the overlay only has
to keep the theme's own contrast; the plan asks for 4.5:1 throughout, which
would refuse every muted colour on those themes.

### Widget trees

What a plugin draws in a slot is a `Tree`: rows of cells, two levels, as
Slint draws no tree of any depth. A cell's `kind` is `text`, `heading`,
`muted`, `code`, `badge`, `icon`, `image`, `button`, `link`, `toggle`,
`select`, `input`, `slider`, `progress`, `divider` or `spacer`, and its
`tone` is `normal`, `muted`, `accent`, `success`, `warning` or `danger`,
never a colour. `Tree::parse` refuses a tree of more than 64 rows and cells
together, or an interactive cell without a plain `id`, and cuts text to
4 KiB a cell.

```json
{ "rows": [
  { "cells": [{ "kind": "heading", "text": "Daily digest" }, { "kind": "badge", "text": "New", "tone": "accent" }] },
  { "framed": true, "cells": [{ "kind": "toggle", "id": "digest", "text": "Every morning", "checked": true }] },
  { "cells": [{ "kind": "button", "id": "open", "text": "Open", "tone": "accent" }] }
] }
```

## Decorating messages

A plugin that puts something under a message declares `decorate` in its
activation:

```json
{ "activation": { "on": ["decorate"] } }
```

When a row comes on screen, the parent asks the plugin for that row with
`yf_decorate`, and the answer is a widget tree or nothing. Only the rows the
person can see are asked about, the whole pass is capped at a screenful (64
rows), and a row keeps its decoration while it is one of the last 512 decorated
and is asked for again when the rows on screen change. The row is drawn plain
until the answer comes: a decoration is worth a frame and not a wait, which is
why the plugin has 16 ms.

What the plugin is told is the row's own summary — the room, the message's id,
who wrote it, what it says and whether this account wrote it. A plugin that
wants the whole message reads it with `messages.get`, which is what
`messages.read` is for. Nothing a plugin answers here changes the message: it is
display only, and the plugin's name is above what it draws.

A tree the widget set refuses is dropped where it arrives, and the row stays as
it was. Turning the plugin off takes its decorations with it, and so does
suspending it.

## Slots

[`plugins/slots.rs`](../../crates/yapfast-core/src/plugins/slots.rs) keeps what
plugins draw, in `AppState::plugins`, which is `None` with no plugin enabled.
The plugin runtime changes it by sending `Event::PluginSlot` with an `Output`:
`SetSlot` and `ClearSlot` for a slot, and `Decorate` for what a plugin draws
under a message. The same tree again changes nothing. `Reach::of` sends that
event to `show_plugin_slots` alone, which builds again only the slots whose
count of changes moved, and only the message rows whose decorations changed.

| Slot | Id | Where | Draws |
| --- | --- | --- | --- |
| Banner | `banner` | Under Yapfast's own notices, above the conversation, with Dismiss | Any widget |
| Toast | `toast` | Over the window's bottom right, gone after 6 s while the window is focused and the pointer isn't on it | Any widget |
| Composer | `composer` | Beside the composer's GIF, sticker and emoji buttons | Buttons only |
| Hover bar | `message.actions` | On a message's hover bar, before More, in the open channel | Buttons only |
| Message | `message` | Under a message's text, attachments and cards, before its reactions | Any widget |
| Dialog | `dialog` | Over the window, modal, with Dismiss; one per plugin, so another replaces it | Any widget |

### How a plugin draws

A guest asks with `ui.set_slot` and `ui.clear_slot`; `ui.notice` and
`ui.toast` are the same thing in one line, and `ui.open_dialog` is
`ui.set_slot` on the `dialog` slot. None of them needs a capability: §9.1 of
the plan gives every plugin its own slots, and only changes to Yapfast's own
interface need `ui.modify`.

The bridge in [`plugins/bridge.rs`](../../crates/yapfast-core/src/plugins/bridge.rs)
answers the call, not the interface, so a guest never waits on a frame:

- **The tree is checked first.** `Tree::parse` refuses one over the cap or an
  interactive cell without an id, and the answer says why in words a plugin's
  author can act on: *"the tree can't be drawn: it has 80 rows and cells, over
  64"*.
- **A slot this build has no place for is refused, not dropped.** `panel`,
  the side panel §11.2 leaves for later, answers *"Yapfast has no slot called
  `panel`; the slots are banner, toast, composer, message.actions, dialog"*, and
  the `message` slot answers that what a plugin draws under a message is a
  `message.decorate` answer, not a slot.
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
512 messages decorated and drops the least recent; the runtime decorates a
message again when it's edited, or when it comes back on screen after being
dropped. Rows of the thread beside a channel aren't decorated yet, and no
plugin decorates a message yet either: `message.decorate` is named here and in
the manifest, and nothing calls it.

[`ui/plugins.slint`](../../ui/plugins.slint) draws a tree with precompiled
components. `PluginWidgets` puts *From* and the plugin's name above every
tree, and a plugin's buttons on the hover bar and beside the composer say
whose they are to a screen reader. Each interactive cell has a role, a label
and keyboard focus, with Space and Enter: buttons and links are buttons, a
toggle a switch, a select a combo box, a slider a slider. Pictures in `icon`
and `image` cells stay empty until the runtime fetches them.

What the person does goes back through the window's `plugin-acted` callback
as `Input::Widget`: the plugin, the slot, the cell's id, its value (`true`
or `false` for a toggle, the option, the text, the number; nothing for a
button) and its subject, the message a widget is drawn on, or the open
conversation for a composer button. Closing a notice, a toast or a dialog sends
`Input::Dismissed` and takes it away at once. Both reach the `Runtime` as
`Effect::PluginInput`.

`slots.json` beside a manifest in `YAPFAST_PLUGINS_DIR` stands in for the
runtime, for trying widgets without writing a `.wasm`: an object from a slot's
id, or `message:` and a message's id, to a tree, shown as the plugins load.
Its trees go through the same checks. [`testing/plugins/widgets`](../../testing/plugins/widgets)
shows each widget that way.
