# The manifest

`yapfast-plugin.json` says what a plugin is, what it needs, and everything it
puts on screen before it runs. It is strict JSON, read with the `serde_json`
Yapfast already depends on, so the plugin system adds no format parser of its
own. Comments and trailing commas aren't allowed, and a field Yapfast doesn't
know is an error rather than something ignored: a misspelled `permission` that
silently granted nothing would be worse than a refused install.

The types are in
[`crates/yapfast-plugin-api/src/manifest.rs`](../../crates/yapfast-plugin-api/src/manifest.rs).

## A complete example

```json
{
  "api": 1,
  "id": "com.example.word-filter",
  "name": "Word Filter",
  "version": "1.4.0",
  "description": "Hides words you'd rather not read.",
  "authors": ["Ada <ada@example.com>"],
  "license": "MIT",
  "homepage": "https://example.com/word-filter",
  "source": "https://codeberg.org/ada/word-filter",
  "min_yapfast": "0.5.0",

  "wasm": "plugin.wasm",
  "icon": "icon.png",
  "wasi": false,

  "limits": { "memory_pages": 128, "storage_bytes": 262144 },
  "activation": { "on": ["event:message_create", "command", "menu"] },
  "permissions": {
    "capabilities": ["messages.read", "storage"],
    "network": ["api.example.com", "*.cdn.example.com"]
  },

  "commands": [
    {
      "name": "filter",
      "description": "Add a word to the filter",
      "options": [{ "name": "word", "type": "string", "required": true }]
    }
  ],
  "menus": [
    {
      "target": "message",
      "label": "Filter this word",
      "icon": "assets/filter.png",
      "action": "filter_selected",
      "when": { "network": "discord", "own": false }
    }
  ],
  "settings": [
    { "key": "replacement", "type": "string", "label": "Replacement text", "default": "***" },
    { "key": "strict", "type": "bool", "label": "Match inside words", "default": false }
  ],
  "tweaks": { "messages.attachments.mosaic": false }
}
```

