# Measuring the plugin helper

`testing/measure_plugins.py` measures the separate plugin helper process
(`yapfast-plugins`) without Yapfast, Slint or a display. It spawns the helper,
speaks the framed JSON protocol in [`docs/plugin-protocol.md`](plugin-protocol.md),
and writes one JSON record. It has no budgets: a record contains observations,
and the plan's budgets are applied to those observations elsewhere.

## What it measures

| Field | Meaning |
| --- | --- |
| `latency.spawn_to_ready_ns` | `time.monotonic_ns` from immediately before spawn to the unsolicited `ready` frame at id 0. |
| `latency.spawn_to_first_call_ns` | `time.monotonic_ns` from immediately before spawn through `ready`, every `load`, and the first successful `reply` to a `call`. Module load is inside this span. |
| `latency.load_ns` | One sample per loaded instance: encode and write a `load`, flush, then read the `loaded` with that id. `by_plugin` maps each plugin id to its own duration, in load order; `completed_ns` is the monotonic instant after the last `loaded`. |
| `latency.round_trip_ns` | `time.monotonic_ns` around one `call` frame: encode and write, flush, then read the `reply` with the same id. Warmup calls are not counted. |
| `memory.samples` | The helper's memory sampled while every instance is loaded: once immediately after the last `loaded`, then at `--interval-ms` until `--samples` are taken or a 2 s grace expires. Each sample also carries the platform's thread count. |
| `memory.first_sample_ns` | The monotonic timestamp of the first memory sample; never earlier than `latency.load_ns.completed_ns`. |
| `memory.peak` | The largest value of each sampled field. |
| `memory.threads` | The thread counts seen beside the memory samples, with the source, the peak and any sampling errors. |

`spawn_to_first_call_ns` samples the first call the run makes. With the default
`--warmup 20` that is the first warmup call, not the first measured one; it is
still the first guest call after the loads, and it contains Python's frame
writing, so compare it across runs rather than reading it as the guest's cost.

The frame codec is tested against the protocol's layout: a 13-byte header with
a little-endian `u32` payload length, the kind byte and a little-endian `u64`
id, followed by the payload. A whole frame is at most 1 MiB, so a payload is at
most 1,048,563 bytes. Reads are reassembled from short pipe reads; end of
stream between frames is clean, and end of stream inside a frame is an error.
The helper's stderr is captured (up to 8 KiB) and never mixed into stdout.

Request ids are the parent's: they start at 1 and increase through every load,
warmup call, measured call, unload and the final `shutdown`. Id 0 is accepted
only on a `ready`; a response at id 0, or a `ready` away from id 0, is a
protocol violation. A call's and a reply's payload bytes are kept as they
arrived; the harness decodes a copy to check the envelope and never rewrites
the bytes it reports or forwards.

## Usage

The helper must already be built; this harness does not run cargo. The host's
fixture example writes the WebAssembly the commands below pass as `--module`,
including `target/plugin-fixtures/echo.wasm`:

```sh
cargo run --profile fast -p yapfast-plugin-host --example build_fixtures
```

That example is the source of `target/plugin-fixtures/echo.wasm`; without it,
pass the absolute path of any module that exports the method you name,
including a previously built `echo.wasm`.

```sh
python3 testing/measure_plugins.py \
    --helper target/measure/yapfast-plugins \
    --module "$PWD/target/plugin-fixtures/echo.wasm" \
    --method echo \
    --instances 1 \
    --requests 200 --warmup 20 --payload-bytes 1024 \
    --samples 10 --interval-ms 50 \
    --profile measure --run-label warm-1 \
    --json-out target/plugin-measurements/warm-1.json
```

`--module` must be an absolute path, matching the protocol's `load`
requirement. On Windows that means a drive-letter path (`C:/...`); an MSYS
`/c/...` path from Git Bash is not absolute to Python. Without `--module` the
harness measures spawn-to-ready and idle memory and threads only;
`latency.spawn_to_first_call_ns`, `latency.load_ns` and
`latency.round_trip_ns` have empty `samples` and `null` percentiles, and
`scenario.instances` is 0.

