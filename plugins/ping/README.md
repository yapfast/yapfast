# Ping

The first Yapfast plugin: one command, no capability, and nothing but an answer.

```sh
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ping.wasm plugin.wasm
scripts/check-plugin-fixtures --update
```

`plugin.wasm` beside `yapfast-plugin.json` is what an install copies into the
store. With the feature on, put this folder in the data directory's plugin store
and enable `com.example.ping`; the helper is started by the first `/ping` and
exits when the last instance goes.

## The module, and what keeps it current

Building a plugin needs a WebAssembly toolchain, and Yapfast's build has none:
the repository builds the app, not the plugins in it. So `plugin.wasm` is
committed beside its manifest and nothing compiles it, and what keeps it from
silently going stale is `plugin.wasm.sha256`, a `sha256sum -c` file (from the
recipe above) that pins the module *and the files it was built from*: `src/`,
`Cargo.toml`, `Cargo.lock` and `.cargo/config.toml`, since the profile and the
locked dependency versions change what comes out as much as the code does.

`scripts/check-plugin-fixtures` checks it, and `scripts/check` runs that as its
`fixtures` step, so the test that installs this plugin cannot pass against a
module older than its source. It fails on:

| Failure | What it means |
| --- | --- |
| A file under `src/`, `Cargo.toml`, `Cargo.lock` or `.cargo/config.toml` changed since the module was built | Rebuild it and run `scripts/check-plugin-fixtures --update` |
| `id` is not `com.example.ping` | The test that installs this plugin expects that one |
| `api` is not the `API_VERSION` this build speaks | The module carries the api it was compiled for, so a moved API needs it rebuilt on a machine with the wasm target; changing this number alone would lie about the module |
| Not a version 1 module, or missing one of the five functions the PDK generates | Not a module the helper can load |
| The executable bit | It is committed 0644: a module is read, not run |

`--update` refuses to record a changed source beside an unchanged module, so
the record cannot be talked into blessing a rebuild you forgot. `pdk/rust` is
deliberately not in the record: it is a dependency of every plugin and editing
it here, which needs no toolchain, must not block the repository on one that
rebuilds this module. The `api` check above is what catches the change to the
PDK that matters.

`testing/plugins/` does the same for its nine `.wat` fixtures, whose text is the
source; see `testing/plugins/README.md`.

## What it does

`/ping` answers `pong`, in the room it was typed in and only to the person who
typed it. That is a command's answer, not a message: `Reply::notice` is a line
the person reads, so the plugin needs no capability at all, and nothing leaves
the machine.

The notice is worth contrasting with a message, because the difference is the
scope of what a plugin can do here today:

| What it wants to do | Today |
| --- | --- |
| Answer a command | Works, and is what this plugin does |
| Send a message into a room | `messages.send` is answered `unsupported` in this build; the API names it and the runtime has no handler |
| Draw a row in a context menu | The manifest declares it (below); nothing dispatches it yet |

## The context-menu declaration

`yapfast-plugin.json` carries a `menus` entry:

```json
{ "target": "message", "label": "Ping this message", "action": "ping-message" }
```

That is the shape the API is built around, and it is deliberately a manifest
field rather than code: the interface can show the row without loading any
WebAssembly, which is what keeps the helper process out of every menu that has
one row in it. The row is drawn — `MenuDecl` is read where the manifest is
(`crates/yapfast-core/src/plugins.rs`) and becomes the interface's own menu row,
and a click on it is dispatched as `Input::Menu` with this plugin's id and the
row's action (`src/ui/plugins.rs`).

The last step is the application's: the click is turned into a request the
helper answers with `yf_input`, which is how every interactive surface reaches a
plugin — a menu row, a widget's toggle, a dismissed notice. The PDK exports it
for every plugin, and this one keeps the default `Plugin::input`, which does
nothing, so the click arrives and is ignored. The export stays optional in the
ABI: the helper reads the export section before calling it, so a module built
without a PDK can leave it out rather than be discarded for not listening.

This plugin declares the row anyway on purpose: the first plugin is a good place
to have the thing that doesn't work yet written down in the form it will work in,
rather than discovered later.

## The manifest, field by field

| Field | Why |
| --- | --- |
| `activation.on: ["command", "menu"]` | Two ways to wake it: typing `/ping`, and the menu row once it is dispatched |
| `permissions.capabilities: []` | It sends nothing and reads nothing; an empty list is a promise the review dialog can show |
| `commands` | The name and description the `/` list draws, with no options |
| `menus` | The context-menu row, above |
