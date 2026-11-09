# Plugin guest fixtures

WAT sources for testing the plugin host. They are **test source only**.

Production code never reads WAT. The module loader in `yapfast-plugin-host`
accepts a wasm **binary** only: the file must start with the `\0asm` magic and
version 1, and it is handed to Extism as `Wasm::file(path)` after that check.
Wasmtime compiles text modules just as happily as binary ones, so the magic
check is the only thing standing between the content store and a text module.
Do not add a production path that parses WAT, and do not point tests at
`Wasm::data`.

Generated `.wasm` files are not checked in. Rust tests assemble a fixture at
run time with the [`wat`](https://docs.rs/wat) crate (test-only dependency)
and write the bytes to a temp file, then call the same loader production uses:

```rust
pub fn fixture_wasm(name: &str) -> (tempfile::TempDir, PathBuf) {
    let wat = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name);
    let bytes = wat::parse_file(&wat).expect("fixture WAT");
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("fixture.wasm");
    std::fs::write(&path, &bytes).expect("write fixture");
    (dir, path)
}
```

Tests that need the WAT bytes verbatim (the bad-magic regression test) can
read the `.wat` file and write it to `fixture.wasm` unchanged.

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
for activation/call tests. `oom.wat` is verified to drive memory growth past
the cap; the exact `"oom"` error string comes from Extism's
`ResourceLimiter` and has not been observed under the real host yet. Treat the
error classification as unverified until a host test asserts it.
