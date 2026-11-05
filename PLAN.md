# Streaming, video codec, and audio optimization plan

## 1. Direction agreed with the user

- **Primary test target:** Windows with an NVIDIA GPU. Exact GPU, driver, and CPU
  are not yet specified. Detect capabilities; do not assume AV1 encoding is available.
- **Scope:** Discord first, covering Go Live sending/viewing, shared system audio,
  and microphone/voice quality. Preserve Matrix/LiveKit behavior in shared code;
  a dedicated Matrix rewrite is not part of this effort.
- **Goals:** sharper video and smoother motion; lower latency and fewer stalls;
  lower CPU/RAM/GPU cost; better audio and A/V synchronization. No single goal
  justifies a silent regression in the others.
- **Working style:** implementation-led, with focused correctness tests and quick
  before/after checks. A new benchmark framework is not a prerequisite for useful work.
- **Parallelism:** independent specialist workstreams are welcome. Parallelize
  investigation, isolated changes, and tests that do not contend for shared state.
  Serialize integration, heavy builds, and measurements on the same machine.

This document is a roadmap, not a statement that the optimizations are implemented
or measured. Every checkbox stays open; what has landed is listed below, and
nothing here counts as measured until a number is beside it. Proposals are
grounded in repository review; future specialist assignments are described in
section 13.

### What has landed

On `improve/streaming-codecs-audio`, each with its doc updated in the same
commit. These close the items named, not the whole section around them.

| Item | Change | Commit |
| --- | --- | --- |
| C1, C2 | Conversion and scaling write into kept buffers, the status lock is never held while a frame is made, and a stale capture generation can't write into the one that replaced it | `5dd91e2`, `ffb09f2` |
| C3 | Frames a stall swallowed are skipped rather than chased | `cf90bf9` |
| C4 | A preview is made only while the UI shows one — including a grid scrolled past the tile | `6e07c38`, `d242374` |
| E1 | An encoded frame owns its bytes from a bounded pool of three | `3ffea45` |
| E2 | The format the encoder took and the clock its frames carry are stated, not assumed; a late packet is dropped, not relabelled | `5032635` |
| E3 | Settings are required or optional, so an open fails on what it needs; a backend passed over says why; AMF's AV1 no longer fails on an `aud` it hasn't got | `9e73614`, `dbb6164` |
| E4 | NVENC's tuning is one table with the applied configuration logged; the default is unchanged pending measurement | `320e31e` |
| E5 | The declared colour is the colour the pixels are | `dbb6164` |
| T1 | Packetizing allocates nothing per frame: one reused plan and one datagram buffer | `5d8b902` |
| T2 | The RTX history reuses buffers, and nothing older than two seconds is resent | `df7e492`, `9e9827c` |
| T3 | A frame's packets go out over many turns of the loop | `958ff2e` |
| T4 | A keyframe is made at most twice a second; a fall is obeyed at a tenth and a rise at a quarter; a stale estimate is forgotten; a backlogged sender skips the encode | `a8ae641`, `d3fd7ec`, `9e9827c` |
| §7 | A decode failure waits for a keyframe; an unknown codec keeps the sound; threads come from a budget the app divides; a replaced picture isn't converted or shown | `215394e`, `5cbc3cc`, `ffb09f2` |
| A1, A2 | Opus settings are checked and reported, and written down where they were only defaults | `27ad894`, `7c6b115` |
| A3 | Capture is bounded without oversizing; the mix doesn't clip and a driver's NaN stops at the input | `b93604b`, `03b13ab` |
| A4 | Received sound carries its numbering and arrival time; the playout reorders, primes, conceals, recovers by FEC and restarts correctly | `56a0f7e` |
| A5 | The hold is bounded in held time, a bound drops the sound that came too early, and loss is counted apart from drift | `e8228e8`, `04fc83d` |
| W1 | Device waits are cancellable and never count as a lost device; a stream starts as soon as sound is queued; an unnamed endpoint doesn't move a working stream | `41b2406`, `6965384` |
| W2 | The loopback leaves out the right process instead of guessing, and the engine's buffer is borrowed only for a copy | `87529b5` |
| M1 | A suppressor loads between blocks, one network at a time; the chain was audited and refuted duplicate processing with measurements | `995e1f2` |
| §11 | Reports cross a bounded queue, stopping doesn't block the call, a stopped share stops reading as live, and the tab shows what bounds gave up | `480374e` |
| §12 | Assessed and **not built**: every experiment needs the NixOS measurement rig, a rebuilt FFmpeg, or hardware this machine hasn't got; see the note below | — |

Section 12's verdict, so it is not re-opened blindly: Windows GPU
capture-to-NVENC needs a replacement capture path and an FFmpeg configured with
`--enable-d3d11va`; GPU conversion pays off only after that; hardware
decode/render interop needs a rebuilt FFmpeg and still downloads to CPU to
display; Intel QSV needs oneVPL headers the vendor lacks; the Linux surface
paths already exist. All of them fail section 14's evidence bar on a machine
without the rig.

Still open: one bullet of section 7 — doing less presentation work while the
window is hidden or minimised without stalling reference decoding or the sound.

Everything else that remains is **waiting on measurement, not on code**: E4's
choice of preset, E5's proof that a different colour claim would look right, A2's
faster complexity setting, and A5's audio-to-video clock mapping. Each needs the
NixOS/NVIDIA rig that produced `docs/measurements/README.md`, a viewer comparison,
or a listener; none can be settled on a machine that can't run the app against a
real screen. Section 14's rule stands: a claim without a number stays open.


