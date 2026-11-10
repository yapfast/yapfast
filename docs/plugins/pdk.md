# Writing a plugin

Plugins are written in Rust with `yapfast-pdk`, in
[`pdk/rust`](../../pdk/rust). The crate is published on its own and built outside
Yapfast's workspace, so a plugin author depends on `yapfast-pdk` alone and never
on the app. Its only runtime dependency is `extism-pdk`, with Extism's own HTTP
and MessagePack support off, since neither is part of this ABI and both would be
carried in the module. The crate's own floor is Rust 1.85, below the app's, so a
plugin author isn't forced onto Yapfast's toolchain.

Underneath the crate is the guest ABI described in [the ABI](abi.md): six
exports, two imports, and JSON written by the crate and not by you.

## Starting from an example

[`pdk/rust/examples/shout`](../../pdk/rust/examples/shout) is a whole plugin in
two files: `yapfast-plugin.json`, which is [the manifest](manifest.md), and
`src/lib.rs`. It adds `/shout`, which sends the person's text in capitals, and it
is the smallest thing that does something useful.

Its `Cargo.toml` is a crate that produces a WebAssembly module and nothing else:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
yapfast-pdk = "0.1"

[workspace]
```

`crate-type = ["cdylib"]` is what makes a module out of the crate. The empty
`[workspace]` keeps a plugin out of whatever workspace it is dropped into: a
plugin is not built by Yapfast, and building one must not need Yapfast's
toolchain.

```sh
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/shout.wasm plugin.wasm
```

`plugin.wasm` beside the manifest is what `wasm` names, and what an install
copies into the store. `wasm32-unknown-unknown` is the only supported target:
the module runs in Extism, which supplies the imports.

## A plugin end to end

```rust
use yapfast_pdk::prelude::*;

/// A plugin is one type, created once per instance.
#[derive(Default)]
struct Shout;

impl Plugin for Shout {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        // The composer checked `text` against the manifest before the plugin was
        // called; `required_string` only turns its absence into a message.
        let text = command.required_string("text")?;
        call::ok(
            method::MESSAGES_SEND,
            json!({ "body": text.to_uppercase(), "reply": true }),
        )?;
        Ok(Reply::None)
    }
}

// Exports the functions Yapfast calls; the handlers not written above do nothing.
yapfast_plugin!(Shout);
```

Reading it in order: what the person typed arrives as a `Command`, whose options
are the ones the manifest declared, so `text` is a string that is there.
`call::ok` asks Yapfast to send, which is a host call with an answer, and it
fails if the answer isn't an `ok` — a refusal, a method this build doesn't have,
or a failure all become an error with a sentence in it. `Reply::None` says there
is nothing else to show, since the send is the visible result.

`yapfast_plugin!` writes the guest's exports once, and the handlers you don't
implement are generated as no-ops, which is why a plugin that only adds a command
never writes a hook. `yf_decorate`, the export that draws beside a row, has no
handler in the crate yet: a plugin that decorates writes that export by hand.

## The handlers

| Handler | Called when | An error means |
| --- | --- | --- |
| `activate` | The instance is new, or was evicted and came back | The activation failed: a fault, and the plugin is retried on its next trigger |
| `deactivate` | The instance is going away | The hand-off is dropped; the instance goes anyway |
| `answered` | A batch carries answers to calls the plugin left pending | Nothing; it returns nothing |
| `event` | Once per event in a batch, after `answered` | Logged, and the rest of the batch still runs |
| `hook` | Before an outgoing message, edit, typing indicator, upload or activity is published | The message goes as it was: hooks fail open |
| `command` | A person typed one of the plugin's commands | Shown to them as the command's failure |

`activate` runs once per instance and before anything else, so it is where a
plugin reads its storage, asks for its timers and checks its settings. A handler
that answers with data rather than acting, `hook`, has its answer checked before
it is believed: a `changed` subject replaces the whole subject, so a plugin that
changes one field copies the rest from what it was given.

## Host calls

A plugin acts by asking Yapfast. Each group has a module, and each method a typed
wrapper:

| Module | Does |
| --- | --- |
| `storage` | `get`, `set`, `delete`, `list`: the plugin's own key-value store |
| `settings` | `get`: one of the settings the manifest declares, as the person set it |
| `http` | `fetch`: one request through the network gate |
| `timers` | `after`, `every`, `cancel`: wake me later |
| `app` | `version`, `version_string`: what Yapfast this is |
| `log` | `error`, `warn`, `info`: lines in the plugin's own ring, shown on its page |

A method newer than the crate you depend on is still reachable with `call::send`
and a method name, and answers `unsupported` rather than failing to link, which
is how a plugin checks whether a Yapfast has something before relying on it.

### A call that answers later

Most calls answer while the plugin's function is running. Two kinds may not: an
HTTP request, because the person may have to allow the host first, and anything
that waits on a dialog. Those answer `pending` with a tag, and the helper calls
the plugin again when the answer arrives.

```rust
use yapfast_pdk::prelude::*;

