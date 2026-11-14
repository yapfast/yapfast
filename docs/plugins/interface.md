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
([the marketplace](marketplace.md)).

For the interface's own half, `YAPFAST_PLUGINS_DIR` names a folder that a build
with plugins reads as the window is made: one subfolder each, with its
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

[`crates/yapfast-plugins/src/lib.rs`](../../crates/yapfast-plugins/src/lib.rs)
reads a manifest, `yapfast-plugin.json`. It's strict JSON, and must name
`"api": 1`, a reverse-DNS `id` and a `name` of up to 64 characters. The
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
  ([`ui/rail.slint`](../../ui/rail.slint)) and at the end of the conversation's
  header ([`ui/header.slint`](../../ui/header.slint)). Choosing one is a menu
  row's path exactly: the action goes to the runtime as `Input::Menu`, with the
  open conversation as its target.
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
  before anything else and sends `Action::PluginInput(Input::Menu)`, carrying
  the row's target and its action. No `Effect` carries it back: `plugin_input`
  in [`application.rs`](../../crates/yapfast-core/src/application.rs) hands it
  to `PluginState::deliver`, which sends `Request::Input` down the pipe, and
  the helper activates the plugin if it was evicted, then calls `yf_input`
  with it ([the ABI](abi.md)). Nothing answers, so nothing waits on it: a
  plugin that wants to be seen draws again or calls a host method.
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
to keep the theme's own contrast; 4.5:1 throughout would
refuse every muted colour on those themes.

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

**Which surfaces use it.** Today, one: the profile card, opened from a message,
from the member list, from a DM or from the friends list. The card is drawn from
the answer at once if one is kept, and again when one arrives; its person's name,
name colour, avatar, banner, avatar decoration and pronouns change, and nothing
about the card's roles, badges, notes or tabs does.

**Which don't, yet.** The message row's author line and avatar, the member list
row's name and avatar, the DM header, the thread's rows, reply previews, mention
chips and search results draw Yapfast's own view of a person and no plugin's.
The answer is kept for them like any other, so wiring one of them is a lookup and
not a new call. The `badges` field of an answer is kept too and drawn nowhere:
the card's badges are Discord's own, with pictures, and a plugin's name-only
badge has no surface of its own yet. A member list's plugin badges come from
`member.decorate` instead, below.

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
where the channel's timeline is built. `unhidden` in
[`src/ui/refresh.rs`](../../src/ui/refresh.rs) filters out every message whose
`hidden(message).row` is set, for the list it shows and for the one it compares
against, so a hidden row takes its place away rather than leaving a gap; it
answers `None` while no plugin hides a row, so an unfiltered timeline is passed
along rather than copied, and the rows are built again when what is hidden
changes. For media, the same row build applies
[`hide_media`](../../src/ui/messages.rs) to a row whose `hidden(message).media`
is set, which empties its galleries, stickers and files and keeps its text.
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

**Which surfaces draw it.** None yet. The spans are asked for, checked and kept,
and reachable through the application's plugin state; the code block itself is
drawn by the markdown renderer in `ui/messages.slint`, which does not take spans
yet. Wiring it is a lookup per block in that renderer and not a new call.

## Slots

[`plugins/slots.rs`](../../crates/yapfast-plugins/src/slots.rs) keeps what
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
| Profile badges | `profile.badges` | On the open person's card, under their own details and above its notes | Any widget |

### How a plugin draws

A guest asks with `ui.set_slot` and `ui.clear_slot`; `ui.notice` and
`ui.toast` are the same thing in one line, and `ui.open_dialog` is
`ui.set_slot` on the `dialog` slot. None of them needs a capability: a slot is a
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
  the slots are banner, toast, dialog, profile.badges"*, and the `message` slot
  answers that what a plugin draws under a message is a `message.decorate`
  answer, not a slot.
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
anew, and so is one the 512 dropped. What no pass asks about is a message that
changed while it stayed on screen — an edit, or a plugin's own retention of a
deleted one — so a row keeps the decoration it was first given until something
moves the rows under it. Rows of the thread beside a channel aren't decorated
at all: the thread's rows are built from their own timeline, which nothing asks
the runtime about.

[`ui/plugins.slint`](../../ui/plugins.slint) draws a tree with precompiled
components. `PluginWidgets` puts *From* and the plugin's name above every
tree, and a plugin's buttons on the hover bar and beside the composer say
whose they are to a screen reader. Each interactive cell has a role, a label
and keyboard focus, with Space and Enter: buttons and links are buttons, a
toggle a switch, a select a combo box, a slider a slider. A select is
`Dropdown` from [`ui/controls.slint`](../../ui/controls.slint), Yapfast's own
field and menu in the theme's colours rather than Slint's stock `ComboBox`, as
it is on a plugin's settings page. Pictures in `icon` and `image` cells stay
empty until the runtime fetches them.

What the person does goes back through the window's `plugin-acted` callback
as `Input::Widget`: the plugin, the slot, the cell's id, its value (`true`
or `false` for a toggle, the option, the text, the number; nothing for a
button) and its subject, the message a widget is drawn on, or the open
conversation for a composer button. Closing a notice, a toast or a dialog sends
`Input::Dismissed`, which takes it away at once and then reaches the plugin.
Both go to `yf_input` the way a menu row's choice does, and neither comes back.

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
