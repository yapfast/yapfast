# yapfast-pdk

Write Yapfast plugins in Rust, compiled to WebAssembly.

A Yapfast plugin is a WebAssembly module. Yapfast runs it inside a small helper
process of its own, so a plugin that is idle costs nothing, and a plugin that
misbehaves costs its process and not the app. The module exports one function
per kind of call and imports two host functions; everything that crosses is
JSON. This crate writes both ends of that for you: the exports are generated
from a trait, the host functions are typed, and no plugin writes JSON by hand.

```rust
use yapfast_pdk::prelude::*;

#[derive(Default)]
struct Shout;

impl Plugin for Shout {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        let text = command.required_string("text")?;
        Ok(Reply::notice(text.to_uppercase()))
    }
}

yapfast_plugin!(Shout);
```

## A plugin is two files

`yapfast-plugin.json`, written by hand, and `plugin.wasm`, made by `cargo
build`. The manifest names the module beside it and says what the person is
asked to allow:

```json
{
  "api": 1,
  "id": "com.example.shout",
  "name": "Shout",
  "version": "1.0.0",
  "wasm": "plugin.wasm",
  "activation": { "on": ["command"] },
  "permissions": { "capabilities": ["messages.send"] },
  "commands": [
    {
      "name": "shout",
      "description": "Send your text in capitals",
      "options": [{ "name": "text", "type": "string", "required": true }]
    }
  ]
}
```

The `id` is reverse-DNS and lowercase; it is what a person sees beside anything
the plugin draws, and it names the plugin's storage, so it can't change after
the first release. `permissions.capabilities` is the list the person consents
to; a host reached over HTTPS is named under `permissions.network` as well.

## Writing a plugin

Start from this crate's `template/` directory, which the `cargo generate`
command fills in with a name and an id:

```sh
cargo generate --path /path/to/yapfast/pdk/rust/template
```

or copy the example plugin in `examples/shout/`, which is the whole of a plugin
that adds one command.

A plugin is one type with a handler per kind of call, all of them optional:

| Handler | Runs when |
| --- | --- |
| `activate` | The instance was created, first thing, and after every eviction |
| `deactivate` | The instance is about to be dropped; it may hand a string to the next `activate` |
| `answered` | Answers to calls the plugin left pending arrived, before that call's events |
| `event` | Something happened in Yapfast: a message, a reaction, a room was opened |
| `hook` | A message, an edit, a typing indicator, an upload or an activity is about to go out |
| `command` | Someone typed one of the plugin's commands |

`yapfast_plugin!(Type)` generates the five exports the helper calls. Handlers
nobody implements are no-ops, so a plugin that only adds a command is the
command.

Everything else the plugin needs is a module: `storage`, `settings`, `http`,
`timers`, `app` and `log`. They are ordinary functions that make one host call
each and return a typed answer, or an error with a reason a person can read.

## The instance may vanish

A plugin's instance is a cache and nothing else. Yapfast drops idle instances
to give memory back, so an instance may disappear between any two calls, and
`activate` is the only notice a plugin gets that it did. Fields are gone when
that happens; anything that must be there next time goes in `storage`, which
belongs to the plugin rather than to its instance, and is read back in
`activate`. Do not hold a value in a field across calls that storage could hold
instead, and do not assume `deactivate` ran: it is a convenience for a clean
hand-off, not a promise.

## Memory

WebAssembly memory only grows. An instance that peaked at 2 MiB keeps that
footprint until it is evicted, and the pages a plugin asks for are reserved when
it is instantiated. That is why a language without a garbage collector is a
better fit for a plugin: a collected runtime carries its heap inside the module,
and it tends to hold on to memory it has finished with, so both the runtime and
its habits cost the person who runs the plugin. Rust is the language this
crate is built for and the one the template uses. Nothing here shuffles data it
was already given, and a plugin should not either: decode what you need, let the
rest drop, and stream what is large through a file rather than through memory.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo build --release
```

The template's `.cargo/config.toml` sets the target and an `opt-level = "z"`
release profile, which is the profile a module is shipped from: a plugin is
compiled once and then lives in memory, so size matters more than the last few
percent of speed. `wasm-opt -Oz` from Binaryen takes another half off, and is
worth running on the module you ship.

## What this crate is not

It is not Yapfast. The plugin ABI is defined by Yapfast, and the wire types in
`src/wire.rs` are this crate's copy of it, so a plugin depends on the PDK alone
and not on the app. A copy that drifts is a bug in the PDK: the shapes are
byte-compatible with the app's, field for field. The ABI version in a manifest
is what changes when a shape changes in a way a written plugin would feel.
