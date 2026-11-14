# Plugin fixtures

Nine behaviours the plugin system has to get right, each a WebAssembly module
written as text (`.wat`), with the compiled module beside it. The helper loads
the `.wasm`, not the text: a loader that accepted `(module …)` would also accept
anything else wasmtime can parse, and the magic check that refuses a text module
outside the tests is the same one that refuses it in them.

The helper's end-to-end tests
(`crates/yapfast-plugin-host/tests/e2e.rs`) load these through the real helper
and the real Extism runtime. `testing/measure_plugins.py` loads `echo.wasm` to
measure what the memory budgets buy.

| Fixture | What it does | What a test expects |
| --- | --- | --- |
| `echo.wat` | Activates, answers an event batch with the same bytes, answers a hook `"unchanged"` | `Activated`, then `EventDone`; the runtime's own tests see the bytes come back |
| `spin.wat` | Never leaves `yf_activate` | `Fault` of kind `Deadline`, and a helper still running |
| `hog.wat` | Grows linear memory until the page limit refuses | `Fault` of kind `OutOfMemory` |
| `trap.wat` | `unreachable` in `yf_activate` | `Fault` of kind `Trap` |
| `bad_activate.wat` | Returns status 1 from `yf_activate` | `Fault` of kind `ActivateFailed` |
| `bad_answer.wat` | Answers a hook with its own question, which is not a `HookOutcome` | `Fault` of kind `BadAnswer`, and the hook fails open |
| `slow_host.wat` | Calls `yf_call` with an `http.fetch` the parent answers later | a `Call` frame to the parent, then a `Return` that leaves the worker free |
| `filter.wat` | Answers `yf_hook` with a draft of its own | `HookDone` carrying that draft, which the send then goes out with |
| `spin_hook.wat` | Never leaves `yf_hook` | `Fault` of kind `Deadline`, the chain answering without it |

Every fixture exports all six of the ABI's functions (`yf_activate`,
`yf_deactivate`, `yf_event`, `yf_hook`, `yf_command`, `yf_decorate`), so one can
be loaded by any path, not only the one it was written for. The exports a
fixture doesn't exercise return status 0 with no answer.

## The text is the source, and the module is a record it stays in step with

Assembling a `.wat` needs a WebAssembly toolchain, and the dev shell has none:
building Yapfast never produces a module, so nine fixtures are not a reason to
put one in every build. The compiled module is therefore committed beside its
text, and what keeps the pair from going stale is a hash record beside them,
`fixtures.sha256`, which pins each `.wat` *and the `.wasm` assembled from it*.

```sh
wat2wasm filter.wat -o filter.wasm       # or any other text assembler
scripts/check-plugin-fixtures --update   # record the pair you just made
```

Commit the text, the module and the record together. `scripts/check` runs
`scripts/check-plugin-fixtures` as its `fixtures` step, and
`.github/workflows/tests.yml` runs `scripts/check`, so nothing here depends on
somebody remembering: a fixture edited without its module fails the check in a
second, before the first compile.

The record is a `sha256sum` file, so it can be read and checked without the
script: `cd testing/plugins && sha256sum -c fixtures.sha256`. It is a record of
bytes, not of a toolchain: assembling the same text with a different assembler
produces different bytes, which is fine, as both lines of the pair change
together.

What the check fails on, and what it says:

| Failure | What it means |
| --- | --- |
| A `.wat` or `.wasm` whose digest moved | The recorded pair is out of step: rebuild the module, then `--update` |
| A text with no module, or a module with no text | Half a fixture; the record cannot describe it |
| A module that is not version 1, or that doesn't export all six functions | Not a module the helper can load, or not one written to this ABI |
| A `.wat` or `.wasm` the record doesn't list | A fixture was added or renamed without recording it |
| A `.wasm` with the executable bit | A module is read, not run: `chmod 644` |

`--update` refuses to record a `.wat` that changed beside a `.wasm` that did
not, because that is exactly the stale pair this is for; the one edit where the
module is right to stay byte-identical is a change a compiler discards, such as
a comment, and `--update --artifact-unchanged` records that one.

`plugins/ping/plugin.wasm` is the same bargain with a different source, and its
record is checked by the same script; see `plugins/ping/README.md`. The three
folders here with no module in them (`quiet/`, `widgets/`, `word-filter/`) are
manifests the interface reads without loading any WebAssembly, so they have no
record and no part in this check.

## Writing one

A guest export takes no arguments and answers a status code: 0 means the call
worked, anything else is a failure. Extism moves the JSON in and out of the
guest itself, through the `extism:host/env` module:

- `input_length() -> i64` and `input_load_u8(offset: i64) -> i32` read the JSON
  the host handed the call.
- `alloc(bytes: i64) -> i64` returns the offset of a block in the shared
  memory, stamped with its length; `store_u8(offset: i64, byte: i32)` fills it,
  and `output_set(offset: i64, length: i64)` makes it the answer.
- `yf_call(offset: i64) -> i64` in the `extism:host/user` module is the guest
  side of a host call: it takes the offset of a block holding a `HostCall` and
  answers with the offset of one holding a `HostReturn`. `yf_post` is the same
  without an answer.

The block an `alloc` returns is the only kind the host can read, so a fixture
that answers with a literal writes it into an allocation. A module may keep
literal bytes in its own memory, as `slow_host.wat` does, and copy them across;
the kernel's memory, where `alloc` lives, is not a place to put a data segment.
`filter.wat` writes its answer one byte at a time with `store_u8`: the same 81
bytes built from `store_u64` chunks came back through `output_set` as something
the parent couldn't parse, and one byte at a time is what works.

## Checking one without the helper

Any WebAssembly runtime that validates modules will do. The one thing to watch
is the shape of an export: a signature other than `() -> i32` or `() -> ()`
fails at call time rather than at load, so the end-to-end test's first
`Activated` is what proves a new fixture links.
