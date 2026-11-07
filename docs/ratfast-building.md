# Building RatFast on Windows

RatFast is a fork of Yapfast. The sources are the same process layout Yapfast
describes in [building](building.md); this page only covers what differs when
you build it here.

## The two supported routes

| Route | What it gives you | Cost |
| --- | --- | --- |
| GitHub Actions (`Build` workflow) | Linux AppImage + Windows ZIP as artifacts, built by the upstream Nix `windows` shell | Free, ~30–60 min per run, no local toolchain |
| Local Windows toolchain on `D:` | `cargo check`, `cargo test`, and the crates that don't need Skia | Needs a Skia archive and `gn`, see below |

The GitHub route is the one that produces a runnable binary today. The local
route compiles everything except the desktop binary, because `skia-safe` refuses
to build without a prepared static Skia archive.

## GitHub Actions

`.github/workflows/ratfast-dev.yml` builds on every push to `ratfast-dev`:

```text
Actions -> Build -> the run -> Artifacts -> linux, windows
```

`linux` holds `Yapfast-<version>-x86_64.AppImage` and its Arch package;
`windows` holds `Yapfast-<version>-windows-x64.zip`. Both are named `Yapfast`
because the internal package names are unchanged (see Branding below).

`Nightly` publishes a prerelease instead of only artifacts. It checks out `main`
unless you pass its `ref` input, so dispatching it for `ratfast-dev` needs that
input set.

Triggering either workflow through the REST API needs a token with the
`workflow` scope. A token with only `repo` can push code but gets `404` from
`/actions/workflows`, while push events still start the build.

## Branding

The display name is RatFast in `packaging/windows/yapfast.iss` (installer,
Start-menu and desktop shortcuts, the Run key, the `yapfast:` protocol
handler's description). Everything else keeps the `yapfast` internal name:
crate names, the executable, the `io.github.yapfast.Yapfast` app id, the data
folder, and the registry key the installer writes. Renaming those would break
path dependencies, the keyring entry and upgrades from an installed Yapfast,
and the upstream GPL-3.0 attribution has to keep naming its origin.

## The local Windows toolchain (D: only)

Set these before the first `cargo` call, or the tool falls back to `C:`:

```bash
export CARGO_HOME=D:/RatFast/cache/cargo
export RUSTUP_HOME=D:/RatFast/cache/rustup
export CARGO_TARGET_DIR=D:/RatFast/app/target
export TEMP=D:/RatFast/temp TMP=D:/RatFast/temp
export PATH="/d/RatFast/cache/cargo/bin:/d/RatFast/tools/cmake-dist/cmake-3.30.5-windows-x86_64/bin:/d/RatFast/tools/ninja:/d/RatFast/tools/nasm-dist/nasm-2.16.03:/d/RatFast/toolchains/llvm-21/bin:$PATH"
```

Installed there:

- `D:/RatFast/cache/cargo`, `D:/RatFast/cache/rustup` — cargo home and rustup
  home. `rust-toolchain.toml` pins `1.97.1`, with `rustfmt` and `clippy`, and
  `x86_64-pc-windows-msvc` is added.
- `D:/RatFast/tools/` — portable CMake 3.30.5, Ninja 1.12.1, NASM 2.16.03.
- `D:/RatFast/toolchains/llvm-21/` — LLVM 21.1.7, the version
  `nix/prepare-windows-skia.py` requires by name.
- `D:/RatFast/app/.local/windows/skia-source/` — the pinned Skia source
  (`61e7ca4e`) with its seven `third_party/externals` dependencies.

What compiles today:

```bash
cd D:/RatFast/app
cargo check -p yapfast-discord      # passes
cargo test  -p yapfast-discord --lib # 259 tests
cargo check -p yapfast-core         # passes, needs LIBCLANG_PATH
```

What does not: `cargo check -p yapfast` (the desktop binary) — `skia-bindings`
downloads a prebuilt archive that upstream does not publish for this target and
aborts with `Refusing to full-build skia with no-compile feature`.

## What the desktop binary still needs

`nix/prepare-windows-skia.py` builds the static (`/MT`) Skia archive the binary
links against. It needs, none of which are on `D:` yet:

- `gn` — Chromium's build generator. `vcpkg` has no port for it, the CIPD
  bucket URL 404s, and the `gn.googlesource.com` tree ships no Python bootstrap.
- `cargo-xwin` — `cargo install cargo-xwin` crashed rustc under full
  parallelism (`STATUS_IN_PAGE_ERROR`, `0xc0000006`); retry with
  `CARGO_BUILD_JOBS=2` or a prebuilt binary.
- the `xwin` SDK cache in `D:/RatFast/cache/cargo-xwin`.

Until those exist, GitHub Actions is the supported build.