**Terminology:** AV1, H.265/HEVC, H.264, and Opus are codecs. NVENC (NVIDIA),
AMF (AMD), and VA-API (Linux hardware acceleration) are encoder backends.
Compare codec/backend combinations, not “AV1 versus NVENC.”

### Guardrails

1. Keep working hardware/software fallback and the ability to start without GPU drivers.
2. Keep media latency and retained memory bounded; do not move a backlog downstream.
3. Preserve negotiated codecs, RTP timing, DAVE, transport encryption, and RTX semantics.
4. Reuse existing capture, audio, packet-loss, and fake-server infrastructure.
5. Make quality changes deliberate and reversible, not hidden preset changes.
6. Avoid extra dependencies or large abstractions unless they solve a concrete problem.
7. No autonomous production Discord testing: follow the project's fake-only rule.
8. Update behavior documentation alongside each implementation change.

## 2. Current implementation and concrete starting points

| Area | Existing behavior | Main implementation |
| --- | --- | --- |
| Video negotiation | GPU AV1, then H.265, then H.264 where available; server chooses a compatible codec. H.264 can fall back to x264. | `crates/yapfast-core/src/encode.rs`, `crates/yapfast-core/src/stream_worker.rs` |
| Hardware | FFmpeg NVENC, Windows AMF, Linux VA-API; Intel Windows currently falls back to x264. NVENC p4/low-latency, no B-frames, runtime bitrate updates. | `crates/yapfast-core/native/hwenc.c`, `vendor/ffmpeg-src/` |
| Capture | Desktop frames become I420 and may be scaled. Capture callback calls the sink while holding capture-state and sink locks. | `Capture::start`, `picture_of`, `Sink::frame` in `crates/yapfast-core/src/stream_worker.rs` |
| Preview | `Sink::frame` scales/converts a local preview on every incoming frame before encoding; server thumbnails are separate, periodic work. | `crates/yapfast-core/src/stream_worker.rs` |
| Video rate | Caps of 3.5 Mbit/s through 720p and 9 Mbit/s above; REMB with a 500 kbit/s floor and 10% change threshold. | `kbps`, `Sink::frame` in `crates/yapfast-core/src/stream_worker.rs` |
| Media transport | Bounded send/receive media channels, packet-burst pacing, NACK/RTX, reorder buffers, and sender reports already exist. | `crates/yapfast-discord/src/stream_connection.rs`, `crates/yapfast-discord/src/rtp.rs` |
| Packet memory | Packetizer builds payload collections then packet collections; RTX history has 2048 slots and copies into a new vector on replacement. | `Packetizer::packets`, `History::remember` in `crates/yapfast-discord/src/rtp.rs` |
| Receive video | Software decoding via dav1d/FFmpeg/libvpx; large streams use up to four decode threads. Frames go through shared-frame output, not JSON pixel transport. | `crates/yapfast-core/src/decode.rs`, `watch` in `crates/yapfast-core/src/stream_worker.rs` |
| Shared audio | 48 kHz stereo Opus, 20 ms frames, 64 kbit/s; 40 ms application priming, 200 ms per-input backlog. | `crates/yapfast-core/src/stream_audio.rs` |
| Stream audio receive | Bounded queue of Opus payloads, but no RTP timing metadata in the player interface; roughly 100 ms requested output buffering and decode with FEC disabled. | `StreamReceiver` in `crates/yapfast-discord/src/stream_connection.rs`, `Player` in `crates/yapfast-core/src/stream_audio.rs` |
| Call audio | 128 kbit/s stereo Opus in Audio mode; receive-side jitter/loss handling already exists. FEC is enabled by the wrapper only for Voice mode. | `crates/yapfast-discord/src/call_connection.rs`, `crates/yapfast-discord/src/opus.rs` |
| Windows audio | Event-driven WASAPI shared-mode audio, automatic format conversion, process-loopback capture, and device recovery wrappers already exist. | `crates/yapfast-core/src/audio_io/wasapi.rs`, `crates/yapfast-core/src/audio_io/lasting.rs` |
| Microphone | Echo cancellation, denoising, optional dereverb, gain/leveling, gate, and push-to-talk. Reuses existing DSP state where implemented. | `crates/yapfast-core/src/discord_audio.rs` |

Treat these as implementation observations, not measurements of which component
is currently the bottleneck. Preserve existing protections rather than rebuilding them.

## 3. Priorities and an implementation-first workflow

**Priority A — start here:** remove avoidable allocations, bound report delivery,
prevent catch-up bursts, preserve audio timing, harden codec configuration errors,
and improve packet/audio scheduling fairness.

**Priority B — next:** reusable capture/preview buffers, better stream-audio playout,
clipping control, adaptive bitrate/frame scheduling, NVENC tuning, and lifecycle fixes.

**Priority C — larger optional changes:** GPU texture capture/encode, hardware decode,
new Windows Intel support, fuller congestion control, and new user-facing profiles.

These are risk/effort categories, not an obligation to finish every A task before
starting independent B tasks. Each change should state its expected benefit, touched
path, ownership/lifetime rules, failure behavior, and smallest meaningful check.

Do not require a multi-GPU lab, VMAF pipeline, or long-running benchmark suite before
fixing an unnecessary copy or adding a regression test. Retain a quick representative
text/motion/audio sample and existing counters; add instrumentation only to answer
a concrete question. More extensive comparison is reserved for tuning defaults,
GPU interoperability, and claimed CPU/memory/quality improvements.

## 4. Capture, frame scheduling, and preview work

Owner: **capture/video pipeline**. Primary file: `crates/yapfast-core/src/stream_worker.rs`.

### C1. Reuse conversion and scaling buffers — A/B

- [ ] Replace fresh desktop I420 allocations in `picture_of` with a small reusable
  buffer set keyed by dimensions/format. Resize on geometry changes, not every frame.
