# Code style

## Configuration

| Scope                                  | Source                                          |
| -------------------------------------- | ----------------------------------------------- |
| Editor defaults                        | [`.editorconfig`](../.editorconfig)             |
| Hooks, tool versions, Prettier options | [`prek.toml`](../prek.toml)                     |
| C/C++                                  | [`.clang-format`](../.clang-format)             |
| Python                                 | [`ruff.toml`](../ruff.toml)                     |
| Rust toolchain                         | [`rust-toolchain.toml`](../rust-toolchain.toml) |

The configuration files define the rules. Keep this document in sync with changes to them.

## Tool selection

The primary goal is short local feedback cycles and low CI overhead. Prefer native tools,
reuse prepared environments, and avoid running separate programs for overlapping checks.

| Tool                                                         | Reason                                                                                                                                                                                       |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [prek](https://github.com/j178/prek)                         | Rust hook runner distributed as a single binary. Built-in text and syntax checks avoid separate Python hook processes. Cached tool environments avoid repeated installation.                 |
| [ruff](https://github.com/astral-sh/ruff)                    | Native Rust implementation with caching. One tool provides Python lint, import sorting and formatting, replacing separate Flake8, isort and Black installations for the configured checks.   |
| [clang-format](https://clang.llvm.org/docs/ClangFormat.html) | Native C/C++ formatter with configurable style. Formatting does not require compiling the application or resolving its native build dependencies.                                            |
| [taplo](https://github.com/tamasfe/taplo)                    | Dedicated Rust TOML formatter. Handles manifests directly without adding a TOML plugin to the JavaScript toolchain.                                                                          |
| [prettier](https://github.com/prettier/prettier)             | One formatter covers documentation, data and web files; the XML plugin covers XML. Chosen for format coverage and consistent output, with the cost of a Node.js environment managed by prek. |
| [rustfmt](https://github.com/rust-lang/rustfmt)              | Standard Rust formatter supplied by the pinned toolchain. No additional Rust formatting dependency.                                                                                          |
| [editor-config](https://editorconfig.org/)                   | Applies shared defaults in supporting editors. No additional hook command.                                                                                                                   |

## Text files

- UTF-8, LF, one final newline.
- No trailing whitespace. Markdown permits two spaces for a hard line break.
- Default indentation: four spaces.
- Editor overrides for JSON/JSONC, YAML, XML, SVG, Markdown, JS/TS, Nix and shell:
  tabs with a width of two columns. Formatters determine the final output;
  YAML indentation uses spaces.
- `.bat` and `.cmd` use CRLF.

## Rust

- Rust 2024 edition; standard rustfmt settings.
- Format with `cargo fmt --all`.
- The prek hook checks formatting without applying changes.
- Build checks run Clippy with `-D warnings`.

## C/C++

- LLVM base style; four spaces; line width 120.
- Opening braces stay on the declaration or control statement line.
- Function bodies, loops and `if` bodies use multiple lines.
- Indent `case` labels.
- Attach pointer markers to the type: `Type* value`.
- Sort includes with case-sensitive ordering.
- clang-format handles layout. Native compilation checks semantics.

## Python

- Ruff lint and formatter; Python 3.15 target.
- Four spaces; line width 100; double quotes.
- Format code examples in docstrings.
- Enabled lint families: `E`, `W`, `F`, `I`, including import sorting.
- Automatic fixes run before formatting. Resolve remaining diagnostics manually.
- Python shebang scripts are checked even without a `.py` extension.

## Data, documentation and web files

- TOML: Taplo, line width 100, preserve key order.
- JSON, YAML, XML: syntax validation and Prettier.
- Markdown, JavaScript/TypeScript, HTML and CSS: Prettier.
- Prettier print width: 100; preserve prose wrapping; skip embedded code formatting.
- XML: preserve significant whitespace with `xmlWhitespaceSensitivity=strict`.
- Shell, Nix and Slint: general text checks only; no dedicated lint or format hook.

## Scope

- All hooks exclude `vendor/`, `target/`, `.local/`, `.venv/`, `node_modules/`,
  `Cargo.lock`, `flake.lock`, patches, SVG files and font license files.
- The copied Weston protocol under `testing/protocols/` is validated without reformatting.
- Do not manually format generated or vendored files.

## Commands

Setup: [Contributing — lint and formatting](../CONTRIBUTING.md#lint-and-formatting).

```sh
# Check and fix tracked files.
prek run --all-files

# Check specific files, including new files not yet tracked by Git.
prek run --files path/to/file.py

# Run one hook.
prek run ruff-check --all-files

# Apply Rust formatting.
cargo fmt --all
```

Inspect automatic changes and rerun until all hooks pass. A hook that modifies files
returns a failure for that run. CI runs the same hooks, Rust formatting and doc link checks.
