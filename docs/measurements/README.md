# Resource measurements

## Vesktop 1.6.7 and Yapfast

| | Vesktop memory | Yapfast memory | Vesktop CPU | Yapfast CPU |
| --- | --- | --- | --- | --- |
| Idle | 724 MB | 30 MB (−96%) | 0.11% | 0.015% (−86%) |
| Image-heavy channel | 706 MB | 41 MB (−94%) | 0.17% | 0.008% (−95%) |
| In a call | 888 MB | 44 MB (−95%) | 0.46% | 0.33% (−28%) |
| Sharing your screen¹ | 1,013 MB | 113 MB (−89%) | 2.15% | 1.1% (−48%) |

Vesktop is basically Discord's own app: the same Discord web app, in the same
Electron, with Vencord on top. So its numbers stand in for Discord's.

¹ Sharing a 1920×1080 monitor at 30 frames a second, while in the call.
Yapfast encodes on the graphics card when it has an encoder, here NVENC
sending H264. Encoding on the processor with x264 instead, it takes 126 MB and
2.5%, a little more CPU than Vesktop.

<details>
<summary>How these were measured</summary>

Measured on 25 October 2026, and screen sharing on 27 October, on the same
PC: an AMD Ryzen 9 9900X (12 cores, 24 threads) with 32 GB of RAM and an NVIDIA
RTX 3090 (driver 580), under NixOS with Hyprland, and each app's default
renderer (the GPU for Vesktop, the CPU for Yapfast). Yapfast is the 0.2.0 Linux
release binary, except while sharing a screen: that's main after 0.2.0, which
encodes on the GPU, or with x264 in place of OpenH264, built for Discord alone
and optimized as the release is.

Memory is what Windows' Task Manager counts: each app's own memory across all
its processes, without the program files and fonts it maps from disk. On
Linux, that's the private anonymous memory of every process in the app, plus
Yapfast's window frames, which Yapfast draws itself. While NVENC encodes, the
NVIDIA driver also maps about 21 MB of shared memory into the stream's process,
which this leaves out, as it leaves out other shared memory. CPU counts the
whole processor as 100%. The percentages compare Yapfast with Vesktop.

Vesktop's window was maximized at 1920×1044. Yapfast's was 1920×1080 on the
test display, and 941×1020 while sharing on the desktop.

- Idle: a DM open, a minute or two after starting.
- Image-heavy channel: scrolled up through about a hundred pictures, then back
  down to the newest.
- In a call: measured from the call's third minute to its fifth, with the
  microphone on and a text channel open. Yapfast's memory held steady from
  the second minute to the sixteenth.
- Sharing your screen: the call, live with one monitor, measured from the
  stream's second minute to its third. Yapfast captures and encodes in a
  second process that runs only while the stream is live; it takes about
  73 MB of the 113 MB. The stream starts in H265, which the test server
  accepts, and switches to H264 a second later, when a viewer who decodes only
  H264 starts watching, as one of Discord's clients would.

Vesktop ran on a real account, which has more servers than Yapfast's, once
for each scenario; its call had one friend talking now and then. Yapfast ran
against the local test server (`testing/fake_discord.ts`), five times for each
scenario, which stayed within 1 MB of each other, and twice for screen
sharing; its call had three people talking without pause. Yapfast shared a
real screen on the Hyprland desktop through the screen sharing portal.

The scripts behind these numbers, with their raw samples, are in the private
`2026-10-25-release-summary` folder beside this file. `suite.sh` ran the idle,
image-heavy and call scenarios through `scripts/ui-session`,
`desktop-share/run.sh` ran screen sharing on the real desktop, and `sample.py`
sampled each app's whole process tree once a second.

</details>

## Targets

| Scenario | Ceiling |
| --- | --- |
| Settled idle after the first sync | Under 250 MiB and under 1% of one core |
| Ten-person audio call | Under 400 MiB |
| Inactive window | No continuous rendering |

These are ceilings; aim lower where the numbers allow. Check them on a
machine of about four cores with 8 GB of RAM and integrated graphics, and on
real Windows.

