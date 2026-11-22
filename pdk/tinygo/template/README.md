# A plugin to start from, in Go, with TinyGo

Go, built with TinyGo rather than the standard toolchain: 10.2 KiB and **two
pages of linear memory**, against the standard toolchain's 1,833.1 KiB and 32
pages, which the repository measured and refused. It is not the default —
AssemblyScript is, because it asks for one page instead of two — but it is the
one to reach for when the plugin wants Go's library and Go's comforts, and it is
second in [the measurements' order](../../../docs/plugins/pdk.md).

## What is here

| File | What it is for |
| --- | --- |
| `yapfast-plugin.json` | The manifest: the plugin's id, version, permissions and the command the composer lists. `"wasi": true` is not optional — see below |
| `plugin.go` | The five exports Yapfast calls, a handler for each, and the little JSON that reads one option |
| `go.mod` / `go.sum` | The module, pinning the Extism Go PDK (`github.com/extism/go-pdk` v1.1.3). Nothing is vendored |
| `build.sh` / `build.cmd` | The build, for a Unix shell and for Windows: makes `plugin.wasm`, which is what the manifest names |
| `plugin.wasm` | Not here yet: the build makes it |

The PDK is Extism's own and unmodified: `pdk.Input`, `pdk.Output*` and
`pdk.SetError*` are the guest's half of the ABI, and the JSON in and out is the
helper's.

## Build it

```sh
cd pdk/tinygo/template && sh build.sh   # writes plugin.wasm beside the manifest
```

That is TinyGo 0.43 (`nix shell nixpkgs#tinygo`), and `go mod download` needs
the network once for the PDK. `build.sh` holds the build's one command:

```sh
tinygo build -target=wasip1 -buildmode=c-shared \
    -opt=z -no-debug -gc=leaking -scheduler=none -panic=trap -o plugin.wasm .
```

On Windows, `build.cmd` is the same build with `tinygo` on `PATH`.

`-target=wasip1` is what the Extism Go PDK is built for, and the module imports
four WASI functions — `clock_time_get`, `random_get`, `args_get`,
`args_sizes_get` — so the manifest says `"wasi": true`. Without it the helper
refuses the module at load instead of letting it trap on its first clock read.
`-buildmode=c-shared` is what makes this a **reactor**: the module exports
`_initialize` and no `_start`, so the helper instantiates it, runs
`_initialize`, and then calls the five exports. A module with only `_start`
would run once and be gone.

Fifteen exports, measured, and only five of them are the ABI's: `yf_activate`,
`yf_deactivate`, `yf_event`, `yf_hook` and `yf_command`, beside `_initialize`,
`memory`, the cgo allocator's `malloc`, `free`, `calloc` and `realloc`, and four
float helpers TinyGo's runtime links (`fminimum`, `fminimumf`, `fmaximum`,
`fmaximumf`). The helper looks for the five and ignores the rest.

### What the flags are worth

Measured on this template with `/tmp/tinygo-measure/measure.py` for the file and
the memory request, and a host giving Extism's seven imports and Node's WASI
for the pages in use, read after one instance ran `yf_activate`, five commands,
an event batch, a hook and a deactivation:

| Flags | Module | At load | In use | Verdict |
| --- | --- | --- | --- | --- |
| the default build (`-gc=precise`, `-scheduler=asyncify`) | 529.4 KiB | 2 pages | 5 pages | fits, and is mostly DWARF |
| `-scheduler=none -no-debug` | 16.7 KiB | 2 pages | 3 pages | |
| **this template**: `-opt=z -gc=leaking -scheduler=none -no-debug -panic=trap` | **10.2 KiB** | **2 pages, 128 KiB** | **3 pages, 192 KiB** | the row to copy |
| the same, `-gc=conservative` instead of `-gc=leaking` | 15.0 KiB | 2 pages | 3 pages | for a plugin that handles many events |
| `-opt=z -gc=leaking -scheduler=none -panic=trap`, DWARF kept | 466.3 KiB | 2 pages | 3 pages | why `-no-debug` is there |

`-gc=leaking` and `-scheduler=none` are the two that need a decision rather than
a copy. **`-scheduler=none`** means no goroutines: `go func`, a channel and
anything that waits on one are out, and `-scheduler=asyncify`, TinyGo's default,
is what leaves the default build at five pages. **`-gc=leaking`** means no
collector: nothing a plugin allocates is ever freed, so it stays allocated until
the instance is evicted, and a plugin that handles a long stream of events should
build with `-gc=conservative` instead — 15.0 KiB, two pages at load.
**`-panic=trap`** makes a panic a trap, which the helper reports as a fault and
discards the instance for; a reactor has nowhere to print and exit to, and the
build is one WASI import shorter for it (`fd_write`). `-no-debug` is not a trade
at all here: 456 KiB of the build without it is DWARF.

## What is not used, and why

`plugin.go` reads its one option with twenty lines of Go instead of
`encoding/json`. That is not taste: `encoding/json` is TinyGo's reflection, and
the same plugin decoding its invocation into a struct is **409.0 KiB and three
pages**, forty times the module this one is. The name is spliced into the answer
exactly as it arrived and still escaped, because an option arrives as a JSON
string and leaves as one, so its escapes are already what the frame needs. A
plugin that genuinely needs JSON in a payload — an event's, say — should weigh
that 409.0 KiB the way
[the measurements](../../../docs/measurements/README.md) weigh everything else.

## Try it

Install the folder (Plugins → Add → from a folder, or copy it into the store's
folder while developing), then type `/hello` — or `/hello name: Ada` — in a
room. It answers the one line the manifest's command promises.

## To take it out of the repository

Copy the whole folder. `go.mod`'s module path is a placeholder
(`example.com/yapfast-hello`) and is worth renaming; nothing in Yapfast reads it,
and the only thing that has to resolve is `github.com/extism/go-pdk`.

## Next

* `docs/plugins/interface.md` — every host call and export.
* `docs/plugins/manifest.md` — every field of the manifest.
* `docs/plugins/pdk.md` — what each language costs, and why AssemblyScript is
  first and TinyGo second.
* `pdk/assemblyscript/examples/shout` — the next step up: a plugin that sends
  a message. There is no Go example yet; this template is the whole of the Go
  PDK's Yapfast side.

**Memory is not given back.** A guest's pages stay with its instance until it is
evicted, so handle one event at a time and let values go when the batch ends;
with `-gc=leaking` there is no collector to do it for you.