- [ ] Provide reusable scaling destinations where the wrapper/native API permits;
  avoid allocating a new scaled image for both outgoing video and local preview.
- [ ] Keep row-stride handling correct and use existing libyuv primitives rather
  than hand-written pixel loops where practical. Do not alias native buffers beyond
  their documented callback lifetime.
- [ ] Cap retained oversized buffers after a 4K-to-720p switch; release them when
  sharing stops rather than keeping the session's peak allocation forever.

**Check:** resize repeatedly, odd source dimensions/strides, stop/restart; verify
correct pixels and that allocation reuse does not retain unbounded capacity.

### C2. Shorten capture callbacks and lock scope — B

- [ ] Move encoding, preview generation, and thumbnail compression out of long
  capture-state critical sections. Update status under a short lock, not while
  running the whole sink. Define a lock order before moving responsibilities.
- [ ] If a separate processing stage is justified, use a bounded latest-raw-frame
  handoff, preferably with reusable storage. Replace stale raw frames before encode.
  Do not add a thread/queue merely to move the same blocking work elsewhere.
- [ ] Preserve source-switch behavior: keep the old source until the new one has
  a usable frame; distinguish source generations so late callbacks cannot revive
  a cancelled source. Release old captures, handles, and buffers promptly.

**Check:** slow encoder, rapid source switches, failed source, resize, stop while
capture is in flight; no deadlock or old-source frame after switch completion.

### C3. Avoid catch-up bursts and pointless captures — A/B

- [ ] `next_frame` uses absolute deadlines but advances one frame at a time.
  After a stall, skip expired deadlines instead of immediately performing all missed
  iterations. Keep elapsed-time RTP timing correct when frames are skipped.
- [ ] Give raw-frame dropping explicit reasons: missed deadline, encoder busy,
  transport saturated, source unchanged. Track aggregate counts, not per-frame logs.
- [ ] Explore capture damage/unchanged-frame information when the backend exposes
  it. Skip unnecessary conversion/encode for static content, while preserving cursor
  changes, new-viewer keyframes, periodic recovery, and source state updates.
- [ ] Do not hash whole 4K frames on every tick just to claim duplicate suppression.
  Keep the ordinary capture path when change detection costs more than it saves.

**Check:** inject a short stall, then verify no burst of stale frames and correct
recovery; static screen, cursor motion, new viewer, and window movement.

### C4. Make local preview optional and cheaper — A/B

- [ ] Introduce explicit preview visibility/demand state rather than assuming every
  captured frame must be converted to RGBA for a visible consumer.
- [ ] Cap local preview cadence independently of stream FPS; reuse its last image
  between updates. Resume with a fresh picture when the preview becomes visible.
- [ ] Separate periodic uploaded thumbnails from local-preview demand. Thumbnails
  must still refresh when the local preview is hidden; avoid extra full-size copies.
- [ ] Keep shared-frame transport. Audit redundant notifications and UI wakeups
  before changing IPC formats; the current video path is not raw JSON frame transport.

**Check:** hide/show/minimize preview, share at 60 fps, request a thumbnail, then stop.

## 5. Encoder ownership, correctness, and GPU operation

Owner: **encoder/backend**. Primary files: `crates/yapfast-core/src/encode.rs`,
`crates/yapfast-core/native/hwenc.c`, `crates/yapfast-core/native/h264.c`.

### E1. Return reusable owned encoded output — A/B

- [ ] `Sink::frame` copies `frame.data` because the encoder exposes a borrowed
  buffer. Design an owned output buffer/pool handoff that can survive asynchronous
  sending; never remove `to_vec()` while keeping the old borrowed lifetime.
- [ ] Trace native packet-to-Rust copying too. Remove a copy only when packet
  ownership/refcounting or caller-owned output safely spans the FFI boundary.
- [ ] Bound pool size, oversized keyframe capacity, and in-flight buffers. Return
  buffers on send failure, stale-codec discard, cancellation, and encoder error.

**Check:** queued frames remain unchanged across subsequent encode calls, codec
switch/fallback, and dropped sends. Use a small allocation check, not a new framework.

### E2. Make native configuration and timestamps explicit — A

- [ ] Check relevant `av_opt_set` and encoder-control return values. Separate
  mandatory low-delay/correctness requirements from optional quality improvements;
  downgrade unsupported optional knobs with one useful diagnostic.
- [ ] Record the effective backend, codec, pixel format, and configuration once per
  open/change. Avoid silently claiming a quality preset the driver ignored.
- [ ] Audit the one-input/one-output assumption. `yapfast_hwenc_encode` currently
  ignores caller PTS and assigns sequential frame indices; asynchronous or delayed
  output must not be labeled with a newer input frame's RTP timestamp.
- [ ] Preserve output PTS/keyframe association if changing polling or introducing
  asynchronous encoding. Test EAGAIN, delayed packets, encoder restart, and flush.
- [ ] Bound backend waits and make shutdown responsive. AMF's polling is an existing
  behavior to handle carefully, not evidence that NVENC needs the same polling loop.

### E3. Improve capability selection and failure behavior — A/B

- [ ] Probe actual codec/size/format support. Distinguish unsupported codec,
  unsupported size, temporary resource exhaustion, and device loss.
- [ ] Avoid repeated full encoder probes after transient events. A small session
  capability cache may help, but must invalidate on device failure or changed settings.
  Do not hold unused encoders open just to cache support; they consume GPU resources.
- [ ] Keep negotiation authoritative. Test viewer-driven AV1/HEVC-to-H.264 switches,
  failed reopen at a new size, and H.264 hardware failure to x264.
