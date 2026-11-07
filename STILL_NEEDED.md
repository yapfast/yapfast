# What is still needed

The state of the streaming work on `improve/streaming-codecs-audio` against
[ASTRA.md](ASTRA.md), written at commit `442d7c91`. Six commits sit on the
reviewed head `55e74718`:

| Commit | What it settles |
| --- | --- |
| `2253e846` | F03, F04, F08, E01 — the stream player and a capture's start |
| `78203e95` | F06, F15 — encoder output growth and the buffer pool |
| `1ac9b947` | F02, F05, F07, F09, F10, F13, F14 — shared pictures, switching, demand |
| `36d33b16` | the Graphics Capture window target |
| `1687b5aa` | F01, F11, F12, O09, features 1, 3 (partial), 4, 5, 6 (worker side) |
| `442d7c91` | feature 2's audible position, feature 6's window-side release |

Verified on this machine: `cargo check --profile fast --tests` clean,
`yapfast-core` 572 tests passed (39 ignored), `yapfast-discord` 294 passed,
`allocations` 1 passed, `call_link` 5 passed. `scripts/check` and
`scripts/test-fake-discord` were **not** run: both enter the nix development
shell, which this machine does not have.

## Blocked, not done

### 1. The upstream merge (`upstream-yapfast/main`, 20 commits)

Fetched and merged; the conflicts were only `Cargo.toml`/`Cargo.lock`. Upstream
bumps Slint 1.18.1 → 1.19.0 and skia-safe 0.153.3 → 0.157.0, and its vendored
`i-slint-*` crates and `[patch.crates-io]` set follow that bump. The build then
needs the prebuilt static-CRT Skia archive for `0.157.0`, key
`30669978c3f39a73e8a7-x86_64-pc-windows-msvc-d3d-ganesh-gl-jpegd-jpege-pdf-static`.
It is not in `C:\Users\desu\skia-cache`, and `skia-safe` is pinned with
`no-compile`, so cargo will not build it from source. The tool that makes the
archive, `scripts/prepare-windows-skia.py --build`, fails here: its own builder
cargo project cannot compile `bindgen` outside the nix toolchain.

The merge was aborted rather than commit a tree that does not build. To finish:

```sh
nix develop path:./nix --command python3 nix/prepare-windows-skia.py   # or scripts/prepare-windows-skia.py --build
git merge upstream-yapfast/main                                        # keep upstream's Slint/skia pins
nix develop path:./nix --command cargo check --profile fast --tests
```

Resolution notes for the merge, if it is redone: our additions to `Cargo.toml`
must survive — `unstable-wgpu-30` on `slint` and `i-slint-renderer-skia`,
`dep:pollster` and `dep:windows` in the `desktop` feature, and the `windows`
(0.62, `Win32_Graphics_Direct3D12`, `Win32_System_Threading`) and `pollster`
dependencies, which `src/ui/gpu.rs` needs. Upstream's own additions
(`slint-build`, `i-slint-core-macros`, `vtable`, `const-field-offset` git
patches) must be kept too.

### 2. Intel QSV in the vendored FFmpeg (feature 3)

`vendor/ffmpeg-src/update.sh` regenerates `ffmpeg/`, `include/` and `gen/` and
**requires nix** (it re-execs itself inside `nix shell` for make, clang,
pkg-config, nasm and llvm). Nothing can be regenerated or verified on this
machine, and there is no Intel GPU here to run the result on. The change, for a
host that can build it:

1. Pin oneVPL beside `NVH_VERSION`/`AMF_VERSION`, with its SHA-256, and fetch it
   in the same `fetch` block.
2. Install its headers into `include/` (`vpl/…`), beside `ffnvcodec/` and
   `AMF/`.
3. For the Windows cross build, give FFmpeg's `require_pkg_config libvpl` what
   it asks for: a `vpl.pc` in `$work/vpl/lib/pkgconfig` with
   `PKG_CONFIG_PATH` set for that step, `$work/vpl/include` on `INCLUDE`, and
   the dispatcher import library (`vpl.lib`) on `LIB`.
4. Add `--enable-libvpl --enable-encoder=h264_qsv,hevc_qsv,av1_qsv` to the
   Windows configure line (and the Linux one, if QSV is wanted there), plus
   `--enable-d3d11va` if the zero-copy QSV decode/encode path is wanted — the
   vendored build currently has `CONFIG_D3D11VA 0` and `CONFIG_HWACCELS 0`.
5. `update.sh` must still fail on a path of the machine leaking into
   `config.h`; check that its last guard passes.

Then a real Intel machine has to encode with `h264_qsv`/`hevc_qsv` and a viewer
has to decode it. Nothing in the Rust side advertises QSV yet beyond the
capability probe, so `encode.rs`'s probe must list the QSV encoders once the
build carries them.

## Still to do, in the order that makes sense

### A. Verify the work three stopped subagents left in the tree

These edits compile and their own tests pass, but nobody reported on them:

- `crates/yapfast-core/src/d3d11.rs`, `native/d3d11enc.c`, `src/ui/gpu.rs` —
  F01/F11/O09 (set-numbered shared resources, handles closed after import,
  partial-set rollback, format validation) and F12's first half (`Texture`
  forging made `unsafe`, fields private). **Wanted:** read the native
  duplication/close paths against `frames.rs`'s `SharedTexture`, confirm a
  partial set really rolls back every handle it made, and confirm the UI closes
  the fence handle too.