`--instances N` loads the same module N times under distinct plugin ids, so
one helper process holds N Extism instances. One instance keeps `--plugin`
verbatim (`measure` by default), so a single-instance run stays comparable
with earlier records. More than one uses `<plugin>-0` through
`<plugin>-(N-1)`. Calls round-robin over the instances, memory is sampled only
after every load has answered, and every instance is unloaded; the helper
exits after the last `unloaded` as the protocol allows. For example:

```sh
python3 testing/measure_plugins.py \
    --helper target/measure/yapfast-plugins \
    --module "$PWD/target/plugin-fixtures/echo.wasm" \
    --method echo --instances 4 \
    --requests 200 --warmup 20 \
    --profile measure --run-label four-instances \
    --json-out target/plugin-measurements/four-instances.json
```

That record's `scenario.plugin_ids` is `["measure-0", "measure-1",
"measure-2", "measure-3"]` and `latency.load_ns.by_plugin` has one entry per
id.

The helper's own flags go through `--helper-arg`, one argument at a time:

```sh
python3 testing/measure_plugins.py \
    --helper target/measure/yapfast-plugins \
    --helper-arg=--cache-dir --helper-arg="$PWD/target/plugin-cache/cold-1" \
    --module "$PWD/target/plugin-fixtures/echo.wasm" \
    --json-out target/plugin-measurements/cold-1.json