#[derive(Default)]
struct Weather {
    /// The request whose answer hasn't arrived. A field is a cache, so this is
    /// the one thing that may be lost when the instance goes.
    waiting: Option<call::Pending>,
}

impl Plugin for Weather {
    fn answered(&mut self, ctx: &Context<'_>) {
        let Some(pending) = self.waiting.clone() else { return };
        let Some(answer) = ctx.answer(&pending).cloned() else { return };
        self.waiting = None;
        match call::value(method::HTTP_FETCH, answer) {
            Ok(value) => log::info(format!("the forecast arrived: {value}")),
            Err(error) => log::error(format!("the forecast failed: {error}")),
        }
    }

    fn event(&mut self, _ctx: &Context<'_>, event: Event) -> Result<()> {
        if let Event::MessageCreate(envelope) = event {
            if envelope.room.as_deref() == Some("room-1") {
                let request = http::HttpRequest::get("https://api.example.com/now");
                if let http::Fetched::Pending(pending) = http::fetch(&request)? {
                    self.waiting = Some(pending);
                }
            }
        }
        Ok(())
    }
}
```

`http::get` is the shorthand for a request that must answer at once, and it is
an error when the answer is deferred, so reaching for `fetch` is how a plugin
says it can wait. Nothing is held open while it does: the call returns, the
worker is free, and the answer arrives in the next batch.

## The eviction contract

**An instance may vanish between any two calls.** Yapfast evicts idle instances
after five minutes, at once when the window closes to the tray, and when its
memory budget is full, and an instance that hit its page limit is discarded
because WebAssembly memory can't shrink. Nothing tells a plugin it is about to
go.

Three things follow, and they shape a plugin more than the API does:

- **Fields are a cache.** Anything that must be there after a batch goes in
  `storage`, and a plugin reads it back in `activate`, which is the only notice
  it gets that it was evicted.
- **`deactivate` is a convenience, not a lifeline.** Its answer is handed to the
  next `activate`, and it is capped at 256 KiB, which is far below what storage
  holds. It is for a cheap hand-off, and storage is for everything else. An
  eviction isn't guaranteed to be preceded by it at all: an instance that trapped
  or was over its deadline has no chance to say anything.
- **Timers and requests keep an instance alive.** A plugin holding a timer or a
  request in flight isn't idle and isn't evicted under it. The timer belongs to
  the helper rather than the instance, so a repeating one survives eviction and
  wakes the plugin again.

A plugin's storage is the only thing that is certain to survive, and the only
thing counted against its quota. Writes are batched and debounced by the helper,
so a value read back in the same call is what was last written.

## Memory, and the languages that make it hard

The guest's memory is capped by the manifest, and the cap is a ceiling rather
than a reservation: only the pages a plugin touches cost anything. Above it the
instance traps, and since WebAssembly memory can't be given back, an instance
that reached its limit is discarded and replaced by a fresh one. Its pages go
back to the system when that happens, and not before.

That makes two habits worth having:

- **Buffer as little as possible.** Every byte a plugin formats, collects or
  clones is a byte its instance keeps for the rest of its life. Handle one event
  at a time, and let a value go when the batch ends rather than accumulating a
  list of everything seen.
- **Prefer a language without a garbage collector,** and Rust first. A collected
  language carries its runtime inside the module, and it tends to keep heap it
  has finished with until the collector decides otherwise, which here means until
  the instance is evicted. Go and JavaScript builds also need `wasi: true` in the
  manifest, since their runtimes ask the host for clocks and randomness.

Extism's other PDKs can talk to the same JSON API without support from this
project, and [the ABI](abi.md) is what they follow. A plugin in one of them
should ask for the default page limit before it asks for more, and check what it
actually costs: a module with a runtime in it starts larger, compiles slower and
holds more for longer.

A module is at most 8 MiB, and each plugin with a compiled module in the helper
costs about 0.56 MiB before it has done anything, which is why size is worth
working on: build with `opt-level` for size and run `wasm-opt` over the result
before checking it in.
