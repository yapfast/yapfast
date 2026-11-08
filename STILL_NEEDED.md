# What is still needed

The streaming work on `improve/streaming-codecs-audio` at `99a861e4` against
[ASTRA.md](ASTRA.md). The commit messages carry the history; this is what is
open.

## What is settled

| Commit | What it settles |
| --- | --- |
| `53ce186a`, `bd5babc6` | `yapfast/main` merged twice — 61 commits, then 21 (the emoji and sticker pickers). Conflicts were in `stream_worker.rs` (upstream's `Last` logger beside this branch's bounded report queue and non-blocking stop), `call_connection.rs` and `docs/discord/voice.md`. |
| `f47e51d4` | The vendored FFmpeg carries H264, HEVC, AV1 and VP9 decoders and parsers on both targets and, on Windows, `--enable-d3d11va` with the four `*_d3d11va` hwaccels (`CONFIG_D3D11VA 1`, each `CONFIG_*_D3D11VA_HWACCEL 1`). `update.sh` gained the flags and four fixes (SDK GUID symbols left unrenamed, `config_components.asm` kept, `fetch` retries, clang for GCC 14.3, which ICEs on `me_cmp.c`), and `patches/h264-qpel-prefixed.patch`. |
| `b3bb483b`, `706a6f8c` | FFmpeg's own Direct3D 11 decode is a third backend behind `decode.rs`'s seam, tried before Media Foundation: coded-size-aligned surfaces, a pre-allocated `AVD3D11VAFramesContext` pool of 20, 10-bit refused before the pool, the same `VideoProcessorBlt` tile pass, the software fallback untouched. Untested on a GPU. |
| `f2b974a0`, `99a861e4` | Feature 2's other half: `av_timing.rs` maps both clocks onto the sender-report timeline (arrival and presentation skew apart, five-second staleness, resets, wrap), and a picture more than 80 ms behind the audible sound is dropped. Early pictures are shown when they arrive: a decoded picture is the decoder's own only until its next frame, so a deferral would cost a copy of every picture, and `docs/discord/go-live.md` says so. Lip sync on hardware is unestablished. |
| `ebc9b13d`, `99a861e4`, `61946378`, `228998ab` | The stopped agents' work reviewed, findings fixed: the frames ring kept back and front on one slot after three publishes; the pressure timer credited unobserved time; the capability probe had no caller and went; `reopen` named the wrong engine and a dropped packet never asked for a keyframe; every handle in the D3D11 shared-set sequence is released and the texture cache can't evict a live slot. |
| `98f3ac4e`, `6a6c81fa` | `scripts/check` ran on this branch: formatting, clippy and dead-code fallout fixed, CRLF files normalised. Upstream's `d1eb59f0` measures a picture's edge against its own middle, so the emoji `round_edges` test passes at Skia 0.157 again. |

Here `cargo check --profile fast --all-targets` is clean and `yapfast-core`
passed 649 tests with 30 ignored. Nothing below is verified on a graphics card:
no d3d11va decode has run on one, and Media Foundation's decoder has no
recorded measurement.

## Blocked, not done

**Intel QSV (feature 3)** is blocked for one reason: oneVPL is not in the
vendored build. `update.sh` fetches no oneVPL, has no `vpl/` headers, no
`vpl.lib` or `vpl.pc` for the Windows cross build and no `--enable-libvpl`, so
`CONFIG_LIBVPL` is 0 on both targets. It needs nix, which is here now, and an
Intel machine to verify; `encode.rs`'s probe must list the QSV encoders once the
build carries them.

Nothing else waits on code; the rest needs Windows hardware.

## Task list

1. **Verify the card's decode on real hardware**, on NVIDIA, AMD and Intel:
   d3d11va decode, `av_hwdevice_ctx_init` accepting the shared device, whether
   20 surfaces satisfy the hwaccel (FFmpeg's static pool never recycles an
   index), a mid-stream resolution change, 10-bit falling back as documented,
   AV1 without Microsoft's extension, VP9 from the player, and a driver reset.
   Measure CPU per frame at 1080p and 4K, time to first picture, memory, and
   codec coverage per machine, and establish lip sync.

2. **Compare Media Foundation with FFmpeg's decode**, then delete the loser:
   `native/d3d11dec.cc`'s Media Foundation half, `decode.rs`'s Media Foundation
   glue, and the libwebrtc software pair if the vendored decoders measure no
   worse. The video-processor pass, the tile slots, the device sharing and the
   fallback decisions stay.

3. **Retry Media Foundation after a runtime failure.** `Hardware` opens only
   the first backend that takes the codec, so a stream FFmpeg gives up on
   mid-decode falls to software without Media Foundation ever seeing it.

4. **Feature 4's window transitions still need a window.** What a test can
   decide without one is now decided: the card's target is the kind the caller
   asked for (`Source::Window` gives `Window(handle)`, everything else gives a
   display, a camera or a test pattern is refused) and nothing captures a
   window as the desktop (`stream_worker.rs`). Resize, minimise and restore,
   close, self-exclusion (`WDA_EXCLUDEFROMCAPTURE`), the CPU fallback keeping
   the same window, and repeated switches not growing handles or GPU memory
   still need a Windows machine with a window the test owns.

5. **F12's leases are in**, as a borrow rather than a reference count:
   `lease.rs`'s `Lease<'a>` is a zero-sized token made only from an owner's
   borrow, so holding a texture past its owner is a compile error, and the
   compile-fail doctest says which error. Carrying one per frame costs no
   allocation, where `Arc` or an extra `AddRef` would make holding past the
   owner legal. What remains is the review of where it is required — a
   scaler's output and a shared set's pictures now hand out a lease, and their
   callers had to change with them.

6. **The remaining small gaps.** The frame's wait before encoding is measured on
   the card path only; the processor path reports unknown, and the docs now say
   so. The receive side has no pool counters (the stream connection's reorder
   queues and resend history), so a viewer's report has none. The *Copy
   diagnostics* export carries no transport counters, deliberately.

7. **oneVPL and QSV**, as in the blocked section: pin and fetch oneVPL, install
   its headers, give the cross build `vpl.pc` and `vpl.lib`, add the configure
   flags, and keep the guard that no path of the machine leaks into
   `config.h`.

8. **ASTRA's O01–O09**: a scaler separate from an NVENC session, the receive
   path's allocations, retained-memory accounting, the per-frame mmap cost,
   memory tests beyond `GlobalAlloc`, presentation demand from actual consumers,
   fence waiting off the UI path, and callback-time growth. None is started and
   none is measured; they are optimisations, not defects.

9. **The last gate has run.** `scripts/check --fast` passes at this head in
   the nix development shell (formatting, doc links, clippy with `-D warnings`,
   the workspace's tests, the build and the smoke run), and so does
   `scripts/test-fake-discord` against `testing/fake_discord.ts`. One caveat:
   `stream_worker::tests::a_worker_that_dies_says_how`, which upstream added,
   failed once in four gate runs on a loaded machine and never in twelve runs
   alone. It reads the log as soon as the `Ended` report arrives, and
   `exited()` gives up after one second and writes "still running"; decide
   which of the two is racing before trusting a red gate.

10. **Two costs accepted for now, neither measured.** A window that stops
    retiring pictures for more than a second makes a share give the card path
    up for the rest of the stream — the escape from a stuck ring — and nothing
    retries it if the window recovers. And a second `EAGAIN` on a retried
    `send_packet` still ends the card path rather than draining again.