### Plugins

| Scenario | Ceiling |
| --- | --- |
| Plugins compiled in, none enabled | No helper process and no thread of its own, the parent's `RssAnon` within noise (≤ 100 KiB), idle CPU unchanged, no timers |
| Enabled, none activated yet | As above, plus ≤ 16 KiB a plugin for its registry entry |
| The last plugin disabled or evicted | Helper exited; the parent back within 256 KiB of its earlier level |
| The helper alive with one small plugin | ≤ 6 MiB `RssAnon` |
| Ten activated plugins with 2 MiB modules | ≤ 16 MiB `RssAnon` before their own heaps |
| An event to a subscribed plugin, 1 KiB | ≤ 50 µs p99 of overhead, pipe included |
| An event nobody subscribed to | One relaxed atomic load |
| Startup with plugins installed, none needing `startup` | ≤ 1 ms added on the UI thread |
| The UI thread | Never runs WebAssembly, never waits on the helper |

A ceiling here is what the design is meant to reach, not a reading: the helper
stays inside one budget by admission control over its instances, and each
instance inside its own. How the rules that follow from these numbers are kept
is in [plugins](../plugins/README.md#keeping-it-small), and the budgets the
helper enforces itself — 256 MiB of summed instance limits, 256 pages an
instance by default — in [the helper](../plugins/host.md#eviction-and-the-memory-budget).

## How to measure

Measure a `measure` build of the release features, not `fast`, which leaves
the Slint interface unoptimized. Use `release` only when the question is the
shipped build's absolute number.

```sh
nix develop path:./nix --command cargo build --locked --profile measure --no-default-features --features desktop,discord --bin yapfast
```

`scripts/ui-session start --discord --profile measure` builds the same and
runs it against `testing/fake_discord.ts` on a private display; `--bin PATH`
runs a build saved outside `target` instead, so two builds can alternate.

Report memory as the table above does: what Windows' Task Manager shows, the
private working set, and on Linux the app's `RssAnon`, plus `VmSwap` when the
host swaps. RSS and PSS also count the mapped code and fonts, and they fall
whenever the kernel reclaims those pages, so they mislead on a busy host.

To compare two builds, use glibc's heap in use, which reclaim doesn't move:
append `stats` to `.local/ui-session/window-control`, and Yapfast writes
glibc's `malloc_stats` to `.local/ui-session/yapfast.log`.

`scripts/measure-process PID --seconds N` samples one process once a second
and prints the mean and peak RSS, mean PSS and CPU, with one core as 100%; it
doesn't read `RssAnon`. For a process's lifetime peak, read `VmHWM` from
`/proc/PID/status`. `testing/measure_tray.py` measures anonymous memory plus
swap and heap in use with #general open, closed to the tray and in a call;
it takes `--bin` for a saved build. The other `testing/measure_*.py` harnesses
and `testing/join_voice.py` cover cameras, scrolling, the network builds and
joining a call.

Alternate the builds over several runs and give the spread between runs of
one build. Keep the hardware, workload, warm-up, window state and memory
metric with every result. Dated records of past measurements are in the
private `records.md` beside this file.

### The plugin helper

`testing/measure_plugins.py` measures `yapfast-plugins`, the plugin helper,
alone: it spawns the helper and speaks its framed protocol, with no Yapfast
process, Slint or display in the way. Build the helper and the fixture plugin
first; [testing the plugin system](../plugins/testing.md) has the commands and
the arguments.

Its memory figure is the helper process's own, not the app's, and each OS names
it differently: Linux reads `RssAnon` from `/proc/<pid>/status`, and Windows
`GetProcessMemoryInfo`'s private usage. Both count memory that process alone
owns. `RssAnon` counts resident pages only, while Windows' private usage also
counts private pages the system has paged out, so a Windows record reads a
little above the private working set Task Manager shows. Compare plugin records
only with records from the same OS.

### A hook's JSON

The plugin ABI is JSON ([the ABI](../plugins/abi.md)), and MessagePack is worth
adding only if JSON is over a fifth of a hook's time. It isn't:

| | Per hook | JSON's share |
| --- | --- | --- |
| `message.pre_send` round trip | 68–131 µs | 1.6–3.9% |
| The JSON in it | 2.0–2.8 µs | |

Measured on 11 November 2026 on the same PC, with a `fast` build of the helper
and `testing/plugins/echo.wasm`, whose `yf_hook` answers `"unchanged"`: 200
sequential hook exchanges, each waited for, from the frame leaving the parent to
`HookDone` arriving; and 200 encode-and-decode pairs of the same two messages,
a `ToHelper::Hook` and a `FromHelper::HookDone`, in the helper's own test
process. The round trip includes the pipe, the scheduler, Extism and the guest's
call, so the JSON figure — both sides' serialization added together — is what
the wire format costs of it. The spread is the machine's, not the measurement's:
the three runs gave 131 µs, 73 µs and 68 µs per exchange with the JSON steady at
2–3 µs, and not one of them came near the fifth that would justify a second
encoding.

```sh
cargo test --profile fast -p yapfast-plugin-host --test e2e the_json_in_a_hook \
  -- --nocapture
```

## Go as a plugin language

**The question.** Should Yapfast ship a Go PDK, and if so built with the
standard Go toolchain (`GOOS=wasip1 GOARCH=wasm`) or with TinyGo? A collected
language carries its runtime inside the module, and every guest page is the
helper's memory for as long as the instance lives, so the answer is a memory
number: `limits::DEFAULT_MEMORY_PAGES` gives each instance 256 pages (16 MiB)
unless its manifest says otherwise, `limits::MAX_HELPER_BYTES` is 256 MiB for
all of them together, and `limits::MAX_WASM_BYTES` refuses a module over 8 MiB.
A PDK that costs most of the default before the plugin has done anything is not
a PDK this design can afford; one that costs a few hundred KiB is.

Measured on 12 November 2026, on an Intel Core i7-14700K (28 threads) with
31 GiB of RAM, which is not the machine the older records in this file were
taken on: compare these numbers with each other, not with those.

### What a module asks for at load

The memory section is the request a module makes before a line of its code
runs, and it is exactly what the page limit governs. The ABI-shaped Go module
here is 74 lines, most of them comments: five exports (`yf_activate`,
`yf_deactivate`, `yf_event`,
`yf_command`, `yf_hook`, each `func() int32`) and five imports from
`extism:host/env` (`input_length`, `input_load_u8`, `alloc`, `store_u8`,
`output_set`), which is the whole of what the Extism guest ABI needs to read an
input and set an answer.

| Module | File | Memory: initial | max | Data segments | Exports |
| --- | --- | --- | --- | --- | --- |
| Rust PDK, `plugins/ping/plugin.wasm` (committed) | 160.8 KiB | 17 pages, 1,088 KiB | none | 1, 19.2 KiB | 5 + `memory` |
| Rust PDK, `shout` example (local artifact, not committed) | 352.3 KiB | 17 pages, 1,088 KiB | none | 1, 43.7 KiB | 5 + `memory` |
| Go std, ABI shape | **1,833.1 KiB** | **32 pages, 2,048 KiB** | none | 21,261, 522.6 KiB | 5 + `_initialize`, `memory` |
| Go std, ABI shape + `runtime/debug` (`SetGCPercent`, `SetMemoryLimit`) | 2,005.1 KiB | 34 pages, 2,176 KiB | none | 24,262, 584.3 KiB | 5 + `_initialize`, `memory` |
| Go std, ABI shape + a 1 MiB slice and a 10,000-entry map | 1,835.3 KiB | 32 pages, 2,048 KiB | none | 21,273, 523.3 KiB | 5 + `_initialize`, `memory` |
| Go std, the five exports only, no imports at all | 1,829.8 KiB | 32 pages, 2,048 KiB | none | 21,139, 521.8 KiB | 5 + `_initialize`, `memory` |
| Go std, `func main() {}` and nothing else | 1,825.6 KiB | 32 pages, 2,048 KiB | none | 20,926, 520.2 KiB | `_initialize`, `memory` |
| Go std, ABI shape built without `-ldflags="-s -w"` | 1,867.6 KiB | 32 pages, 2,048 KiB | none | 21,261, 522.6 KiB | unchanged |
| `testing/plugins/echo.wasm`, `filter.wasm`, `spin_hook.wasm` | 0.2–1.1 KiB | no memory section | — | 0 | 5–6 |

Three things in that table decide the question.

**The module is the runtime, and stripping it buys almost nothing.**
`-ldflags="-s -w"` takes 34.5 KiB off, from 1,867.6 KiB to 1,833.1 KiB, because
what is in the file is 1,156 KiB of the Go runtime's own machine code and 21,000
data segments, not debug info. 1,833.1 KiB is 11.4× the Rust PDK's module and 32× the Zig
example's 57 KiB, and it is inside `MAX_WASM_BYTES` (8 MiB) with room to spare,
so nothing refuses it.

**The load-time request is 2 MiB and cannot be turned down.** It is not a
function of the program: `func main() {}`, the five exports with no imports, and
the whole ABI shape all ask for the same 32 pages, and the only thing that moved
it was static data (34 pages once `runtime/debug` was imported). The linker
hardcodes both halves of it, in `cmd/link/internal/wasm/asm.go`:

```go
dataEnd := uint64(ldr.SymValue(ldr.Lookup("runtime.end", 0)))
var initialSize = dataEnd + 1<<20 // 1 MB, for runtime init allocating a few pages
...
ctxt.Out.WriteByte(0x00)          // no maximum memory size
```

So every Go module asks for its static data plus a 1 MiB reserve, and declares
no maximum at all; the page cap is the only ceiling on it. `MIN_MEMORY_PAGES`
is 32, so a manifest may legally ask for exactly what an idle Go plugin needs
and nothing more — the floor, with no room for the event it is there to handle.

**The Go runtime is bigger than the module.** The 2 MiB is a reservation that
is only paid for as it is touched; 522 KiB of data segments are touched at
instantiation. What that grows to in use is the next table.

### What it costs the helper

`yapfast-plugins`'s own `RssAnon`, with a `fast` build of the helper (the
`measure` build on this disk can't start: its Nix interpreter is gone), driven
through `testing/measure_plugins.py`'s protocol with `wasi` set true for the Go
modules. Four plugin ids, three runs, 256 pages each.

| | Helper at rest | One instance, cold compile | One instance, warm compile cache | Each further instance |
| --- | --- | --- | --- | --- |
| Rust PDK, `ping` | 1.20 MiB | 37.5–39.3 MiB | 3.51 MiB | 1.18–1.37 MiB |
| Go std, ABI shape | 1.20 MiB | 112.7–118.3 MiB | **9.63 MiB** | **6.42–9.12 MiB** |

The cold column is wasmtime's JIT output in anonymous memory, and the ratio
between the two compilations is the first sign of what a Go plugin costs to
load: 34 MiB of JIT for `ping`'s 140 KiB of code, and 103–109 MiB for the Go
module's 1,156 KiB. A warm on-disk artifact cache serves that from a mapped
file instead, and the same module then costs 3.51 MiB of anonymous memory
rather than 37–39, or 9.63 rather than 112–118.

The last column is the number that decides a budget: **8.2 MiB of `RssAnon`
per live Go instance against 1.3 MiB for Rust**, 6.5× on the means and 4.7–7.7×
across the runs. Sixteen of them is 131 MiB of the helper's 256 MiB, and their
declared caps alone sum to the whole 256 MiB.

`load` and `activate` together, from the frame out to `Activated`: 0.29 s for
the Go module cold and 0.02 s warm, against 0.19 s and 0.00 s for `ping`. The
2 s activation budget is met either way, but a cold compile is longer than the
25 ms a hook gets and longer than the 100 ms the whole chain gets, so a hook
chain that has to load a Go plugin is answered unchanged that time.

### A session's worth of events: 10,000 of 1 KiB

What matters is not what an idle instance reserves but what it has grown to
after a session's worth of events, because WebAssembly memory never shrinks.
Each event here is a `message_create` whose payload is 1 KiB of text.

| Go std, 1 KiB events | Instance at rest | Growth by 10,000 events | Peak total RSS |
| --- | --- | --- | --- |
| ABI shape, input read per event, 128 pages, warm cache | 9.63 MiB | +4.9 MiB (peak 14.54 at 6,000) | 25.7 MiB |
| ABI shape + 1 MiB slice and a 10,000-entry map, built once, 256 pages, cold | 112.6 MiB | +8.4 MiB | 134.6 MiB |
| The five exports only, no imports, 64 pages, warm cache | 9.61 MiB | +0.1 MiB | 22.9 MiB |

At the **64-page (4 MiB)** limit a garbage-collected PDK would have to hold
itself to, the default runtime does not get through the load, and it says so
itself rather than trapping somewhere unexplained:

| Go std, 1 KiB events, 64 pages | Result |
| --- | --- |
| ABI shape, input read per event, default `GOGC` | `out_of_memory` on event 2,891 |
| ABI shape + 1 MiB slice and map, default `GOGC` | `out_of_memory` on event 1,379 |
| ABI shape, `SetGCPercent(10)` + `SetMemoryLimit(3 MiB)` in `yf_activate` | ran all 10,000, 11.41 → 13.16 MiB |
| ABI shape + slice and map, the same two calls | ran all 10,000, 11.41 → 14.38 MiB |
| The five exports only, no imports (nothing allocated per event) | ran all 10,000, 9.61 → 9.68 MiB |

The two rows that matter are the first and the last: a plugin that allocates
1 KiB per event runs out of 4 MiB by event 2,891, and one that allocates nothing
sits at 9.68 MiB the whole way. So the runtime's knobs do work under `wasip1` —
choosing them by hand takes the default from "doesn't fit half the default page
limit" to "fits 4 MiB with 1 MiB of data besides" — but what is being fixed is
Go's own garbage, not a plugin's data, and the fix is two calls in `yf_activate`
that nothing in the ABI asks for.

<details>
<summary>How these were measured</summary>

Go 1.27.3 at `/usr/bin/go`, no network, nothing installed: `go build` with
`GOOS=wasip1 GOARCH=wasm` works here, wasmtime, wasmer, wasm-tools and the
`wasm-objdump` CLI are not, so every wasm number in this section was read out
of the binary by a short Python script in `/tmp`.

The module, in `/tmp/gopdk/abi`, is the ABI shape the tables name:

```sh
cd /tmp/gopdk/abi && printf 'module example.com/plugin\n\ngo 1.27\n' > go.mod
GOOS=wasip1 GOARCH=wasm go build -trimpath -buildmode=c-shared \
  -ldflags="-s -w" -o /tmp/gopdk/abi.wasm .
```

`-buildmode=c-shared` is what makes a reactor module — `_initialize` instead of
`_start`, which Extism calls, and which is what starts the Go runtime. The
exports take no arguments and answer an `int32`: Extism calls an export with no
arguments and reads the answer out of the guest's memory with `output_set`, and
it takes the call's status from a single `i32` result, defaulting to `-1` when
the result is not one — so an `i64` result and a first parameter both make
every call fail.

Sections are read without a wasm tool, by walking the binary: section 5 is the
memory section, and its first entry is flags, minimum pages, and the maximum
when flag bit 0 is set.

```python
b = open(sys.argv[1], 'rb').read(); i = 8
while i < len(b):
    sid = b[i]; i += 1
    size, i = uleb(b, i); end = i + size
    if sid == 5:                       # memory
        n, j = uleb(b, i)
        for _ in range(n):
            flags, j = uleb(b, j); mn, j = uleb(b, j)
            mx = None
            if flags & 1: mx, j = uleb(b, j)
            print('initial', mn, 'pages', mn * 65536, 'bytes, max', mx)
    i = end
```

For the runtime half, `testing/measure_plugins.py` drives the helper over its
own pipe, with one change in a copy in `/tmp`: the `load` frame's `wasi` field
set true, since a Go build imports `wasi_snapshot_preview1` for clocks and
randomness and the helper refuses that for a manifest which doesn't ask:

```sh
WASI_OK=1 testing/measure_plugins.py --helper /tmp/helper-shim \
  --plugin /tmp/gopdk/abi.wasm --pages 256 --plugins 4 --runs 3
```

`/tmp/helper-shim` is three lines: this disk has no `/nix/store`, so the built
`target/fast/yapfast-plugins` can only start under the host's loader,
`/lib64/ld-linux-x86-64.so.2`. It is a `fast` build, so read its numbers as the
helper's shape rather than the shipped one.

The event numbers come from `/tmp/event_probe.py`, which speaks the same frames:
`hello`, `load` with `wasi`, `activate`, then N `Event` requests of one 1 KiB
`message_create`, sampling `RssAnon` after the first, tenth, hundredth,
thousandth and last event. Nothing answers an event frame, so a `deactivate`
sent after the batch is the barrier — it is queued behind them on the plugin's
mailbox — and the sample after its `Deactivated` is the last one. The faults in
the 64-page table are the helper's own `out_of_memory`, which is `memory.grow`
being refused at the page limit.

The host was swapping while the long runs were taken: `/proc/PID/status` showed
`VmSwap` rising to 9.20 MiB by event 10,000 in one run, and `RssAnon` falling
with it, since a swapped page is not a resident one. That is why the event
table gives growth and a peak rather than a final figure — a single sample
taken at the end of a long run can read below the instance's own rest — and why
the per-instance numbers, which come from runs of seconds rather than a minute,
are the ones to weigh. The page-limit faults are unaffected: `memory.grow`
counts pages, resident or not.

</details>

### Not measured

- **TinyGo, all of it.** It is not installed here and nothing may be. Every
  number above is standard Go; none of them stands in for TinyGo, in either
  direction. TinyGo is the interesting comparison precisely because it has no
  GC metadata, no reflection tables and no goroutine scheduler unless asked, so
  it should be several times smaller — and its `-gc=` choice is what decides by
  how much.
- **The C, Zig and AssemblyScript PDK outputs.** `docs/plugins/pdk.md` records
  the examples as 11.9 KiB, 17.7 KiB and 57 KiB, but no `.wasm` for them is
  committed and their toolchains weren't run here, so the only non-Go module
  measured in this section is the Rust one.
- **A Rust plugin under the same event load.** `ping` subscribes to no events,
  and building a Rust fixture needs `cargo`, which was out of bounds while this
  was taken. The Rust column is an idle instance only; at 10,000 events the
  comparison is Go against itself.
- **`GOGC` and `GOMEMLIMIT` as environment variables.** A guest's environment
  comes from Extism's WASI configuration, not from the helper's process, so a
  plugin can't be handed either one from outside. Only `debug.SetGCPercent` and
  `debug.SetMemoryLimit` inside the plugin are measured, and by their effect
  (the last four rows of the 64-page table) rather than by reading the pacer's
  state back.
- **Windows and macOS.** These are Linux `RssAnon` numbers from one machine.
- **What an event costs in time.** This is a memory measurement; the hook round
  trip is in [A hook's JSON](#a-hooks-json).

### The experiment for someone with TinyGo

The same three numbers decide it — module bytes, the memory section at load,
and resident memory per instance after 10,000 events of 1 KiB — and each flag
moves one of them. Build with `-target=wasi` and `-buildmode=c-shared`, and
record the same tables.

| Flag | What it trades | What to record |
| --- | --- | --- |
| `-gc=conservative` (the default on most targets) | No allocation metadata, so free memory is found by scanning: the smallest code, and the heap it holds longest | The baseline: module bytes, initial pages, resident per instance, and the event at which 64 pages first refuses `memory.grow` |
| `-gc=leaking` | Frees nothing: the smallest and fastest runtime, and every byte a plugin allocates is kept until eviction | The slope: growth per 10,000 events, and whether 256 pages holds for a session |
| `-gc=precise` | Frees reliably, for a little more code and slower allocation | The steady state as `conservative`: the difference is what it holds after 10,000 events, not what it reserves |
| `-scheduler=none` | No goroutines at all: smallest, and no `select` or `time.Sleep` in a plugin | Module bytes and initial pages; this is the floor |
| `-scheduler=asyncify` | Goroutines by transform: the code and the stacks both grow | How much bigger, and whether it still fits 64 pages |
| `-scheduler=cores` | Real threads, needing shared memory and atomics, which this host's single-call-at-a-time design does not offer | Whether the module links at all; not expected to be shippable |
| `-opt=z` against `-opt=s` | Size against speed | Module bytes, and the hook round trip from [A hook's JSON](#a-hooks-json) |
| `-no-debug` | Drops DWARF: a smaller file, worse panics | Module bytes — expect most of the saving standard Go did not get from `-s -w` |
| `-panic=trap` | A trap instead of a message on panic | Module bytes only; the helper reports a trap as a fault either way |
| `-ldflags="-s -w"` | Strips symbols and DWARF | Module bytes |
| `-wasm-abi=generic` | The i64 ABI for the `wasip1` target; `js` is for the browser and is not what a plugin wants | That it is the default, and that a PDK built with `js` fails to link |

The std-Go knobs to carry into the comparison, since a TinyGo PDK will be
judged against the standard one: `debug.SetGCPercent` and
`debug.SetMemoryLimit` work under `wasip1` (measured above — they are what gets
a default Go plugin through 10,000 events at 64 pages), `GOGC` and `GOMEMLIMIT`
can't be set from outside the guest, and the runtime's initial reserve is a
linker constant, `dataEnd + 1 MiB` with no maximum declared, that no flag
reaches. TinyGo's answer to that last one is its own: record its memory section
first, because that is what a 256-page budget sees before anything runs.

### The decision

A Go PDK is worth shipping only if a plugin written against it meets all three
of these, measured the way this section measured them:

| Bar | Ceiling | Where it comes from |
| --- | --- | --- |
| Module size | **512 KiB** | 3.2× the Rust PDK's 160.8 KiB and 9× the Zig example's 57 KiB; a module is paid for at install, in the store on disk, and again in every cold compile |
| Linear memory in steady state | **4 MiB (64 pages)** | Half of `DEFAULT_MEMORY_PAGES`, leaving 12 MiB of the default for a plugin's own data, a 64 KiB payload, a 256 KiB hand-off and the input |
| Resident per live instance | **4 MiB above the helper's rest** | [Targets](#plugins) allows the helper 6 MiB `RssAnon` with one small plugin alive, and its rest is 1.2 MiB; the row under it allows ten plugins 16 MiB, which is 1.6 MiB each |

By those bars, **the standard Go toolchain does not qualify and no Go PDK
should be built on it.** 1,833.1 KiB is 3.6× the size bar; 8.2 MiB resident per
instance is 2× the one-plugin ceiling and 5× the ten-plugin one; and against the
64-page bar its default runtime runs out of memory on event 2,891 of a workload
that only reads its input. It is not a marginal miss on one axis, and the two
calls that fix the third are each a plugin author's private decision —
unenforced by any PDK, invisible in the manifest, and undone by the next author
who doesn't know about them.

So: **no Go PDK for now; Rust stays the only official one.** The ABI
description stays as it is, so a Go author can still build against Extism's Go
PDK directly if their plugin fits, and these three bars give them a way to
check before they ship it. TinyGo reopens the question rather than answering
it: it is unmeasured here, and the experiment above is what would settle it. If
it comes in under 512 KiB, under 4 MiB of steady state and under 4 MiB resident
per instance, the PDK is worth writing; if it lands between those numbers and
the standard toolchain's, the answer is the same as today's.

For someone writing a plugin rather than maintaining this one, the same verdict
and bars are in [writing a plugin](../plugins/pdk.md#go); the numbers stay
here.
