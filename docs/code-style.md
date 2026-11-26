# Code style

| Files | Tool and configuration |
| --- | --- |
| Rust | rustfmt from [`rust-toolchain.toml`](../rust-toolchain.toml), standard settings |
| Python, including shebang scripts | Ruff lint and format; [`ruff.toml`](../ruff.toml) |
| C/C++ | clang-format; [`.clang-format`](../.clang-format) |
| JSON, YAML, TOML, XML | Built-in prek syntax checks |
| Markdown, JS/TS, HTML, CSS, shell, Nix, Slint | Built-in prek text checks only |

[prek.toml](../prek.toml) defines the hooks and pinned tool versions. Prek manages
cached tool environments for Ruff and clang-format. Rustfmt uses the installed
Rust toolchain and checks formatting without changing files; other formatters apply changes.
Built-in hooks check trailing whitespace, final newlines and merge conflicts.

Built-in hooks enforce LF line endings (CRLF for `.bat` and `.cmd`) and a final
newline. Markdown permits trailing spaces for hard line breaks. Indentation is
handled by language formatters where configured; text checks do not enforce it.
Editors can use `.clang-format` directly for the same C/C++ style as the hook.

Hooks exclude vendored and generated paths, lockfiles, patches, SVG files and font
licenses as listed in `prek.toml`. Do not manually format generated or vendored files.

## Commands

See [Contributing](../CONTRIBUTING.md#lint-and-formatting) for setup.

```sh
# Check and fix the files you changed.
prek run --files path/to/file.py

# Run one hook across tracked files.
prek run ruff-check --all-files
```

Inspect automatic changes and rerun until all hooks pass. A hook that modifies
files fails that run. Apply Rust formatting with `cargo fmt --all` in the dev shell.

## CI

The repository-wide formatting baseline is handled separately, so
`prek run --all-files` may modify unrelated files or report existing violations.
Prefer focused checks while working.

The formatting job (prek, including rustfmt) and dependency job (cargo-audit and
cargo-machete) in [CI](../.github/workflows/ci.yml) are temporarily commented out
until the dependency-update and formatting-baseline PRs are merged. Active CI
runs strict Clippy and workspace tests on Linux in parallel.
Local `scripts/check --fast` also checks Rust formatting and documentation links,
then runs Clippy, tests, a build and a smoke test.

Workflows name GitHub's own actions (`actions/*`) by their major tag, such as
`actions/checkout@v7`. Every other action is pinned to a full commit SHA with
its release in a comment, such as
`oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0`, because
anyone who controls that repository can move a tag to new code.
