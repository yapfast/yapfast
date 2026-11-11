# Plugin fixtures

Nine behaviours the plugin system has to get right, each a WebAssembly module
written as text (`.wat`), with the compiled module beside it. The helper loads
the `.wasm`, not the text: a loader that accepted `(module …)` would also accept
anything else wasmtime can parse, and the magic check that refuses a text module
outside the tests is the same one that refuses it in them. The text is what a
person edits; regenerating its module needs a WebAssembly toolchain, so the
compiled file is checked in with it.

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