- [ ] Avoid fallback/retry storms: bounded attempts, stable active backend, explicit
  withdrawal of failed codecs, and a deliberate route to retry on a new session.
- [ ] If adding asynchronous work, attach an encoder/source generation to results
  so old output cannot leak into a new stream. Do not enqueue two codec generations
  behind each other and hope payload types alone solve decoder initialization.

### E4. Tune NVENC deliberately — B

- [ ] Keep p4/low-latency as the starting point. Existing docs already report that
  a slower preset did not improve quality in prior work; do not blindly select p7.
- [ ] Compare a small nearby preset range for text and motion at equal bitrate.
  Consider low versus ultra-low latency, VBV size, and rate-control mode separately.
- [ ] Evaluate spatial/temporal AQ only where codec and driver support it. Account
  for CUDA/GPU contention with a game; encoder utilization alone is not total GPU cost.
- [ ] Keep B-frames and lookahead off in the low-latency baseline. Any profile that
  enables buffering must expose its latency/memory cost and remain optional.
- [ ] Compare capped VBR against CBR without padding static screens needlessly.
  Optimize keyframe burst size and join/recovery delay together, not bitrate alone.
- [ ] Keep AV1/HEVC/H.264 choice dependent on negotiated support, encode latency,
  decode cost, and fallback reliability. No universal codec winner is assumed.

**Check:** one text sample and one motion sample on the user's NVIDIA system,
plus forced keyframe, bitrate change, resize, and software fallback. Broader tuning
requires broader hardware evidence, not a block on unrelated correctness changes.

### E5. Color, format, and non-NVIDIA preservation — B

- [ ] Validate conversion and signaling together: current hardware frames are
  labeled limited-range BT.601 to match the existing conversion. A BT.709 path
  requires matching pixels and metadata; changing only metadata is incorrect.
- [ ] Test black/white levels, saturated text edges, gradients, and scaling artifacts.
  Preserve even dimensions, crop information, and AMD AV1 alignment requirements.
- [ ] For AMF/VA-API NV12 filling, evaluate existing vectorized libyuv conversion
  instead of the scalar chroma-interleave loop. Keep stride/padding behavior exact.
- [ ] Preserve x264's dependable CPU path and existing effort option; avoid letting
  expensive presets build a queue on low-core-count machines.
- [ ] Do not change AMF/VA-API defaults merely because an NVENC setting improved.
  Compile/test shared changes and label hardware-specific runtime behavior unverified.

## 6. RTP memory, packet scheduling, and congestion

Owners: **packet memory** and **transport scheduler**, with agreed interface boundaries.
Primary files: `crates/yapfast-discord/src/rtp.rs`, `crates/yapfast-discord/src/stream_connection.rs`.

### T1. Packetize incrementally — A/B

- [ ] Replace intermediate payload and packet collections with a bounded iterator
  or emitter that fills reusable datagram storage, if it simplifies ownership.
- [ ] Preserve last-packet marker, sequence advancement, codec fragment headers,
  and extensions. Handle empty/malformed input without partially sending a frame
  that the current implementation would reject completely.
- [ ] For AV1, validate OBU structure first or otherwise retain all-or-nothing failure
  behavior; incremental emission must not turn malformed data into a partial stream.
- [ ] Reserve room for transport authentication tags/nonces once. Reuse scratch
  storage without reusing encryption nonces or mutating a packet still needed by RTX.

**Check:** existing codec packetize/reassemble fixtures, malformed AV1, sequence
wrap, encryption round trips, MTU budget, and keyframe fragmentation.

### T2. Reuse and age RTX storage — A

- [ ] Reuse the vector in each `History` slot instead of allocating a replacement
  for every packet. Keep sequence validation when slots wrap.
- [ ] Define byte/time limits alongside the current slot limit. If adding age expiry,
  use a transport-appropriate recovery window rather than expiring useful packets early.
- [ ] Preserve the encryption layers: history currently stores packets before outer
  transport sealing, after frame-level processing. RTX must be rebuilt/resealed
  correctly, not resend ciphertext with reused nonce/header state.
- [ ] Coalesce repeated NACK entries within a bounded pass and budget retransmission
  traffic so old video cannot monopolize bandwidth needed by current audio/video.

**Check:** repeated NACK, wraparound, expired history, RTX payload type/sequence,
and bounded allocation under a sustained packet stream.

### T3. Keep audio and control responsive during video sends — A/B

- [ ] The send loop currently sleeps inside a per-video-frame packet loop. Make
  video emission resumable in bounded batches so audio, RTCP, gateway heartbeats,
  cancellation, and feedback can be serviced between batches.
- [ ] Use a byte/deadline pacing budget rather than only packet-count sleeps if
  warranted. Include keyframe bursts and retransmissions; account for protocol overhead.
- [ ] Do not hold a mutex across `.await`. Keep transport nonce/sequence ownership
  in one place; multiple send tasks must not race a shared packetizer or cipher.
- [ ] Give audio a small latency budget without permanently starving video. Cap
  retries and wakeups; avoid per-packet timer/task allocation where a paced batch works.
- [ ] Distinguish full media queues from closed connections. Avoid asking for endless
  keyframes after the consumer has exited. Surface terminal failure promptly.

**Check:** oversized keyframe plus active audio, stop while paced, feedback arrival
mid-frame, and bandwidth reduction. Assert fairness rather than exact wall-clock timing.

### T4. Drop at the correct stage and adapt smoothly — B

- [ ] Signal downstream pressure before encoding when practical. The current full
  encoded queue drops a frame and requests a keyframe; avoid wasting encode work
  and repeatedly manufacturing large recovery frames during sustained saturation.
