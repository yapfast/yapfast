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

That writes `target/plugin-fixtures/<name>.wasm` for the echo and OOM fixtures;
`scripts/check` runs it as its `fixtures-plugins` step.

## Fixtures

| File | Exports used in tests | Purpose |
| --- | --- | --- |
| `echo.wat` | `yf_activate`, `yf_deactivate`, `echo` | Activation succeeds; `echo` returns the call input unchanged. Uses only Extism 1.41 kernel imports (`extism:host/env`), no WASI, no custom host functions. |
| `bulk_echo.wat` | `yf_activate`, `yf_deactivate`, `echo` | Measurement twin of `echo.wat`: same ABI and same copy-to-a-fresh-buffer behaviour, but complete 8-byte chunks go through `input_load_u64`/`store_u64` and only the trailing `len % 8` bytes go through the byte-wide pair. |
| `oom.wat` | `yf_activate`, `yf_deactivate`, `eat_memory` | `eat_memory` calls the kernel `alloc` for 32 x 1 MiB and drops the handles, growing kernel memory past the 16 MiB (256-page) default until Extism's memory limiter fails the grow. Activation must not allocate. |
| `wasi_stdout.wat` | `write_stdout` | Writes one short line through WASI `fd_write`. The process test sets `EXTISM_ENABLE_WASI_OUTPUT=1` and checks that guest text cannot enter the helper's framed stdout. |

The echo and OOM sources assemble to modules well under the 8 MiB module cap
(418 and 192 bytes respectively); `bulk_echo.wat` is 575 bytes.

### Why `bulk_echo.wat` exists

`echo.wat` copies one byte at a time, so every input byte costs two kernel calls
(`input_load_u8` + `store_u8`). When a round-trip latency number is taken through
the helper, that per-byte guest loop and the parent/helper IPC overhead are added
together and cannot be separated from the outside. `bulk_echo.wat` is the same
guest with only the copy strategy changed: it returns every input byte unchanged,
allocates a fresh output buffer, and keeps `yf_activate` / `yf_deactivate` /
`echo` exactly as `echo.wat` declares them. Running the two under the same
harness therefore isolates the guest loop's cost from the IPC around it.

It is a measurement control, not a recommended guest, and it says nothing about
how a production plugin should copy data. Byte order needs no special handling:
`input_load_u64` and `store_u64` are inverse kernel operations, so a load/store
round trip reproduces the input exactly whatever the kernel's internal
endianness — asserted against the real kernel below, not assumed.

`bulk_echo.wat` is assembled like the other fixtures. It is not in the
`build_fixtures` list (that example currently emits the echo and OOM modules), so
the measurement harness has no path to it until the example is extended.

## Validation

Validated with Python `wasmtime` 50.0.1 (the same WAT parser wasmtime 49 uses,
which is what Extism 1.41 pins). The scratch script used for this is not part
of the repository; the checks it makes are:

- assemble each `.wat` and assert the first 8 bytes are `\0asm` + version 1;
- run `echo.wat` against (a) a stub `extism:host/env` module and (b) Extism
  1.41's real `src/extism-runtime.wasm` kernel, with inputs `b"hello"`, `b""`
  and `b"\x00\xff binary \x00"`, asserting the output equals the input;
- run `oom.wat` against the real kernel with a 256-page guard and assert it
  stops at the cap after 15 x 1 MiB allocations;
- run `bulk_echo.wat` against the same real kernel at input lengths 0, 1, 7, 8,
  9, 15, 16, 17, 1024 and 1035, for a pseudo-random binary payload (containing
  `0x00` and `0xff`) and a non-ASCII UTF-8 payload at each length, asserting the
  output bytes equal the input bytes exactly and that the output lives in a
  freshly allocated buffer rather than the input handle; for `len > 0` it also
  counts kernel calls and asserts `input_load_u64`/`store_u64` run exactly
  `len / 8` times each and `input_load_u8`/`store_u8` exactly `len % 8` times,
  which is what proves the 8-byte path is taken instead of degrading to
  byte-at-a-time.

Status: `echo.wat` is verified against the real Extism 1.41 kernel and is safe
for activation and call tests. `bulk_echo.wat` is verified against the same
kernel for all lengths listed above, with exact-byte output and the expected
kernel call counts, so it is usable as the bulk-copy arm of a latency
comparison. `oom.wat` is verified to drive memory growth past
the cap, and the real host now shows how that ends:
`a_guest_that_grows_past_the_page_limit_fails_the_call` in
[`tests/host.rs`](../../crates/yapfast-plugin-host/tests/host.rs) loads it,
calls `eat_memory`, and asserts the answer is an error whose message Extism
classifies as `oom` — a plugin fault, not a helper fault. The scratch script
above only showed the grow stopping at the cap; the classification the helper
reports is the host test's to pin.
