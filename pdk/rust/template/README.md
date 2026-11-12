# {{project-name}}

A Yapfast plugin: the manifest in `yapfast-plugin.json` and a WebAssembly
module it names. It adds one command, `/hello`, which shows a line to the
person who typed it.

## What is here

| File | What it is for |
| --- | --- |
| `yapfast-plugin.json` | The manifest: the plugin's id, version, permissions, and the command the composer lists |
| `src/lib.rs` | The handlers, and the macro that exports them |
| `.cargo/config.toml` | Builds for `wasm32-unknown-unknown` in release, optimised for size |
| `plugin.wasm` | Not here yet: it is what `cargo build` makes, and what the manifest names |

## Build it

```sh
rustup target add wasm32-unknown-unknown
cargo build --release
cp target/wasm32-unknown-unknown/release/plugin.wasm plugin.wasm
```

The `.cargo/config.toml` already sets the target and the release profile, so
the copy is the only bookkeeping: the manifest names `plugin.wasm`, beside
itself. `cargo build` without `--release` also works and is quicker, but the
module is several times larger, and a plugin's pages count against the memory
budget for as long as its instance lives.

If `wasm-opt` is installed (it is part of Binaryen), it takes another half off:

```sh
wasm-opt -Oz --strip-debug plugin.wasm -o plugin.wasm
```

Run it on the copied `plugin.wasm`, after `cargo build`: `cargo` overwrites its
own output, not this one.

On Windows, `build.cmd` does the three lines above (it copies whatever `.wasm` cargo wrote).

## Try it

While a plugin is being written, Yapfast can load it from a folder rather than
from an installed copy: point it at this directory, whose manifest and
`plugin.wasm` are all it needs. Enable the plugin, then type `/hello` in any
room. Rebuild and copy the module again to see a change, and reload the plugin
to see it.

## Where to go next

- The manifest's `permissions.capabilities` is what the person is asked to
  allow. A command that only answers needs nothing; sending a message needs
  `messages.send`, and reaching a host over HTTPS needs it listed under
  `permissions.network`.
- `Plugin`'s other handlers are `activate`, `deactivate`, `answered`, `event`
  and `hook`; each one nobody implements does nothing, so add them as they are
  needed.
- Anything that must outlive the plugin's instance goes in `storage`: the
  helper may drop an idle instance between any two calls, and its fields go
  with it.
