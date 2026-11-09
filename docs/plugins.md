# Plugins

Plugins are behind the `plugins` Cargo feature, off by default:
`cargo build --profile fast --features plugins`. The plan for the whole
system is [`PLAN.md`](../PLAN.md). What exists so far is the interface's
side of it: reading what a plugin's manifest changes in Yapfast's own
interface, and checking it. Nothing runs a plugin's WebAssembly yet.

## Trying a plugin

Git install isn't built yet. Until it is, a build with plugins loads every
plugin in the folder `YAPFAST_PLUGINS_DIR` names, one subfolder each with
its `yapfast-plugin.json`, in the order of the subfolders' names, which is
the order their changes win in. A manifest that can't be used is skipped and
logged, and so is each change refused; run with `YAPFAST_LOG=info` to see
them. [`testing/plugins`](../testing/plugins) has an example:

```sh
YAPFAST_PLUGINS_DIR=testing/plugins cargo run --profile fast --features plugins -- --preview
```

`yapfast --no-plugins` starts in safe mode: no plugin's changes apply, and
which plugins are enabled stays as it was. It's the way back from an
interface a plugin made unusable. A restart, such as after the GPU fails,
keeps it.

## The manifest's interface changes

[`crates/yapfast-core/src/plugins.rs`](../crates/yapfast-core/src/plugins.rs)
reads a manifest, `yapfast-plugin.json`. It's strict JSON, and must name
`"api": 1`, a reverse-DNS `id` and a `name` of up to 64 characters. The
interface reads only `id`, `name`, `wasm`, `permissions`, `menus` and `ui`,
and leaves the rest of the manifest to the runtime. Inside `menus` and `ui`
a field Yapfast doesn't know is an error, so a misspelt change fails loudly.

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

[`plugins/ui.rs`](../crates/yapfast-core/src/plugins/ui.rs) checks each
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

## In the window

[`src/ui/plugins.rs`](../src/ui/plugins.rs) loads the plugins as the window
is made, before the theme applies, and keeps them in the adapter as
`Option<Box<Plugins>>`: `None` without any, so no model, timer or thread
exists for them. Each change applies where Yapfast already builds that part:

- **Menus.** `menu` in [`src/ui/menus.rs`](../src/ui/menus.rs) builds its
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
  before anything else, and sends `Action::Plugin(Input::Menu)`, which the
  application returns as `Effect::Plugin`. The adapter hands it to the
  `Runtime` it holds, which is `None` until the runtime exists.
- **Parts.** The `UiParts` global in
  [`ui/yapfast/parts.slint`](../ui/yapfast/parts.slint) holds one switch per
  part that can be hidden, and each part sits behind an `if` on it: the
  server banner over the channel list, who's typing, and the hover bar's
  Reply, reactions and More, in the channel and the thread beside it.
  Shift's Delete takes More's place whether or not More is hidden.
- **Sizes.** `ClientStyle` in
  [`ui/yapfast/style.slint`](../ui/yapfast/style.slint) takes one input per
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
