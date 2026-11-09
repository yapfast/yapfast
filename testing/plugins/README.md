# Plugin guest fixtures

WAT sources for testing the plugin host. They are **test source only**.

Production code never reads WAT. The module loader in `yapfast-plugin-host`
accepts a wasm **binary** only: the file must start with the `\0asm` magic and
version 1, and it is handed to Extism as `Wasm::file(path)` after that check.
Wasmtime compiles text modules just as happily as binary ones, so the magic
check is the only thing standing between the content store and a text module.
Do not add a production path that parses WAT, and do not point tests at
`Wasm::data`.

Generated `.wasm` files are not checked in. The Rust tests assemble a fixture at
run time with the [`wat`](https://docs.rs/wat) crate (test-only dependency),
write the bytes to a temp file, and hand that file to the same loader production
uses. Those helpers are in
[`crates/yapfast-plugin-host/tests/support/mod.rs`](../../crates/yapfast-plugin-host/tests/support/mod.rs):

```rust
wasm_fixture("echo.wat")  // (TempDir, PathBuf): assembled into a temporary .wasm
text_fixture("echo.wat")  // the .wat bytes verbatim, so the loader refuses them
fixture_path("echo.wat")  // testing/plugins/echo.wat
```

`wasm_fixture` is what a module test wants; `text_fixture` exists for the
bad-magic regression test, which needs a file that is not a WebAssembly binary.
Keep the returned `TempDir` alive: dropping it deletes the module.

The measurement harness cannot run a WAT compiler either, so it is handed real
modules by the host crate's fixture example:

```sh
cargo run --locked --profile fast -p yapfast-plugin-host --example build_fixtures
```

That writes `target/plugin-fixtures/<name>.wasm` for both files; `scripts/check`
runs it as its `fixtures-plugins` step.

## Fixtures

| File | Exports used in tests | Purpose |
| --- | --- | --- |
| `echo.wat` | `yf_activate`, `yf_deactivate`, `echo` | Activation succeeds; `echo` returns the call input unchanged. Uses only Extism 1.41 kernel imports (`extism:host/env`), no WASI, no custom host functions. |
| `oom.wat` | `yf_activate`, `yf_deactivate`, `eat_memory` | `eat_memory` calls the kernel `alloc` for 32 x 1 MiB and drops the handles, growing kernel memory past the 16 MiB (256-page) default until Extism's memory limiter fails the grow. Activation must not allocate. |

Both files assemble to modules well under the 8 MiB module cap (418 and 192
bytes respectively).

## Validation

Validated with Python `wasmtime` 50.0.1 (the same WAT parser wasmtime 49 uses,
which is what Extism 1.41 pins). The scratch script used for this is not part
of the repository; the checks it makes are:

- assemble each `.wat` and assert the first 8 bytes are `\0asm` + version 1;
- run `echo.wat` against (a) a stub `extism:host/env` module and (b) Extism
  1.41's real `src/extism-runtime.wasm` kernel, with inputs `b"hello"`, `b""`
  and `b"\x00\xff binary \x00"`, asserting the output equals the input;
- run `oom.wat` against the real kernel with a 256-page guard and assert it
  stops at the cap after 15 x 1 MiB allocations.

Status: `echo.wat` is verified against the real Extism 1.41 kernel and is safe
for activation and call tests. `oom.wat` is verified to drive memory growth past
the cap, and the real host now shows how that ends:
`a_guest_that_grows_past_the_page_limit_fails_the_call` in
[`tests/host.rs`](../../crates/yapfast-plugin-host/tests/host.rs) loads it,
calls `eat_memory`, and asserts the answer is an error whose message Extism
classifies as `oom` — a plugin fault, not a helper fault. The scratch script
above only showed the grow stopping at the cap; the classification the helper
reports is the host test's to pin.