- [ ] Bound encoded queues by bytes and age as well as count. Never discard arbitrary
  encoded reference frames without a recovery plan; prefer dropping raw frames.
- [ ] Treat stale/missing/implausible REMB separately from healthy low bandwidth.
  Revisit the 500 kbit/s floor under a path that genuinely cannot carry it.
- [ ] Use fast reductions and conservative increases with hysteresis. Preserve
  runtime bitrate updates without reopening the encoder for every small fluctuation.
- [ ] Add staged frame-rate/resolution adaptation only after bitrate handling is
  stable. Avoid rapid resize/reopen cycles; make recovery slow enough to prevent oscillation.
- [ ] Investigate real transport-wide congestion feedback before implementing a new
  controller. Emitting the extension alone is not congestion control. Do not modify
  Discord payloads/events without checking its protocol documentation and fake shape.

## 7. Receiving, decoding, and displaying streams

Owner: **receive/display**, coordinated with capture owner for `stream_worker.rs`.

- [ ] Audit reorder/assembler/frame queue retention as a combined byte/time budget.
  Existing packet-count, frame-size, and SSRC limits must remain; tighten only with
  awareness of legitimate large keyframes and packet recovery windows.
- [ ] Coalesce repeated decode-failure keyframe requests; rate-limit noisy errors
  while retaining enough information to diagnose corruption or unsupported formats.
- [ ] Recreate/reset decoders on genuine codec/stream changes and require a usable
  recovery frame. Avoid presenting stale frames after a restart or source generation change.
- [ ] Keep decoding reference frames needed for continuity, but skip unnecessary
  RGBA conversion/scaling/presentation when a newer decoded picture replaces them.
  Do not mistake “latest displayed frame” for “safe to skip any compressed frame.”
- [ ] Budget decoder threads across active streams; current per-stream up-to-four
  threads can oversubscribe a small CPU. Retain efficient software decode for small
  streams rather than forcing hardware everywhere.
- [ ] Reuse conversion scratch and respect output dimensions. Verify repeated
  resize cannot retain a growing chain of image buffers or trigger UI work per packet.
- [ ] Define hidden/minimized behavior: less presentation work, bounded receive
  state, and fast visible recovery. Do not pause reference decoding blindly or
  silently stop audio because a video window is hidden.
- [ ] Consider hardware decode only after software decode is a demonstrated issue.
  Include device setup, output surfaces, color conversion, GPU readback, and rendering
  interop; a hardware decoder followed by readback may cost more than dav1d.

**Check:** codec switch, missing keyframe, malformed fixture, repeated resize,
multiple streams, hidden/visible transitions, and decoder fallback.

## 8. Opus configuration and shared-audio quality

Owner: **audio codec/mixer**. Primary files: `crates/yapfast-discord/src/opus.rs`,
`crates/yapfast-core/src/stream_audio.rs`, `crates/yapfast-discord/src/call_connection.rs`.

### A1. Make Opus configuration safe and explicit — A

- [ ] Check encoder CTL return values instead of silently accepting failure. Keep
  the wrapper small and typed enough to prevent invalid bitrate/mode combinations.
- [ ] Verify the linked libopus version before relying on newer controls or features;
  do not introduce another independently linked copy of libopus.
- [ ] Separate shared music/game audio from microphone intent. Preserve 48 kHz and
  negotiated channel compatibility; Opus remains Discord's codec, not replaceable
  unilaterally with AAC, FLAC, LC3, or another preferred codec.
- [ ] Preserve timestamp advancement through pauses and existing silent-tail behavior.
  DTX or different frame duration is a protocol/timing change, not a free optimization.

### A2. Offer useful quality choices, not arbitrary high numbers — B

- [ ] Compare stream audio's current 64 kbit/s stereo against candidate 96/128 kbit/s
  where service constraints and interoperability permit. These are proposed experiments,
  not verified Discord limits or promised defaults.
- [ ] Evaluate complexity and VBR/constrained-VBR trade-offs with a short level-matched
  speech/music sample. Keep 20 ms packets as the starting point; shorter packets raise
  packet overhead and CPU, while longer packets add latency and loss exposure.
- [ ] Keep speech-specific processing and DTX away from shared music by default.
  A microphone speech profile may be useful, but changing production Audio mode
  requires checking bandwidth, stereo/wire compatibility, and existing DSP tuning.
- [ ] For FEC, use supported controls and an actual loss estimate. Verify encoded
  output really contains recoverable data in the chosen mode; enabling a flag is not
  evidence of effective FEC, especially across Opus operating modes.
- [ ] Preserve graceful runtime changes: no burst of stale PCM, loud gain jump, or
  unnecessary encoder recreation when a supported control can update in place.

### A3. Improve mixing, input bounds, and buffer reuse — A/B

- [ ] Bound samples before extending `Input.samples`: current extend-then-trim can
  temporarily allocate for an oversized callback. Preserve newest useful samples,
  stereo alignment, and bounded capacity even for unusually large input chunks.
- [ ] Keep input-state locks short. Avoid coupling a slow encoder or downstream
  channel to the capture callback; reuse mixer and encoded-packet storage where safe.
- [ ] Compare modest headroom or a lightweight peak limiter with hard clipping when
  multiple loud applications play. Avoid latency-heavy lookahead, pumping, or blanket
  attenuation of quiet material. Handle non-finite samples at the input boundary.
- [ ] Preserve per-application priming/recovery and remove inactive inputs promptly.
  Do not let a silent/disconnected application's buffer remain active indefinitely.
- [ ] Make audio deadline handling skip overdue wall-clock ticks with an explicit
  sample/timestamp discontinuity policy, rather than sending a catch-up burst. Avoid
  arbitrary sample deletion that creates clicks; use bounded recovery when needed.

