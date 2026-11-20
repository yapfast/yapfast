# A plugin to start from, in Zig

Zig gives you C's control with better errors, and the module stays small:
55.1 KiB, most of which is the language's default 1 MiB stack. It is not the
default — AssemblyScript is, because it asks for one page instead of seventeen —
but it is the nicest of the no-runtime options to write.

## What is here

| File | What it is for |
| --- | --- |
| `yapfast-plugin.json` | The manifest: the plugin's id, version, permissions and the command the composer lists |
| `hello.zig` | The one handler this plugin has, and the five exports |
| `build.sh` | What makes `plugin.wasm`, which is what the manifest names |
| `plugin.wasm` | Not here yet: `build.sh` makes it |

## Build it

```sh
sh build.sh          # writes plugin.wasm beside this file
```

`wasm32-freestanding` is the module Extism runs: no WASI, nothing to preopen.
The `--export=` flags in `build.sh` are **not** optional — a module's export
section is written by wasm-ld, and `export fn` alone produces no exports.

On Windows, `build.cmd` is the same build with `zig` on `PATH`.

## Try it

Install the folder, then type `/hello` — or `/hello name: Ada` — in a room. It
answers the one line the manifest's command promises.

## Next

* `docs/plugins/interface.md` — every host call and export.
* `docs/plugins/manifest.md` — every field of the manifest.
* `docs/plugins/pdk.md` — what each language costs. Zig's seventeen pages are
  its default stack rather than your data, and Zig 0.17 has no flag to lower it.
* `../examples/shout` — the next step up: a plugin that sends a message.

**Memory is not given back.** A guest's pages stay with its instance until it is
evicted, so answer into a static buffer and keep it small; `hello.zig` does.