- `crates/yapfast-core/src/encode.rs`, `src/gpu.rs`, `native/hwenc.c` — the
  capability probe and adapter-preference work for feature 3. **Wanted:** check
  that the probe is cached per session, that a failed backend still falls back
  to x264 with a keyframe, and that NVENC's vendor guard still refuses another
  vendor's device.

### B. Feature 2 — the other half

The audible position is in place (`Output::position`, `LastingOutput::position`,
`Player::heard`, reported as *Sound played* / *Sound queued in the device*).
Missing:

1. Map audio and video positions onto the sender-report timeline: separate
   fields for arrival skew and presentation skew, reset on SSRC/session/device
   change, unknown when either mapping is missing or older than five seconds,
   timestamp wrap accounted for.
2. The bounded presentation scheduler: audible audio as master, decoded pictures
   more than 80 ms late dropped, early ones deferred by at most 80 ms, at most
   one pending picture besides the displayed one, never blocking the UI or the
   network thread, reset on stop/seek/deafen/clock discontinuity.
3. Publish each decoded picture's time to the window (done: `ReceivedFrame::timestamp`
   → `Writer::write_timed`) and record what the renderer actually presented
   using the completion information `src/ui/gpu.rs` already has, labelled
   estimated when only submission is known.
4. Lip-sync on real hardware. Unit tests cannot establish it.

### C. Feature 4 — the window path's transitions

`wgc::Window`, `Target`, `CreateForWindow`, the routing in `graphics_capture`
and `on_the_card`, and the "never the desktop for a window" rule are in.
Untested (they need a window the test owns): resize, minimise/restore, close,
and self-exclusion (`WDA_EXCLUDEFROMCAPTURE`); the CPU window fallback keeping
the same window; repeated switches not growing handles or GPU memory.

### D. Feature 1 — the smaller gaps

- The frame's wait before encoding is measured on the card path only; the
  processor path reports unknown. Either measure it from the capturer's callback
  or say so in the doc (it currently reads as if it were always known).

- Pool live/peak bytes come from the encoder's output pool; the receive side
  (`stream_connection`'s reorder queues, the RTX history) has no counters, so a
  viewer's report has no equivalent.

- The *Copy diagnostics* export covers the share and a watched stream; it has no
  packet or transport counters, which is deliberate (O03) but worth saying in
  `docs/discord/voice.md` if it comes up.

### E. F12 — the rest of the soundness work

`Texture` can no longer be forged by safe code, which stops the worst of it.
Still missing: lifetime-bound leases for textures returned by a source, scaler
or shared set, so a safe caller cannot hold one past the owner that gave it out,
and the compile-fail tests for it.

### F. The review's remaining notes

- The `frames.rs` slot protocol now frees a slot whose picture the reader went
  past (`taken` watermark) and lets the worker claim any free slot. A second
  reviewer pass on that change is worthwhile, since the first pass found the
  starvation bug it fixes.

- `Sink::check` takes its `pressure::Reading` from the loop, so the sampler no
  longer runs under the sink mutex; the doc comment in `pressure.rs` about a
  signal that disappears should be re-read against `Policy::update`'s actual
  behaviour (it ignores an unknown signal rather than ending the wait).

### G. Housekeeping

- `scripts/check --fast` and, for the stream connection changes,
  `scripts/test-fake-discord` have not been run; both need nix.

- `crates/yapfast-core/src/frames.rs` is CRLF in the index while the rest of the
  repository is LF, and the formatter here rewrites whole files with CRLF after
  an edit. Before committing, files whose committed version is LF need
  `perl -pi -e 's/\r\n/\n/'` run over them; a one-off normalising commit would
  end the churn.

- `ASTRA.md` is untracked. It is the review this work answers; commit it or keep
  it out of the tree deliberately.

- Optional follow-ups from ASTRA (O01–O09) were never started: a scaler/readback
  component separate from an NVENC session (O01), the receive-path allocation
  work (O02), retained-memory accounting (O03), the per-frame mmap cost (O04),
  memory tests beyond `GlobalAlloc` (O05), presentation demand from actual
  consumers (O06), fence waiting off the UI path (O07), callback-time growth
  (O08). They are optimisations, not defects, and none is measured.

## Task list

| # | Task | State |
| --- | --- | --- |
| 1 | Prerequisite audio fixes F03/F04/F08 (+E01) | done |
| 2 | Prerequisite encoding fixes F06 + pool F15 | done |
| 3 | GPU presentation ownership F01/F02/F09/F10/F11/F12/O09 | F02/F09/F10 done inline; F01/F11/O09/F12 landed by a stopped agent — **needs the review in A** |
| 4 | Presentation demand hints F13/F14 | done |
| 5 | Feature 1: end-to-end resource diagnostics | done, with the gaps in D |
| 6 | Feature 5: content-aware idle sharing (F07) | done |
| 7 | Feature 4: GPU capture of the selected window | boundary and routing done — **D transition tests outstanding** |
| 8 | Feature 3: Intel/AMD encoders and adapter selection | partial — probe/preference landed unverified; **QSV blocked on the FFmpeg build** |
| 9 | Feature 2: A/V timing and bounded synchronization | partial — audible position done; **mapping, scheduler and hardware lip-sync missing** |
| 10 | Feature 6: memory-pressure policy | done, worker and window sides |
| 11 | Docs cleanup and final check | docs done; **`scripts/check` not run** |
| 12 | Merge `upstream-yapfast/main` | **blocked on the Skia 0.157 archive** |
| 13 | oneVPL in the vendored FFmpeg | **blocked, needs nix + an Intel machine** |
