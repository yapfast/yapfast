# The plugin helper

`yapfast-plugins` is the helper process that runs plugin modules for Yapfast. It
is a separate binary, not a mode of `yapfast`, because a child of the big
executable pays for relocating it before `main` runs, and because wasmtime and
Cranelift then stay out of every everyday build ([PLAN.md](../PLAN.md#32-a-separate-binary-not-yapfast---plugin-host)).
When the last instance is unloaded the helper exits, and the parent spawns a
fresh one on the next trigger, so a wasmtime bug or a compile pool's memory
cannot outlive the plugins ([PLAN.md](../PLAN.md#31-why-a-separate-process)).

The code lives in `crates/yapfast-plugin-host`, in four modules:

| Module | What it is |
| --- | --- |
| `protocol` | The loop over the framed protocol: read a request, ask the backend, write the response. Knows no WebAssembly. |
| `module` | The checks a file passes before Extism sees it: absolute path, 8 MiB, `\0asm` magic, version 1. |
| `environment` | What `main` drops from the helper's own environment before Extism reads it. |
| `runtime` | Extism behind the protocol: load, call, unload, and the plan's settings. Behind the `host` feature. |

The wire format itself, including the frame layout and the JSON envelopes, is
[the protocol's own document](plugin-protocol.md); the types live in
`crates/yapfast-plugin-api`.

## Building

Extism is behind the `host` feature and the binary requires it, so a default
workspace check builds neither the binary nor wasmtime:

```sh
cargo build --profile fast -p yapfast-plugin-host --features host
cargo test  --profile fast -p yapfast-plugin-host --features host
```

Words are not modules, so the measurement harness gets `.wasm` files assembled
from the checked-in `.wat` sources first:

```sh
cargo run --profile fast -p yapfast-plugin-host --example build_fixtures
```

That writes `target/plugin-fixtures/echo.wasm` and `oom.wasm`, the paths
`docs/plugin-measurements.md` passes to the harness. Production never reads WAT;
the example exists for the harness, which cannot run a WAT compiler itself.

## Running it

```text
usage: yapfast-plugins [--cache-dir <dir> | --no-cache] [--wasi]
```

| Flag | Meaning |
| --- | --- |
| `--cache-dir <dir>` | Keep wasmtime's compile cache under an absolute directory. The config file is `<dir>/wasmtime-cache.toml` and the cache itself is `<dir>/cache`. |
| `--no-cache` | Compile every module, with no on-disk cache. This is how the measurement harness times a cold compile. |
| `--wasi` | Give every guest a WASI context. Off by default. |
| `--help`, `-h` | Print the usage on stderr and exit 0. |

`--cache-dir` and `--no-cache` are alternatives: giving both is a usage error.

The parent spawns the helper with a cleared environment, which is what keeps
every `EXTISM_*` switch away from it
([PLAN.md](../PLAN.md#34-extism-configuration)). The helper does not rely on that
alone: `main` removes every `EXTISM_*` variable before the runtime starts, so a
helper started by the measurement harness or by hand behaves like one started by
Yapfast. `EXTISM_ENABLE_WASI_OUTPUT` would send a guest's stdout to the helper's
own stdout, which is the frame stream, and `EXTISM_CACHE_CONFIG` would move the
compile cache out from under `--cache-dir`; neither can take effect. The one
variable the helper reads is `RAYON_NUM_THREADS`, below.

The helper sets `RAYON_NUM_THREADS=2` if the parent has not, which is the plan's
setting: wasmtime compiles on rayon, and two threads keep the compile pool at
four threads instead of thirty.

### Streams and exit codes

Requests arrive on stdin, responses leave on stdout, and **nothing else is ever
written to either**: a diagnostic on stdout would corrupt the stream. Every
diagnostic goes to stderr.

| Exit code | Cause |
| --- | --- |
| 0 | `shutdown`, a clean end of stdin, or the unload of the last instance |
| 1 | A malformed frame or envelope, a response that could not be written, or a runtime that could not start |
| 2 | A command line it could not use |

A malformed wire or envelope ends the stream: both sides then exit rather than
guess where the next frame starts. A request the helper understands but cannot
serve — an unknown export, a module that is not WebAssembly, an unload of
something not loaded — is answered with `error`, and the helper keeps serving.
In M0 `error` travels one way only, from the helper to the parent: the app-side
half of the protocol, which is what the parent would answer with, arrives with the
app-side `plugins` feature.

A clean end of stdout is how the parent learns the helper has stopped; it should
not depend on the exit code alone, and M1 restarts the helper after an
unexpected exit ([PLAN.md](../PLAN.md#44-failure)).

## What a load does

`load` compiles and links the module at the request's `module` path as the
request's `plugin` id, and answers `loaded` or `error`. In order:

1. The path must be absolute. The helper resolves no paths of its own, so it
   names the path the parent checked and checks that path. The check reads the
   file's header, and wasmtime opens the same path a moment later: a file
   replaced in that window is not noticed. Pinning the bytes that actually run —
   a content hash at load time — is M2's.
2. The file must be at most 8 MiB.
3. The first eight bytes must be the WebAssembly magic and version 1.
4. Extism gets the module as `Wasm::file`, so each instance's manifest copy is a
   path rather than the module's bytes.
5. The manifest carries the plan's settings: a 256-page limit when the caller
   named none, extism vars disabled with an explicit zero, no allowed hosts, no
   preopened path, and a 30-second timeout as a last resort behind the parent's
   per-call deadlines.
6. WASI is off unless `--wasi` was given. With it on there are still no
   preopened paths, so a guest gets clocks and randomness and no filesystem.

The magic check is load-bearing, not a formality: Extism and wasmtime compile
WAT from a file just as readily as a binary, so a `.wasm` that really holds
`(module …)` would otherwise load. What the check promises is that a text module
at the path the parent checked never reaches wasmtime.

`loaded` means compiled and linked, not running. Extism builds the module's
instance lazily, on the first `call`, so a fault that only building the instance
can show — an unsatisfied import, a `start` function that traps — arrives as that
call's `error`, not as the `load`'s. (A reactor module's `_initialize` runs while
Extism links, so a fault there is still reported at `load`.)

`call` invokes the export named by the request's `method`, with the request's
JSON payload as the export's argument bytes, exactly as it arrived. M0 has no
guest method envelope: one call is one export call. The export's output is
parsed as JSON and returned as `reply`; an export that sets no output answers
`null`. An answer that is not JSON is an `error`, because the protocol's
`reply.payload` is JSON.

An M0 export returns an `i32`: zero for success. A non-zero return is a guest
failure and becomes the call's `error`; the guest sets that message with the
kernel's `error_set`, and without it Extism's own `Returned non-zero exit code`
is all the parent sees. `yf_activate` and `yf_deactivate` follow the same rule.

A guest cannot end the helper by answering too much. An answer too large for a
1 MiB frame is answered with `error` at the call's own id, and so is an error
message too large for one: the helper cuts its own error text to 4 KiB on a
character boundary. Every other loaded instance keeps answering.

`unload` drops the instance and answers `unloaded`. The unload of the last
instance ends the helper, after that answer has been written and flushed.

## The compile cache is a file, not a directory

`PluginBuilder::with_cache_config` takes the path of a wasmtime cache config
**TOML file**, and a directory is a hard error at the first compile. The file's
shape is wasmtime's, with an absolute directory:

```toml
[cache]
directory = "C:\\Users\\you\\AppData\\Local\\yapfast\\plugins\\modules"
```

`CacheConfig` writes that file, and refuses a relative directory or a relative
config file, because wasmtime refuses them too, and a directory that is not
UTF-8, because the TOML is text: writing the path lossily would cache under a
directory the helper never named. Without a configured cache, wasmtime falls back
to `EXTISM_CACHE_CONFIG` and then to its own default path, a location Yapfast
does not control — and `main` has already removed that variable, so the fallback
is wasmtime's own default only. The parent passes `--cache-dir` rather than
relying on either.

`--cache-dir <dir>` uses one layout: the config file at
`<dir>/wasmtime-cache.toml`, the cache itself in `<dir>/cache`. The config file
sits **beside** the cache directory, not in it, because wasmtime's cleanup
deletes files it does not recognise from its cache directory — a config file
placed there is gone after the first load. `CacheConfig::under` is that layout,
and a test pins it.

Two helpers can share one cache directory, so `CacheConfig::write` leaves a
config file alone when its bytes are already right: truncating it to write
identical content would let the other helper read an empty file. Only a changed
config is written.

## M0's limits

M0 is the helper skeleton and the numbers. It has no manifest parsing, no
scheduler, no lanes or deadlines of its own, no guest imports, no storage, no
hot reload, and no OS sandbox. `timeout_ms` bounds the export call but **not**
instantiation — which happens on the first call — or `_initialize`, so a module
that spins there hangs the helper; fuel is the only lever there and arrives in a
later milestone. Each compiled plugin means its own wasmtime `Engine` and its own
cache worker thread ([PLAN.md](../PLAN.md#5-threading-and-scheduling)).