**Check:** two loud sources, silence/restart, oversized callback, packet-send failure,
source removal, and scheduling stall. Use generated tones and a short listening sample.

## 9. Stream audio playout, loss recovery, and A/V sync

Owner: **audio playout**, sharing an interface with transport rather than concurrently
rewriting `stream_connection.rs`.

### A4. Preserve timing and order from the network — A/B

- [ ] Replace the payload-only receive boundary with a small owned audio-frame type
  carrying RTP sequence, timestamp, SSRC/generation, and arrival time as needed.
  Preserve metadata through decrypt/queue; do not duplicate packet payloads just
  to attach timing fields.
- [ ] Extract/reuse appropriate call-side jitter/clock/loss logic after reading its
  assumptions. Do not copy the entire call implementation or replace it without cause.
- [ ] Add bounded reordering, late/duplicate discard, timestamp-wrap handling, and
  startup priming. Separate network jitter from playback-device buffering.
- [ ] Schedule PLC for missing audio; attempt next-packet FEC only when available
  and within the playout budget. Decode the following packet normally afterwards.
- [ ] Reset timing appropriately on SSRC/connection changes and long silence. Keep
  queue age bounded after deafen/unmute and device loss; do not replay old sound.

### A5. Correct drift without accumulating latency — B

- [ ] Track playout occupancy in time/samples and actual packet timestamps. Use
  conservative bounds for increasing/reducing jitter delay, not an ever-growing queue.
- [ ] Distinguish packet loss from capture/playback clock mismatch. Consider a small
  resampling-ratio correction only where clock evidence supports it; avoid audible
  drop/duplicate cycles as normal steady-state drift control.
- [ ] Establish an audio/video clock mapping using complete RTP/sender-report timing
  where available. Existing reports/parsers may need richer timing data; do not infer
  cross-stream synchronization from arrival time or a truncated report identifier alone.
- [ ] Prefer dropping late video presentation to arbitrarily delaying audio. Bound
  any video hold and recover after codec switch, source switch, and timestamp reset.
- [ ] Expose current queue delay and A/V skew in optional diagnostics, not raw media
  logs. A proposed healthy-link target is absolute skew within 80 ms; confirm feasibility
  rather than treating this as an already measured guarantee.

**Check:** deterministic loss/reorder/wrap tests, mute/deafen transitions, and one
long drift session only when clock/playout logic changes. No requirement to run
30-minute tests for every unrelated encoder or documentation change.

## 10. Windows audio latency and microphone processing

Owners: **Windows audio backend** and **microphone DSP**.

### W1. Improve WASAPI startup, recovery, and shutdown — A/B

- [ ] Keep event-driven I/O; it already exists. Inspect requested versus effective
  device period/buffer and actual padding before reducing latency targets.
- [ ] Separate startup priming from steady-state queue length. `Output::write`
  currently starts when the requested queue fills; ensure short sounds and restarted
  output do not wait indefinitely for more samples or produce an avoidable delay.
- [ ] Investigate lower-latency shared-mode initialization with newer Windows audio
  interfaces only as an optional capability. Preserve the current initialization path
  when a device/OS rejects it; do not impose exclusive mode on the user's audio device.
- [ ] Give blocking device waits a cancellation route where needed. `Stream::wait`
  can wait up to two seconds; make stop/device-change behavior deliberate rather than
  waiting on a potentially lost event while holding unrelated resources.
- [ ] Surface WASAPI discontinuity/timestamp information where useful for playout
  and capture recovery. Distinguish silence from a broken device or lost samples.
- [ ] Preserve `LastingInput`/`LastingOutput` recovery. Flush or age out stale media
  after a long device outage; do not replay the backlog when headphones reconnect.
- [ ] Keep automatic sample-rate conversion until a replacement demonstrably improves
  quality/cost. Avoid accidental double resampling between Windows and app-side drift control.
- [ ] Consider audio-thread scheduling hints only if underruns warrant them. Keep
  priority changes bounded and reversible; never elevate every worker indiscriminately.

**Check:** short sound, idle restart, headphone unplug/replug, default-device switch,
stop during wait, and a busy CPU/GPU. Verify no loud burst or multi-second stale replay.

### W2. Preserve loopback isolation — A/B

- [ ] Keep process-tree exclusion on Windows and existing per-process/client filters
  on Linux. An optimization must not reintroduce call audio into the shared stream.
- [ ] Document Windows' existing inability to separately exclude other Discord
  processes in this capture path. Any application-specific sharing feature requires
  a supported capture mechanism and explicit behavior, not a promise of Linux parity.
- [ ] Preserve loopback packet lifetimes and aligned/unaligned handling. Avoid work
  that blocks Windows' borrowed capture buffer for an entire mixer/encoder cycle.

### M1. Improve microphone quality without stacking expensive DSP — B

- [ ] Audit the complete chain: echo cancellation, high-pass filtering, optional
  dereverb, denoiser, gain control, leveling, gate, user gain, and output. Avoid
  accidental duplicate gain/noise processing and clipping between stages.
- [ ] Replace a fixed echo-delay assumption only with meaningful capture/render
  timing or queue estimates. Check the reverse reference represents audio actually
  played at the correct level, including relevant shared/soundboard output paths.
- [ ] Test double-talk and device-delay changes; stronger suppression is not always
  better speech quality. Keep voice-only processing away from shared music.
- [ ] Preserve gate pre-roll/hold and push-to-talk release behavior. Focus on first
  syllables, soft speech, final consonants, keyboard transients, and noise-only input.
