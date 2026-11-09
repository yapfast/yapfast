# Measuring the plugin helper

`testing/measure_plugins.py` measures the separate plugin helper process
(`yapfast-plugins`) without Yapfast, Slint or a display. It spawns the helper,
speaks the framed JSON protocol in [`docs/plugin-protocol.md`](plugin-protocol.md),
and writes one JSON record. It has no budgets: a record contains observations,
and the plan's budgets are applied to those observations elsewhere.

## What it measures

| Field | Meaning |
| --- | --- |
| `latency.spawn_to_first_response_ns` | `time.monotonic_ns` from immediately before spawn to the first frame read from stdout, normally the unsolicited `ready` frame. |
| `latency.round_trip_ns` | `time.monotonic_ns` around one `call` frame: encode and write, flush, then read the `reply` with the same id. Warmup calls are not counted. |
| `memory.samples` | The helper's memory sampled while a module is loaded: once immediately, then at `--interval-ms` until `--samples` are taken or a 2 s grace expires. |
| `memory.peak` | The largest value of each sampled field. |

The frame codec is tested against the protocol's layout: a 13-byte header with
a little-endian `u32` payload length, the kind byte and a little-endian `u64`
id, followed by the payload. A whole frame is at most 1 MiB, so a payload is at
most 1,048,563 bytes. Reads are reassembled from short pipe reads; end of
stream between frames is clean, and end of stream inside a frame is an error.
The helper's stderr is captured (up to 8 KiB) and never mixed into stdout.

## Usage

The helper must already be built; this harness does not run cargo.

```sh
python3 testing/measure_plugins.py \
    --helper target/measure/yapfast-plugins \
    --module "$PWD/target/plugin-fixtures/echo.wasm" \
    --method echo \
    --requests 200 --warmup 20 --payload-bytes 1024 \
    --samples 10 --interval-ms 50 \
    --profile measure --run-label warm-1 \
    --json-out target/plugin-measurements/warm-1.json
```

`--module` must be an absolute path, matching the protocol's `load`
requirement. On Windows that means a drive-letter path (`C:/...`); an MSYS
`/c/...` path from Git Bash is not absolute to Python. Without `--module` the
harness measures spawn-to-first-response and idle memory only;
`latency.round_trip_ns.samples` is then empty and its `percentiles` is `null`.

The helper's own flags go through `--helper-arg`, one argument at a time:

```sh
python3 testing/measure_plugins.py \
    --helper target/measure/yapfast-plugins \
    --helper-arg=--no-cache \
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
| `--requests N` | 200 | Counted round trips. |
| `--warmup N` | 20 | Uncounted round trips before them. |
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

`schema` is `yapfast-plugin-measurement/1`. The labels needed to compare or
reproduce a run are at the top:

| Key | Contents |
| --- | --- |
| `platform` | `os` (`win32`, `linux`), `os_version`, `machine`, `python`, and the `--os-tag` label. |
| `metric` | The per-OS metric's `name`, `unit`, `source`, and a `comparison` note. |
| `build` | The helper path, its SHA-256 and size, the exact `argv`, and the profile label. |
| `scenario` | Module, method, plugin id, request and warmup counts, payload size, sample count and interval. |
| `latency` | `clock` is `time.monotonic_ns`; each block has `samples` and nearest-rank `percentiles`. |
| `memory` | Samples with `t_ns` plus the platform's fields, the count, the per-field peak, and any sampling errors. |
| `helper_exit` | `exit_code`, whether stdout reached a clean EOF, and the captured stderr tail. |

## Memory metrics

| OS | Field gated elsewhere | Also sampled | Source |
| --- | --- | --- | --- |
| Linux | `rss_anon_kib` | `vm_swap_kib`, `vm_hwm_kib`, `threads` | `/proc/<pid>/status` |
| Windows | `private_usage_bytes` | `working_set_bytes`, `peak_working_set_bytes` | `GetProcessMemoryInfo` with `PROCESS_MEMORY_COUNTERS_EX` through `ctypes` |

Windows private usage includes non-resident private pages, so it reads a
little higher than the private working set Task Manager shows;
`working_set_bytes` is reported beside it. Compare only results from the same
operating system. There is no `psutil` dependency and no PDH query: the
harness reports exactly the source named in `metric.source`.

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

- Python writes and reads the frames, so `round_trip_ns` includes interpreter
  and JSON work that the helper does not pay. Use it to compare runs and find
  gross regressions. The 50 µs event budget needs the Rust latency example,
  which times the same exchange without Python in the loop.
- Memory is sampled from outside the process, not requested from Extism. The
  sampling interval bounds when a peak can be seen; a short spike between
  samples can be missed.
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

The unit tests use a stdlib Python fake helper that speaks the same frames, so
the codec, spawn-to-first-response, round trips, timeouts, clean EOF, memory
parsing and JSON labels are testable before the real host exists. A real
helper run remains an integration step.
