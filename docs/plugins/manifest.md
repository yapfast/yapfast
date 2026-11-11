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

`on` names what wakes the plugin. `startup` activates it with the app; `command`
and `menu` are satisfied by the manifest's own contributions, so nothing loads
until one is used; `event:<kind>` subscribes to an event. The event kinds are
`message_create`, `message_update`, `message_delete`, `reaction_add`,
`reaction_remove`, `room_selected`, `voice_state`, `app_focus`,
`account_ready`, and the opt-in coalesced ones, `typing`, `presence` and
`speaking`. A plugin that holds `messages.intercept` is activated at startup
whether or not it says so, because the first message must not go out before it
is ready.

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
`user`, `channel`, `server`, `role`, `emoji`, `sticker`, `image`, `stream`,
`game`, `composer` and `attach`. `action` names what the plugin receives when
the row is chosen. `icon` is a path in the plugin's directory or an icon name.

`when` is a closed set of conditions, not an expression language:

| Field | Meaning |
| --- | --- |
| `network` | `discord` or `matrix` |
| `own` | Whether the message or person is the account's own |
| `dm` | Whether it is a DM |
| `has_attachment` | Whether the message carries a file |
| `has_link` | Whether its text contains a link |
| `is_bot` | Whether the person is a bot |

## Settings

The plugin's page draws a setting from its declaration, so the page exists
before the plugin has ever run:

```json
{ "key": "replacement", "type": "string", "label": "Replacement text", "default": "***" }
```

| Field | Meaning |
| --- | --- |
| `key` | What the plugin reads it by. Two settings with one key are refused. |
| `type` | `string`, `bool`, `select`, `slider`, `number`, `integer` or `list` |
| `label`, `description` | What the page shows |
| `default` | The starting value, as its type expects |
| `options` | For `select`: the choices |
| `min`, `max` | For `slider`, `number` and `integer` |
| `pattern` | For `string`: a regular expression the value must match |
| `when` | The same conditions a menu row takes |

A `list` is a list of records a person adds to and edits.

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

`tweaks` are named switches that parts of Yapfast offer, such as
`messages.attachments.mosaic`. A plugin may set them without holding a
capability, because each one is a behaviour Yapfast already has.

## What validation refuses

| Refusal | The manifest's fault |
| --- | --- |
| Not JSON, or not this shape | A syntax error, an unknown field, an id or host pattern that isn't one |
| Another API | `api` isn't 1 |
| A path outside the plugin | `wasm` or `icon` is absolute, empty, or climbs out |
| Nothing to do | No `wasm`, and no command, menu, setting or tweak |
| Memory outside the range | `memory_pages` is below 32 or above 1024 |
| A quota too high | `http_response_bytes` or `storage_bytes` is above the most consent may allow |
| A port without its capability | `local_port` is set and `network.local` isn't declared |
| A command with nothing to say | Its name is empty, or its description is an empty string |
| Two settings with one key | Duplicate `settings[].key` |
| Depending on itself | `dependencies` names the plugin's own id |

Nothing is executed during validation: the manifest is read, the module's header
and magic are checked, its imports are checked against the two host functions the
ABI offers, and the module is compiled into the cache. A refusal reaches the
person as the sentence above, and no part of the plugin is installed.