- [ ] Move expensive DSP reconfiguration out of the time-critical block where feasible;
  reuse initialized state while avoiding two large denoisers retained indefinitely.
- [ ] Keep 10 ms processing and 20 ms packet boundaries consistent when changing
  buffering. Reuse existing PCM pools rather than allocating blocks per callback.
- [ ] Handle processing errors visibly but cheaply; avoid repeated per-block warnings
  or silently treating persistent DSP failure as successful capture.

**Check:** existing recording/voice tests plus a short microphone sample with speech,
noise, double-talk, and mute/PTT transitions. A new subjective audio lab is not required.

## 11. Worker reports, UI updates, and lifecycle

Owner: **worker/UI integration**, coordinated with the capture owner.

- [ ] Replace the unbounded `Worker::start` report channel with bounded/coalesced
  delivery for replaceable status updates. Keep errors and terminal events reliable;
  a full statistics queue must not hide worker death.
- [ ] Do not block the worker's stdout reader behind a stalled UI until the child
  blocks on its pipe. Separate state snapshots from lifecycle events or define an
  equivalent bounded draining strategy. Bound thumbnail/report payload size too.
- [ ] Push statistics at a modest cadence, publish only changed UI state, and avoid
  cloning large status payloads or rebuilding UI models every capture tick.
- [ ] Make stream/capture/encoder/receiver generations explicit where asynchronous
  work can outlive a source, codec, or session. Cancel stale uploads/retries promptly.
- [ ] Verify stop releases threads, surfaces, audio clients, channels, and pool memory.
  Join only where it cannot deadlock with a producer waiting on the stopping consumer.
- [ ] Add clear fallback diagnostics: selected codec/backend, target/actual bitrate,
  resolution/fps, queue/drop reason, and device/codec failure. No credentials or media
  contents; no continuously growing diagnostic collections.
- [ ] Keep quality controls honest: the existing higher-quality option affects x264,
  not GPU tuning. Only broaden it after a real backend-specific behavior is implemented.
- [ ] Prefer a few understandable profiles over exposing every FFmpeg/Opus knob.
  Preserve current defaults until the corresponding change has evidence and a fallback.

## 12. Larger experiments — separate, optional deliverables

These must not block allocation, scheduling, or audio correctness fixes.

- [ ] **Windows GPU capture-to-NVENC:** investigate capture textures and supported
  native/GPU interop before replacing the portable path. Identify adapter ownership,
  texture formats, synchronization, resize/device-loss handling, cursor composition,
  and multi-GPU laptops. Retain CPU capture for unsupported windows and drivers.
- [ ] **GPU conversion/scaling:** avoid CPU readback only when downstream encode and
  preview can consume the surfaces efficiently. Copying back for preview may erase
  the win; cap preview demand independently. No HDR/10-bit claims without a full path.
- [ ] **Hardware decode/render interop:** start with one Windows NVIDIA path only
  if receive-side CPU cost warrants it. Budget surfaces and threads; keep software
  fallback and correct color handling for other codecs/devices.
- [ ] **Intel Windows QSV:** separate capability/build/packaging work after current
  paths stabilize. Include runtime discovery, codec support, dependency size, and
  behavior without Intel drivers. Not part of the user's available hardware validation.
- [ ] **Linux GPU surface paths:** investigate DMA-BUF/VA-API interoperability later,
  with a Linux test owner. Do not claim Windows success proves Linux surface correctness.
- [ ] **Advanced codecs/features:** software AV1, HDR, 10-bit, 4:4:4, simulcast, and
  new codecs require negotiated support, resource budgets, receiver compatibility,
  and a concrete use case. Keep them research items rather than immediate defaults.

## 13. Parallel subagent assignments and integration boundaries

Use this layout when subagent execution is available. Separate branches/worktrees
are preferred for implementation. Read-only agents can inspect overlapping files;
writers must have explicit ownership. Do not have agents independently rewrite
shared connection types and then rely on an automatic merge to reconcile semantics.

| Assignment | Owns | Can begin independently | Integration dependency |
| --- | --- | --- | --- |
| Capture/preview | C1–C4, `stream_worker.rs` capture/preview sections | Buffer reuse, deadline tests, visibility design | Coordinate owned-frame interface with encoder; worker/UI edits merged by one owner |
| Encoder/NVENC | E1–E5, `encode.rs`, native encoder files | Option checks, configuration/fallback tests | E1 owned output connects to capture/sender; no concurrent FFmpeg vendor edits |
| Packet memory | T1–T2, `rtp.rs` | History reuse, codec fixture tests | Agree incremental packet API with scheduler before T1 migration |
| Transport | T3–T4, `stream_connection.rs` | Fairness design, closed/full distinction | Consumes packet API and audio-frame metadata; one writer owns this file |
| Audio codec/mixer | A1–A3, `opus.rs`, capture/mixer portions of `stream_audio.rs` | CTL errors, bounded input, clipping tests | Audio playout owner shares file: serialize patches or agree non-overlapping regions |
| Audio playout | A4–A5, player/jitter logic | Deterministic queue/clock tests, interface design | Transport owner adds metadata; extract call logic only with call-owner review |
| Windows/DSP | W1–W2 and M1, platform audio and `discord_audio.rs` | Device cancellation, DSP chain review | Shared queue/latency changes coordinated with playout; native Windows checks |
| Receive/UI | Receive/display and worker lifecycle | Decode budgets, UI demand design | Shares `stream_worker.rs`: capture owner integrates changes |

### Suggested waves

1. **Small independent fixes:** RTX buffer reuse, checked codec controls, bounded
   audio input, capture deadline skip, and tests. No dependency on a benchmark suite.
