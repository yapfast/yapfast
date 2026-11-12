# A plugin to start from, in C

C is the smallest thing a plugin can be: 10.3 KiB and **two pages of stack and
data** beyond what it allocates, with no runtime at all. It is not the default —
AssemblyScript is, because it is cheaper still in memory and much kinder to
write — but it is the plainest way to see what the ABI is, and it needs nothing
but `clang`.

## What is here

| File | What it is for |
| --- | --- |
| `yapfast-plugin.json` | The manifest: the plugin's id, version, permissions and the command the composer lists |
| `plugin.c` | The five handlers, all of them stubs but the command; delete the ones you don't need and the PDK's defaults take over |
| `build.sh` / `build.cmd` | The build, for a Unix shell and for Windows: makes `plugin.wasm`, which is what the manifest names |
| `plugin.wasm` | Not here yet: the build makes it |

## Build it

```sh
cd pdk/c/template && sh build.sh    # writes plugin.wasm beside the manifest
```

That is plain `clang` with `--target=wasm32-unknown-unknown -nostdlib`, linking
this plugin with `yapfast.c`, `json.c` and `freestanding.c` from the folder
above. No SDK, no WASI, nothing to install.

On Windows, `build.cmd` is the same build with `clang` from LLVM on `PATH`.

## Try it

Install the folder, then type `/hello` — or `/hello name: Ada` — in a room. It
answers the one line the manifest's command promises.

## Next

* `docs/plugins/interface.md` — every host call and export.
* `docs/plugins/manifest.md` — every field of the manifest.
* `docs/plugins/pdk.md` — what each language costs, and why AssemblyScript is
  the one to reach for first.
* `../examples/shout` — the next step up: a plugin that sends a message.

**Memory is not given back.** A guest's pages stay with its instance until it is
evicted, so answer into a static buffer and keep it small; `plugin.c` does.
