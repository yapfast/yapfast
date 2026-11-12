# Writing a plugin

A plugin is a WebAssembly module speaking the guest ABI, so it can be written in
any language with a PDK that speaks it. **AssemblyScript is the default**, and
the rest of this page is ordered by what the measurements say a plugin costs.

| PDK | Where | Builds with | Plugin | Load |
| --- | --- | --- | --- | --- |
| **AssemblyScript** | [`pdk/assemblyscript`](../../pdk/assemblyscript) | `asc`, with the plugin's own file as the entry | 11.4 KiB | **1 page, 64 KiB** |
| TinyGo | no PDK yet; see [Go](#go) | `tinygo build -target=wasip1 -buildmode=c-shared` | 143.7 KiB | 2 pages, 128 KiB |
| Rust | [`pdk/rust`](../../pdk/rust) | `cargo build --target wasm32-unknown-unknown`, `crate-type = ["cdylib"]` | 160.8 KiB | 17 pages, 1,088 KiB |
| C | [`pdk/c`](../../pdk/c) | plain `clang --target=wasm32-unknown-unknown -nostdlib`; no SDK, no WASI | 12.1 KiB | 2 pages, 128 KiB |
| Zig | [`pdk/zig`](../../pdk/zig) | `zig build-exe --target=wasm32-freestanding --export=…` per export | 56.4 KiB | 17 pages (its 1 MiB stack) |

The order is the recommendation, and it is the measurements' order rather than
anyone's taste ([all of them](../measurements/README.md#the-other-collected-languages-measured-the-same-way)):

* **AssemblyScript** first, because it is the cheapest thing measured by a wide
  margin: one page of linear memory — a quarter of C's and a seventeenth of
  Rust's — an 11.4 KiB module, and the fastest cold compile of the set. A
  plugin that does not touch the heap links no runtime at all.
* **TinyGo** second, when you want Go: two pages and 143.7 KiB by default, 6.0
  KiB with `-opt=z -gc=leaking -scheduler=none -no-debug -panic=trap`. Its
  standard toolchain is not an option — [what Go costs](#go).
* **Rust** third: the most documented, the only PDK with a published crate, and
  the one to pick when correctness matters more than a page or two. Its 17
  pages are 1 MiB of preloaded code rather than a growth curve.
* **C and Zig** when you want no runtime at all and will be careful. Zig's 17
  pages are not the plugin's data but its default 1 MiB stack, and Zig 0.17 has
  no flag to lower it; C asks for two.

Measured and **not** worth writing a PDK on: JavaScript (`extism-js` 2,425.9
KiB, and 100 pages after a single event), .NET (24,567.4 KiB and 800 pages, so
the installer refuses it), and Haskell (74–142 pages, and it retries a refused
`memory.grow` instead of failing). The numbers are in the measurements record.

## Starting here

Every PDK has a `template` folder to copy: a manifest, one handler and a build
script, all four of them the same `/hello` command, so the difference you see
is the language's.

| Template | Builds in | Produces |
| --- | --- | --- |
| [`pdk/assemblyscript/template`](../../pdk/assemblyscript/template) | a second | 11.4 KiB, 1 page |
| [`pdk/rust/template`](../../pdk/rust/template) | a cargo build | 160.8 KiB, 17 pages |
| [`pdk/c/template`](../../pdk/c/template) | a second | 10.7 KiB, 5 pages |
| [`pdk/zig/template`](../../pdk/zig/template) | a second | 55.1 KiB, 19 pages |

Each has `build.sh` for a Unix shell and `build.cmd` for Windows, so a plugin
builds the same way on either.

Everything below about Rust applies to the PDK that has a published crate; the
advice about memory applies to all of them.

Rust is the one with a published crate. Plugins are written in Rust with
`yapfast-pdk`, in
[`pdk/rust`](../../pdk/rust). The crate is published on its own and built outside
Yapfast's workspace, so a plugin author depends on `yapfast-pdk` alone and never
on the app. Its only runtime dependency is `extism-pdk`, with Extism's own HTTP
and MessagePack support off, since neither is part of this ABI and both would be
carried in the module. The crate's own floor is Rust 1.85, below the app's, so a
plugin author isn't forced onto Yapfast's toolchain.

Underneath the crate is the guest ABI described in [the ABI](abi.md): five
exports, two imports, and JSON written by the crate and not by you. The four the
interface's half adds — `yf_decorate`, `yf_member_decorate`, `yf_identity` and
`yf_code_block` — are not among them yet, and a plugin that answers one writes
that export by hand.

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

`yapfast_plugin!` writes the guest's five exports once, and the handlers you
don't implement are generated as no-ops, which is why a plugin that only adds a
command never writes a hook. None of the four exports the interface's half adds
has a handler in the crate — `yf_decorate`, `yf_member_decorate`, `yf_identity`
and `yf_code_block` are written by hand by a plugin that answers one, alongside
the generated five.

## The handlers

| Handler | Called when | An error means |
| --- | --- | --- |
| `activate` | The instance is new, or was evicted and came back | The activation failed: a fault, and the plugin is retried on its next trigger |
| `deactivate` | The instance is going away | The hand-off is dropped; the instance goes anyway |
| `answered` | A batch carries answers to calls the plugin left pending | Nothing; it returns nothing |
| `event` | Once per event in a batch, after `answered` | Logged, and the rest of the batch still runs |
| `topic` | Once per publish to a topic it subscribed to, after the batch's events | Logged, and the rest of the batch still runs |
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

`plugin.publish` and `plugin.subscribe` have no wrapper of their own: they go
through `call::send` with `wire::PLUGIN_PUBLISH` or `wire::PLUGIN_SUBSCRIBE`, and
what arrives is the `topic` handler.

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
- **Prefer the smallest runtime that fits,** which is AssemblyScript's and C's:
  no runtime in the module at all when a plugin doesn't touch the heap. A
  collected language carries its runtime inside the module, and it keeps heap it
  has finished with until the collector decides otherwise, which here means until
  the instance is evicted. A module whose runtime asks the host for clocks or
  randomness needs `wasi: true` in the manifest, which AssemblyScript's build
  doesn't and a Go build's does. Go is the language this advice costs most:
  [what Go costs](#go).

A plugin in any of them should ask for the default page limit before it asks for
more, and check what it actually costs: a module with a runtime in it starts
larger, compiles slower and holds more for longer. The examples measure it — the
C `shout` is 11.9 KiB, AssemblyScript's 17.7 KiB, Zig's 57 KiB.

### Go

**Write Go with TinyGo, not with the standard toolchain.** Both were measured
([standard toolchain](../measurements/README.md#go-as-a-plugin-language),
[TinyGo](../measurements/README.md#the-other-collected-languages-measured-the-same-way)),
and they are not close to each other. The standard toolchain misses every bar
by too much to be a close call:

| | Go, standard toolchain | Bar |
| --- | --- | --- |
| Module | 1,833.1 KiB | 512 KiB, 3.2× the Rust PDK's 160.8 KiB |
| Memory section at load | 32 pages, 2,048 KiB | Fits the default 256 pages, and is `MIN_MEMORY_PAGES` exactly: the smallest manifest a plugin may declare is what a Go runtime asks for before it runs |
| Resident per live instance | 6.4–9.6 MiB, warm compile cache | 4 MiB, from the helper's 6 MiB with one plugin alive |
| 10,000 events of 1 KiB at a 4 MiB cap | `out_of_memory` at event 2,891 | All 10,000 |

A Go module is its runtime: 1,156 KiB of machine code and 21,000 data
segments, which `-ldflags="-s -w"` takes 34.5 KiB off. The load request is the
linker's and not the program's — static data plus a 1 MiB reserve, no maximum —
so `func main() {}` and a full ABI module ask for the same 32 pages. In use, an
instance costs the helper 8.2 MiB `RssAnon` against Rust's 1.3, so its 6 MiB
target is passed by the first instance and its 16 MiB for ten by the second on
the means, the third at the low end of the samples.

The runtime's own knobs move the last row and nothing else: `SetGCPercent(10)`
and `SetMemoryLimit(3 MiB)` in `yf_activate` take a default Go plugin through
all 10,000 events at 64 pages, and no flag reaches the reserve. `GOGC` and
`GOMEMLIMIT` can't be handed to a guest, and the two calls are a plugin author's
private decision — unenforced by any PDK, invisible in the manifest.

**What would make one worth shipping.** TinyGo, measured the same way: under
512 KiB a module, under 4 MiB of linear memory after a session's events, and
under 4 MiB resident per instance. **It clears all three**, which is why it is
in the table at the top of this page:

| | TinyGo 0.43, default | TinyGo, `-opt=z -gc=leaking -scheduler=none -no-debug -panic=trap` | Bar |
| --- | --- | --- | --- |
| Module | 143.7 KiB | **6.0 KiB** | 512 KiB |
| Memory at load | 2 pages, 128 KiB | 2 pages | 256 pages |
| Memory in use | 4 pages | 2 pages | 64 pages |
| Resident per instance | ~1.0 MiB | ~0.7 MiB | 4 MiB |

Two flags are worth knowing. `-no-debug` alone takes the module from 143.7 to
11.6 KiB, because most of the default one is DWARF. `-gc=leaking
-scheduler=none` gives up goroutine scheduling and collection, which is a
plugin author's decision the same way `GOGC` was; `-scheduler=asyncify`, the
default, is what makes the default build grow to four pages on its first event.
`scheduler=none` also takes the module from 46 WASI imports to 2.

A Go plugin can also be built against Extism's own Go PDK and installed, with
`wasi: true` in its manifest. The three bars above are how its author checks
whether it fits before shipping it.

## The other PDKs, and how a plugin in each is shaped

Each of them is the same shape as the Rust one: five exports a plugin writes,
the ones it leaves out do nothing, and the JSON is never written by hand.

```c
// pdk/c/examples/shout/shout.c
void yapfast_command(yf_str invocation, yf_buf *answer) {
    yf_str text;
    if (!yf_path(invocation, YF_STR("options.text"), &text)) {
        yf_failed(answer, YF_STR("say what to shout"));
        return;
    }
    yf_notice(answer, text);
}
```

```zig
// pdk/zig/examples/shout/shout.zig
pub fn command(invocation: []const u8) ?[]const u8 {
    return "\"none\"";
}
```

```ts
// pdk/assemblyscript/examples/shout/shout.ts
class Shout extends Plugin {
  command(invocation: string): string {
    return '"none"';
  }
}
register(new Shout());
```

### Two facts that cost each of us a module with no exports

**Extism's handles are opaque.** A handle names memory Extism keeps for the
guest, and only the kernel's `store` and `load` may read or write through one.
Casting a handle to a pointer or an `ArrayBuffer` offset compiles, writes bytes
that the host never reads, and breaks every host call silently — measured both
ways in one module: a kernel store read back through a pointer is `\0`, and a
pointer write read back through the kernel is `\0`. All three PDKs marshal
through the kernel for that reason.

**A WebAssembly export section is written by the linker.** Zig needs both
`@export(&f, .{ .name = "yf_activate" })` and `--export=yf_activate` on the
command line before the function appears there; `export fn` and `@export` alone
produce a module with one export, `memory`. AssemblyScript exports the functions
of the file `asc` is pointed at and not those of a library it imports, so a
plugin names the five itself and each is a line.

### Where to look

Each folder's README says what its PDK gives a plugin, its `examples/shout` is a
whole plugin, and `template` is the same with nothing in it yet.

A module is at most 8 MiB, and each plugin with a compiled module in the helper
costs about 0.56 MiB before it has done anything, which is why size is worth
working on: build with `opt-level` for size and run `wasm-opt` over the result
before checking it in.