2. **Agree interfaces:** owned encoded frame, incremental packet emission, timed
   received audio frame, preview demand, and report delivery. Keep types minimal.
3. **Parallel implementation:** capture/preview, encoder, packet/scheduler, audio
   playout, and Windows/DSP in distinct worktrees or explicitly owned file regions.
4. **Integration:** one owner merges dependencies, checks timestamps/ownership and
   teardown across boundaries, runs relevant fake-server cases, then Windows checks.
5. **Optional tuning/interop:** NVENC/audio profile comparisons and GPU-surface work
   after correctness, with reversible settings and measured claims.

### Agent completion contract

Each assignment returns: files changed, existing behavior preserved, optimization
mechanism, failure/fallback behavior, focused tests run, unverified assumptions,
and integration notes. Include before/after numbers only when actually measured.
An unavailable GPU or failed test is a limitation to report, not a passed check.

Run heavy UI/native builds sequentially. Avoid simultaneous GPU performance tests,
multiple agents changing the same vendor patch, competing device-capture tests,
or agents all running the full check suite. Use one integration owner to run it.

## 14. Lean validation policy

### For every meaningful code change

- Run affected unit/regression tests and one relevant fake-server case where needed.
- Exercise the changed failure path, not just happy-path streaming.
- Check queue bounds, ownership, cancellation, and reuse for memory-path changes.
- For a visible/audible change, use one representative short sample on available hardware.
- No full benchmark matrix, quality-scoring infrastructure, or long soak by default.

### Additional checks only when the change needs them

| Change | Minimum additional evidence |
| --- | --- |
| Copy/allocation optimization | Explain removed ownership/copy step; small before/after allocation or memory sample if claiming savings |
| Preset/bitrate/quality default | Equal-rate text/motion or level-matched audio comparison, latency and resource spot check |
| Pacing/adaptation | Deterministic congestion/loss/fairness cases; brief constrained-link run |
| Clock/jitter/playout | Loss/reorder/wrap tests and a longer drift/device-transition run |
| GPU interop/hardware decode | Actual target-GPU run, surface lifetime/device-loss checks, fallback, CPU/VRAM comparison |
| Shared audio/native code | Relevant Discord tests plus Matrix build coverage when shared behavior/dependencies change |

Use `measure`, not `fast`, for resource comparisons. Follow the existing memory
metrics: Windows private working set; Linux anonymous memory plus swap where
appropriate. Include both app and worker, plus GPU memory where relevant. Keep
workload/settings fixed and avoid competing measurements. A small repeated comparison
is enough for an isolated change; report variation rather than false precision.

### Repository commands and release gates

- In a new worktree run `scripts/warm-worktree` before building.
- Run focused Rust commands in the dev shell with the `fast` profile, for example:
  `nix develop path:./nix --command cargo test --profile fast -p yapfast-core stream_audio`.
  Select affected `encode`, `stream_worker`, `rtp`, `opus`, or connection tests as needed.
- Performance builds use the documented `measure` exception, not UI-unoptimized `fast`.
- Run `scripts/test-fake-discord [FILTER]` while working; the full service suite once
  before the final commit of relevant Discord/service/fake changes.
- Run `scripts/check --fast` once before the final implementation commit; include
  `--matrix` for shared-path changes. Keep expensive builds sequential.
- Windows/NVIDIA is the primary available runtime target. Preserve compile/test
  coverage for other backends and record untested hardware honestly; do not require
  owning every GPU before landing portable correctness fixes.
- Fake Discord validates protocol shape and controlled behavior, not production
  Discord client compatibility. Any real-client validation is a separate authorized
  activity consistent with project rules, never an implicit automated test.

## 15. Decisions still to discover, not blockers for safe work

**Already answered:** Windows/NVIDIA; all four quality/latency/resource/audio goals;
Discord-first with broad audio; parallel specialist work with lean checks.

**Optional user detail:** exact GPU/driver/CPU, usual stream resolution/fps, and
whether typical content is games, code, or video. Until supplied, target 1080p30/60
text and motion, preserve 720p/software fallback, and probe AV1 capability at runtime.
Do not assume a high-end RTX card or reserve its full GPU budget for encoding.

**Implementation investigations:** supported codec combinations and effective driver
options; service constraints for candidate audio settings; real receiver compatibility;
which queues dominate delay; and whether GPU interop saves work after preview/rendering.
These require code/protocol/hardware investigation, not guesses from the user.

**No preselected outcome:** no promised universal codec, higher audio bitrate,
“zero-copy” pipeline, new dependency, or faster preset. Implement safe improvements
first, then choose larger changes on their actual trade-offs.

## References

- [Architecture](docs/architecture.md), [building](docs/building.md),
  [Go Live](docs/discord/go-live.md), [voice](docs/discord/voice.md), and
  [resource measurements](docs/measurements/README.md).
- [Discord voice connections](https://docs.discord.food/topics/voice-connections):
  recheck negotiation, session updates, RTP/RTCP, and shared audio before changing
  endpoints, payloads, or gateway events; update the fake to the same shape.
- [NVIDIA NVENC programming guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvenc-video-encoder-api-prog-guide/index.html):
  tuning guidance must match the bundled FFmpeg and installed driver capabilities.
- [AMD AMF AV1 documentation](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/wiki/AV1-Encoder).
- [Opus encoder controls](https://www.opus-codec.org/docs/opus_api-1.6/group__opus__encoderctls.html):
  newer documentation does not establish support in the currently linked libopus.

Keep behavior docs current with implementation. Put dated measurements/history in
commit messages, and report a needless copy on the touched path if it cannot safely
be fixed in the same change. This plan can be split into issues or agent assignments
without turning optional benchmarking or advanced hardware experiments into blockers.