```

Warm and cold runs are the caller's decision: reuse or recreate the cache
directory, and put that state in `--run-label`. The harness only reports what
the process did. Run several times and report the spread between runs, as
`docs/measurements/README.md` requires.

Useful arguments:

| Argument | Default | Purpose |
| --- | --- | --- |
| `--helper PATH` | required | The helper binary to spawn. |
| `--helper-arg ARG` | none | Repeatable helper argument. |
| `--module PATH` | none | Absolute `.wasm` to `load` before calling. |
| `--method NAME` | `echo` | The method each `call` names. |
| `--plugin ID` | `measure` | The plugin id; with `--instances 1` it is used verbatim. |
| `--instances N` | 1 | Load the module under N distinct ids; 1 keeps `--plugin`, more use `<plugin>-0` .. `<plugin>-(N-1)`. |
| `--requests N` | 200 | Counted round trips. |
| `--warmup N` | 20 | Uncounted round trips before them; the first of these is the first call timed by `spawn_to_first_call_ns`. |
| `--payload-bytes N` | 1024 | Bytes in the call payload's `data` string. |
| `--samples N`, `--interval-ms N` | 10, 50 | Requested memory samples and their spacing; `memory.sample_count` reports what was actually read. |
| `--timeout SECONDS` | 10 | Limit for any one frame. |
| `--profile NAME` | none | Build profile label in `build.profile`. |
| `--os-tag NAME` | none | Host label, such as `wsl2`; a WSL2 run is not a bare-metal verdict. |
| `--json-out PATH` | stdout | Where to write the JSON record. |

Exit status is `0` when a measurement was written, `3` when a prerequisite
(helper or module) is missing, `2` for a usage error, and `1` when the protocol
or a timeout failed. A failed run writes `error.kind` and `error.message` and
omits `latency` and `memory`; it never fills them with zeros.

## The JSON record

`schema` is `yapfast-plugin-measurement/2`. The labels needed to compare or
reproduce a run are at the top:

| Key | Contents |
| --- | --- |
| `platform` | `os` (`win32`, `linux`), `os_version`, `machine`, `python`, and the `--os-tag` label. |
| `metric` | The per-OS metric's `name`, `unit`, `source`, and a `comparison` note. |
| `build` | The helper path, its SHA-256 and size, the exact `argv`, and the profile label. |
| `scenario` | Module, method, base plugin id, `instances`, `plugin_ids`, request and warmup counts, payload size, sample count and interval. |
| `latency` | `clock` is `time.monotonic_ns`; `spawn_to_ready_ns`, `spawn_to_first_call_ns`, `load_ns` and `round_trip_ns`. Each has `samples` and nearest-rank `percentiles`; `load_ns` also has `by_plugin` and `completed_ns`. |
| `memory` | Samples with `t_ns` plus the platform's fields and `threads`, the count, `first_sample_ns`, the per-field `peak`, sampling errors, and the `threads` summary. |
| `helper_exit` | `exit_code`, whether stdout reached a clean EOF, and the captured stderr tail. |

## Memory metrics

| OS | Field gated elsewhere | Also sampled | Source |
| --- | --- | --- | --- |
| Linux | `rss_anon_kib` | `vm_swap_kib`, `vm_hwm_kib`, `threads` | `/proc/<pid>/status` |
| Windows | `private_usage_bytes` | `working_set_bytes`, `peak_working_set_bytes`, `threads` | `GetProcessMemoryInfo` with `PROCESS_MEMORY_COUNTERS_EX`, and a Toolhelp32 thread snapshot, through `ctypes` |

Windows private usage includes non-resident private pages, so it reads a
little higher than the private working set Task Manager shows;
`working_set_bytes` is reported beside it. Compare only results from the same
operating system. There is no `psutil` dependency and no PDH query: the
harness reports exactly the source named in `metric.source`.

### Windows thread counts

Windows has no thread count in `PROCESS_MEMORY_COUNTERS_EX`, so the harness
walks a Toolhelp32 snapshot: `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)`,
then `Thread32First` and `Thread32Next`, counting entries whose
`th32OwnerProcessID` is the helper's pid. The snapshot handle is always
closed. The count is every thread the helper owns, engine and compile-pool
threads included; it is not a count of busy threads.

The source is `memory.threads.source`. A refused snapshot, or a snapshot that
lists no thread for the pid, is reported in `memory.threads.errors` rather
than as zero; `memory.threads.samples` then holds only the counts that were
read. A platform with no thread reader is named in `memory.threads.error` and
leaves the samples empty. On Linux the count comes from
`/proc/<pid>/status: Threads` and is already in each memory sample, so the
Windows snapshot is not run there.

## Percentiles

Percentiles use the nearest-rank method: sort the samples and take the value at
the 1-based rank

```
rank = ceil(percent / 100 * n)
```

so the result is always one observed value, never an interpolation. `p50` of
ten samples is the fifth smallest; `p99` of ten is the largest. An empty sample
has no percentile and is reported as `null`.

## Limits

- Python writes and reads the frames, so `round_trip_ns`, `load_ns` and
  `spawn_to_first_call_ns` include interpreter and JSON work that the helper
  does not pay. Use them to compare runs and find gross regressions. The 50 µs
  event budget needs the Rust latency example, which times the same exchange
  without Python in the loop.
- `spawn_to_first_call_ns` contains Extism's instantiation of every instance
  plus the first call, so it is deliberately not comparable with
  `round_trip_ns`.
- Memory is sampled from outside the process, not requested from Extism. The
  sampling interval bounds when a peak can be seen; a short spike between
  samples can be missed, and a spike during a load happens before sampling
  starts. `load_ns` records the load itself.
- `--instances` is one helper process with several Extism instances, not
  several processes. Their memory is one total, not a per-instance split.
- On Windows each thread count walks the system's whole thread list, so a
  sample costs more there than the memory read alone.
- The harness runs one helper at a time and does not build fixtures, manage the
  compile cache, or choose a page limit. Those belong to the build and the
  scenario that invokes it.
- A WSL2 result is indicative: `RssAnon` is inside a VM and spawn timings are
  not the bare-metal budget. Tag it with `--os-tag wsl2`.
- `build.helper_sha256` hashes `--helper`; a helper launched through an
  interpreter (`--helper python --helper-arg helper.py`) hashes the
  interpreter, not the script. The full `argv` is still recorded.

## Tests

```sh
python3 testing/test_measure_plugins.py
```

The unit tests use stdlib Python fake helpers that speak the same frames, so
the codec, spawn-to-ready and spawn-to-first-call labels, per-instance loads,
round trips, reserved ids, timeouts, clean EOF, multi-instance ids and
unloads, memory parsing, thread counting and JSON labels are testable before
the real host exists. The Windows thread count is tested against a fake Win32
API at the ctypes boundary, so its error paths run on any OS. A real helper
run remains an integration step.
