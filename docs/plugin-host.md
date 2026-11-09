# The plugin helper

`yapfast-plugins` is the helper process that runs plugin modules for Yapfast. It
is a separate binary, not a mode of `yapfast`, because a child of the big
executable pays for relocating it before `main` runs, and because wasmtime and
Cranelift then stay out of every everyday build ([PLAN.md](../PLAN.md#32-a-separate-binary-not-yapfast---plugin-host)).
When the last instance is unloaded the helper exits, and the parent spawns a
fresh one on the next trigger, so a wasmtime bug or a compile pool's memory
cannot outlive the plugins ([PLAN.md](../PLAN.md#31-why-a-separate-process)).

The code lives in `crates/yapfast-plugin-host`, in three layers:

| Module | What it is |
| --- | --- |
| `protocol` | The loop over the framed protocol: read a request, ask the backend, write the response. Knows no WebAssembly. |
| `module` | The checks a file passes before Extism sees it: absolute path, 8 MiB, `\0asm` magic, version 1. |
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

The parent spawns the helper with a cleared environment, which is what blocks
every `EXTISM_*` switch, including `EXTISM_ENABLE_WASI_OUTPUT`
([PLAN.md](../PLAN.md#34-extism-configuration)). M0's helper does not clear its
own environment, so it relies on the parent's `env_clear()`; nothing else in the
helper reads the environment except `RAYON_NUM_THREADS`, below.

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

A clean end of stdout is how the parent learns the helper has stopped; it should
not depend on the exit code alone, and M1 restarts the helper after an
unexpected exit ([PLAN.md](../PLAN.md#44-failure)).

## What a load does

`load` instantiates the module at the request's `module` path as the request's
`plugin` id, and answers `loaded` or `error`. In order:

1. The path must be absolute. The helper resolves no paths of its own, so the
   file the parent checked is the file the helper loads.
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
`(module …)` would otherwise load.

`call` invokes the export named by the request's `method`, with the request's
JSON payload as the export's argument bytes, exactly as it arrived. M0 has no
guest method envelope: one call is one export call. The export's output is
parsed as JSON and returned as `reply`; an export that sets no output answers
`null`. An answer that is not JSON is an `error`, because the protocol's
`reply.payload` is JSON.

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
config file, because wasmtime refuses them too. Without a configured cache,
wasmtime falls back to `EXTISM_CACHE_CONFIG` and then to its own default path, a
location Yapfast does not control — so the parent passes `--cache-dir` rather
than relying on either.

`--cache-dir <dir>` uses one layout: the config file at
`<dir>/wasmtime-cache.toml`, the cache itself in `<dir>/cache`. The config file
sits **beside** the cache directory, not in it, because wasmtime's cleanup
deletes files it does not recognise from its cache directory — a config file
placed there is gone after the first load. `CacheConfig::under` is that layout,
and a test pins it.

## M0's limits

M0 is the helper skeleton and the numbers. It has no manifest parsing, no
scheduler, no lanes or deadlines of its own, no guest imports, no storage, no
hot reload, and no OS sandbox. `timeout_ms` bounds the export call but **not**
instantiation or `_initialize`, so a module that spins in `start` hangs the
helper; fuel is the only lever there and arrives in a later milestone. Each
compiled plugin means its own wasmtime `Engine` and its own cache worker thread
([PLAN.md](../PLAN.md#5-threading-and-scheduling)).