The `tweaks` line is refused in this build, though the rest of that manifest is
read as written: nothing registers a setting point yet, so every point a manifest
sets comes back as one this Yapfast doesn't have
([interface](interface.md#setting-points)).

## The plugin

| Field | Meaning |
| --- | --- |
| `api` | The plugin API the module was built for. This build speaks 1, and a manifest that asks for anything else is refused with the two version numbers rather than loaded and left to fail at instantiation. |
| `id` | Reverse-DNS, lowercase, stable forever: `com.example.word-filter`. At least two dot-separated labels of `a-z`, `0-9` and hyphen, no label starting or ending with a hyphen, at most 128 bytes and 63 per label. It is the key of the lock file, the name of the plugin's storage directory and the name shown beside anything it puts on screen, so it is never derived from the display name. |
| `name` | What the page shows. |
| `version` | `major.minor[.patch]`, so `1.4` reads as `1.4.0`. A pre-release or build suffix, as in `1.0.0-beta.2`, is compared by its numbers, since installs pin commits rather than tags. |
| `description`, `authors`, `license`, `homepage`, `source` | Shown on the plugin's page and in the review dialog. `source` is where the code that was reviewed lives. |
| `min_yapfast` | The oldest Yapfast the plugin runs on. The review shows it so a person can see what they are installing; what decides whether a plugin loads is `api`. |
| `wasm` | The module, relative to the manifest. Absent for a plugin that only sets `tweaks` or only contributes commands, menus or settings. The file is at most 8 MiB, and must start with `\0asm`: WebAssembly text and Extism's own module formats are refused. |
| `icon` | A path relative to the manifest, like `wasm`. |
| `wasi` | Whether the guest needs WASI, as Go and JavaScript builds do. Off by default; WASI here offers clocks and randomness and no preopens. |

`wasm` and `icon` are resolved inside the plugin's own directory. An absolute
path, or one that climbs out with `..`, is refused before anything is read, and
so is an empty one; `./plugin.wasm` is the same file as `plugin.wasm` and is
allowed.

## Limits a manifest may ask for

| Limit | Default | Range a manifest may ask for |
| --- | --- | --- |
| `memory_pages` | 256 pages, 16 MiB | 32 to 1024 pages, 2 to 64 MiB |
| `http_response_bytes` | 4 MiB | up to 16 MiB |
| `storage_bytes` | 1 MiB | up to 16 MiB |

A page is 64 KiB of guest memory. The limit is a ceiling and not a
reservation, so only pages the guest touches cost anything, and the default is
about half of Yapfast's own idle memory: one ordinary plugin shouldn't be able
to dominate the app. Below 32 pages there is no room for a Rust PDK plugin's
stack and its input, and above 1024 the review dialog's memory line stops
meaning anything. Asking for more than the default is a request someone
approves, and the dialog states the amount.

The storage cap is the plugin's own key-value store, which is the only thing that
survives an eviction. HTTP responses are streamed, so the cap bounds what one
answer may carry rather than what the helper holds.

## Activation

```json
{ "activation": { "on": ["event:message_create", "command"] } }
```

`on` names what wakes the plugin, and nothing else does: a plugin nothing has
woken has no instance, no memory and no cost beyond its registry entry. Eight
kinds of trigger are read:

| Trigger | What wakes it |
| --- | --- |
| `startup` | The session starts |
| `command` | A person typed one of its commands |
| `menu` | A person chose one of its menu rows, rail items or header buttons |
| `event:<kind>` | One of the events below, for the rooms and people it subscribed to |
| `decorate` | A message it draws under came on screen ([interface](interface.md#decorating-messages)) |
| `identity` | A surface showed someone whose answer it doesn't have ([interface](interface.md#how-people-look)) |
| `code` | A code block on screen wasn't coloured yet ([interface](interface.md#colouring-code-blocks)) |
| `member.decorate` | A member list row came on screen ([interface](interface.md#badging-people-in-the-member-list)) |

The event kinds are `message_create`, `message_update`, `message_delete`,
`reaction_add`, `reaction_remove`, `room_selected`, `voice_state`, `app_focus`,
`account_ready`, and the opt-in coalesced ones, `typing`, `presence` and
`speaking`. A plugin that holds `messages.intercept` is activated at startup
whether or not it says so, because the first message must not go out before it
is ready.

A trigger this build doesn't know wakes nothing: the subscriptions are a bitmask
built from the names above, so an `event:` name that isn't one subscribes to
nothing, and a trigger word nobody reads keeps the plugin idle rather than
failing the install.

## Hooks and their failure

`on_failure` says what happens when the plugin fails or runs late in a hook. It
is `fail_open` unless the manifest writes it, and the only other value is
`block`:

```json
{ "on_failure": "block" }
```

`block` needs `messages.intercept`, and a manifest that asks for it without that
capability is refused with `on_failure: "block"` needs the
`messages.intercept` capability rather than installed. With it, the plugin's
`block` outcome stops the send and the reason is shown where the person typed;
from a plugin that didn't say it, a `block` is the fail-open answer. Both ways
the message is held no longer than the hook's budget: 25 ms for the plugin and
100 ms for the chain.

## Commands

Each command is a row in the composer's `/` list. A command needs a name and a
non-empty description: the description is what tells a person what it does, so
an empty one is refused. A command's options are parsed and checked in the
composer before the plugin is called.

```json
{
  "name": "filter",
  "description": "Add a word to the filter",
  "options": [
    { "name": "word", "type": "string", "required": true },
    { "name": "scope", "type": "choice", "description": "Where it applies" }
  ]
}
```

The types are `string`, `integer`, `number`, `bool`, `choice`, `person` and
`channel`. A `choice` is one of a closed list of values, and `person` and
`channel` take an id the composer resolves; an option of any other type is
checked by the composer before the plugin is called, so a command handles a
value of its declared type and not a string it has to parse.

## Menus

Menus are data, so a row appears without loading any WebAssembly: the row's
label and `when` are all the interface needs. `target` is one of `message`,
`user`, `user_profile`, `channel`, `thread`, `group_dm`, `server`,
`server_header`, `role`, `emoji`, `sticker`, `image`, `stream`, `game`,
`forum_tag`, `composer` and `attach`, and Yapfast draws the rows of the menus it
has. `action` names what the plugin receives when the row is chosen. `icon` is a
path in the plugin's directory, which no row draws yet.

`when` is a closed set of conditions, not an expression language:

| Field | Meaning |
| --- | --- |
| `network` | `discord` or `matrix` |
| `own` | Whether the message or person is the account's own |
| `dm` | Whether it is a DM |
| `has_attachment` | Whether the message carries a file |
| `is_bot` | Whether the person is a bot |
| `role` | A role id the person the menu is about has |

## Rail items and header buttons

A plugin can put a button where Yapfast draws a row of its own: at the foot of
the server rail, and in a conversation's header. Both are data like a menu row,
so they show for a plugin that has never run, and both take the same two fields:

```json
{ "rail_items": [{ "label": "Notes", "action": "notes" }],
  "header_buttons": [{ "label": "Summarise", "action": "summarise" }] }
```

| Field | Meaning |
| --- | --- |
| `label` | What the button says: plain text on one line, 1 to 64 characters |
| `action` | What choosing it tells the plugin, as a plain name of up to 64 characters: letters, digits, `-`, `_` and `.` |

A plugin may add up to 64 of each, and needs no capability for either: a button
is the plugin's own surface. The rail's buttons sit under a line at the rail's
foot, the header's beside the conversation's own controls, and each says whose it
is to a screen reader. Both fields are optional, and a manifest that carries them
installs like any other: they are part of what the installer reads, so nothing
about them waits for the plugin to run.

## The interface Yapfast already draws

`ui` names changes to Yapfast's own interface, and needs the `ui.modify`
capability; without it nothing in the section applies. Its four parts:

```json
{ "ui": {
    "menu": [{ "target": "user", "row": "profile", "label": "Wave at", "before": "mention" }],
    "parts": { "hide": ["timeline.typing"] },
    "colors": { "accent": "#7aa2ff" },
    "sizes": { "message-group-gap": 8 } } }
```

| Field | What it changes |
| --- | --- |
| `menu` | A built-in menu row: `target`, `row` (the built-in row's id), and `hide`, `label`, `before` or `after`. A row is hidden, relabelled, or moved before or after another; not before and after both |
| `parts` | `hide` and `replace`, each a list of part ids such as `sidebar.banner` or `message.actions.react` |
| `colors` | Theme tokens by name, as `#rgb`, `#rrggbb` or `#rrggbbaa` |
| `sizes` | Sizes by name, in logical pixels, each clamped to its range |

The rows, parts, colours and sizes a plugin may name are closed lists, and a
change costs one of the 64 a plugin may make; [the interface](interface.md#the-manifests-interface-changes)
has each list, and the reason a row or a colour is refused there rather than
here. A field of `ui` this build doesn't know — a misspelt `hide`, say — refuses
the manifest, while a name it doesn't have is refused by the interface, which is
the only side that knows what exists in this build, and its reason shows on the
plugin's page.

## Settings

The plugin's page draws a setting from its declaration, so the page exists
before the plugin has ever run:

```json
{ "key": "replacement", "type": "string", "label": "Replacement text", "default": "***" }
```

| Field | Meaning |
| --- | --- |
| `key` | What the plugin reads it by. Two settings with one key are refused. |
| `type` | `string`, `bool`, `select`, `slider`, `number`, `integer`, `list` or `widget` |
| `label`, `description` | What the page shows |
| `default` | The starting value, as its type expects |
| `options` | For `select`: the choices |
| `min`, `max` | For `slider`, `number` and `integer` |
| `pattern` | For `string`: a regular expression the value must match |
| `when` | The same conditions a menu row takes |

A `list` is a list of records a person adds to and edits. A `widget` is a tree
the plugin draws while it runs, and it holds no value of its own.

## Dependencies

```json
{
  "dependencies": { "com.example.base": "^1.2" },
  "optional_dependencies": { "com.example.theme": "~0.4.0" }
}
```

A requirement is `*`, `^1.2`, `~1.4.0` or `=1.4.0`. `^` admits the same major
version at least as high as the one named, and `^0.2` admits `0.2.x` alone,
since a zero major version is its own compatibility boundary. `~` keeps the major
and minor version, `=` is exact, and a bare `1.4.0` means `^1.4.0`.

A required dependency must be installed, enabled and a version that satisfies the
requirement, or the plugin is refused when it is enabled, naming both plugins
and the version that is installed. An optional dependency orders a plugin only
when it is enabled itself. Activation order is by layer: everything a plugin
needs activates before it, and plugins in one layer may activate together. Only
activation has an order; compilation doesn't, because a module needs nothing
from its dependency to compile.

A dependency is also what gives a plugin permission to reach another. The only
call that goes from one plugin to another is `plugin.call`, and it runs only
along an edge the caller's manifest declares, required or optional: the caller
names the callee, the callee doesn't name the caller, so no pair of plugins can
call each other in a loop. `plugin.publish` and `plugin.subscribe` need no edge:
a topic is a name either plugin may use.

Turning a plugin off turns off every plugin that needs it, since one left
enabled over a dependency that is off would be one whose calls were all refused.
Updating or rolling back a plugin stops its dependents in reverse dependency
order, swaps the version, and starts them again in dependency order.

`tweaks` are named switches that parts of Yapfast offer, such as
`messages.attachments.mosaic`. A plugin may set them without holding a
capability, because each one is a behaviour Yapfast already has. No part offers
one yet: the registry in
[`tweaks.rs`](../../crates/yapfast-core/src/tweaks.rs) is empty, so every point a
manifest names comes back refused as one this Yapfast doesn't have, and nothing
in the window changes.

## What validation refuses

| Refusal | The manifest's fault |
| --- | --- |
| Not JSON, or not this shape | A syntax error, an unknown field, an id or host pattern that isn't one |
| Another API | `api` isn't 1 |
| A path outside the plugin | `wasm` or `icon` is absolute, empty, or climbs out |
| Nothing to do | No `wasm`, and no command, menu, setting, rail item, header button, change to the interface or tweak |
| Memory outside the range | `memory_pages` is below 32 or above 1024 |
| A quota too high | `http_response_bytes` or `storage_bytes` is above the most consent may allow |
| A port without its capability | `local-port` is set and `network.local` isn't declared |
| A command with nothing to say | Its name is empty, or its description is an empty string |
| A label that won't show | A menu row's, rail item's, header button's or `ui` row's label is empty, over 64 characters, or carries a control character |
| An action that isn't a name | A menu row's or button's `action`, or a `ui` row's `row` and anchor, isn't letters, digits, `-`, `_` and `.` |
| More than 64 contributions | Menu rows, rail items, header buttons, or changes to the interface, past 64 |
| Two settings with one key | Duplicate `settings[].key` |
| Depending on itself | `dependencies` names the plugin's own id |

Nothing is executed during validation: the manifest is read, the module's header
and magic are checked, its imports are checked against the two host functions the
ABI offers, and the module is compiled into the cache. A refusal reaches the
person as the sentence above, and no part of the plugin is installed.
