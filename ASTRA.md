# ASTRA — streaming branch review

**Implementation entry point:** follow [the approved six-feature PLAN](#implementation-plan--approved-features-16-only). The review below supplies evidence and prerequisite fixes; it is not an instruction to implement every optional idea.

## Scope and evidence

- **Branch:** `improve/streaming-codecs-audio`.
- **Reviewed HEAD:** `55e74718`.
- **Review base:** `28174c7227e9b5a7a7dd222378086e88a62cbdb1` (`upstream-yapfast/main` in this checkout). This is an ancestor of HEAD; the comparison contains **61 files, 25,413 insertions and 1,411 deletions**.
- Comparing with the fork's `origin/main` also includes unrelated upstream/emoji changes (4,140 files). Those are not the streaming review's scope. GitHub CLI was unauthenticated, so the actual PR number, remote target and current remote refs were not confirmed.
- This is a **static review**, not a successful build, hardware qualification or memory benchmark. Nix is unavailable in this shell; no Cargo commands, service suite or `scripts/check` were run. LSP probes did not confirm clean Rust files; `stream_worker.rs` also exceeded the probe's file-size limit.
- Parallel subsystem reviews were interrupted by a provider usage limit. Their partial observations were not treated as completed reviews; findings below were checked directly against source. Coverage prioritizes GPU resource ownership/presentation, capture switching, encoding buffers, stream audio, receive allocations and UI lifecycle. It is **not an exhaustive audit of every native codec, driver interaction or changed line**.
- Line references are to this HEAD. No application code was changed for this review.

**Priority:** P1 = fix before merging/relying on the affected path; P2 = correctness/resource issue worth fixing soon; P3 = smaller bounded retention issue. Optimization and feature sections are proposals, not claims of measured savings. “Confirmed” below means the source establishes the defect; hardware symptoms and performance magnitudes remain unmeasured unless explicitly stated otherwise.

## Executive summary

The branch makes useful improvements: bounded media queues, encoder-buffer reuse, reusable RTP plans/datagrams, in-place transport decryption, time-bounded audio queues, capture/encode device sharing, visibility controls and worker-process isolation. Keep those designs.

However, the new GPU sharing path has **real handle/resource ownership holes**, and several playback/state transitions defeat the intended behavior. The most urgent work is ownership and correctness, not another reduction in allocator calls.

| ID | Priority | Finding |
| --- | --- | --- |
| F01 | P1 | Duplicated GPU texture/fence handles have no close owner in the UI |
| F02 | P1 | GPU slots are released before successful replacement/consumer completion |
| F03 | P1 | The audio wait path receives and discards valid packets |
| F04 | P1 | Long Opus packets replay their first chunk instead of advancing |
| F05 | P1 | GPU-to-CPU source switching leaves the sink in GPU-only mode |
| F06 | P1 | Growing encoded output buffers re-encodes after consuming/discarding a frame |
| F07 | P1 | An unchanged captured desktop cannot answer a new keyframe/start request |
| F08 | P2 | Drift “skips” substitute equal-duration silence rather than catch up |
| F09 | P2 | The presentation cache retains the last stopped/refused stream's resources |
| F10 | P2 | Fence event reuse can mistake an old signal for current completion |
| F11 | P2 | Shared texture format metadata disagrees between producer and importer |
| F12 | P2 | Public safe texture/device wrappers do not enforce native lifetimes |
| F13 | P2 | Watch visibility/size caches are not keyed by watch identity |
| F14 | P2 | Hidden/minimized local previews still count as visible |
| F15 | P3 | Encoder pool growth can strand old-size idle buffers |
| E01 | P2, existing | Audio startup failure can detach an already-running capture thread |

## Findings and regression tests

### F01 — duplicated shared-resource handles leak into the UI process

**Evidence:** `crates/yapfast-core/native/d3d11enc.c:2338–2362, 2409–2415, 2451–2456, 2475–2506, 2602–2612`; `crates/yapfast-core/src/d3d11.rs:985–991`; `src/ui/gpu.rs:219–223, 350–392, 405–479`.

`duplicate_into` creates handles in the UI's handle table. Each successful shared set contains **three textures and one fence**. The native destructor releases worker-side COM references but deliberately does not close the remote handles. The Rust destructor delegates to it. The UI opens resources from the numeric handles, but does not put those handles in an owning wrapper or close them; its only `CloseHandle` is for the presentation wait event.

`OpenSharedHandle` is not a transfer that closes the input handle. Dropping imported COM/wgpu objects or exiting the worker therefore does not release the UI's kernel handles. This is distinct from F09's cache retention.

**Trigger/impact:** start/stop sharing, toggle preview visibility (which destroys/recreates `Shared`), switch sources, rebuild a capture or fail halfway through shared-resource construction. Repeated generations leave handles and backing resources alive until the UI process exits. A three-texture 1920×1080 BGRA8 set represents **23.73 MiB of texture storage**, before alignment/driver overhead; that is arithmetic, not a measured leak slope. Even handles for textures never published to the UI can leak after partial initialization.

**Fix:** define an explicit cross-process ownership protocol for the entire shared set. Register all handles, including unpublished slots, with an owner that can roll back partial construction and retire a generation after the consumer is finished. Use RAII for UI-owned handles. Do not simply close a handle after its first import while continuing to publish/reopen that same numeric value, or remotely close recycled handle values without coordination.

**Test:** repeated start/stop, preview-hide/show, resize and injected failure at each texture/fence-creation step. Track UI handle count, dedicated/shared GPU memory and worker/UI memory separately. Counts must plateau and return near the warmed baseline after stopping.

### F02 — taking a GPU frame is incorrectly treated as finishing with the old one

**Evidence:** `crates/yapfast-core/src/frames.rs:573–650, 916–921`; `src/ui/gpu.rs:52–65, 254–333`; `crates/yapfast-core/native/d3d11enc.c:2520–2574`.

`take_frame_timed` advances the shared `showing` generation **before** invoking its callback. The UI callback only extracts a `Picture`; import, fence checking and `Image` construction happen later. `Writer::claim` uses that generation advance to allow reuse of an older texture.

There is a concrete failure even without asynchronous GPU scheduling: if the new picture times out or fails to import, `Present::show` returns `None` and the old Slint image remains displayed. The producer has nevertheless been told that the reader moved past it and can overwrite its texture. On successful replacement, the UI still provides no fence acknowledging that previously submitted GPU sampling has completed. The native wait is on the **producer's last write**, not the consumer's last read.

**Impact:** tearing, changing an image still displayed, and unsynchronized cross-device resource access. Three slots and a CPU-side generation counter are not a consumer-completion protocol.

**Fix:** separate acquired, successfully presented and GPU-retired states. Retain a lease for the actually displayed texture, keep it on timeout/refusal, and only recycle after the rendering queue signals completion. If that cannot be integrated with Slint/Skia, use a UI-owned copy or the CPU path rather than claiming zero-copy safety.

**Test:** stall the UI/render queue; publish faster than the window redraws; force the next picture to time out while displaying the previous one. Assert that the old slot cannot be reclaimed. Run with D3D debug validation and a visible frame-number/checkerboard pattern.

### F03 — successful receives are thrown away while the audio player waits

**Evidence:** `crates/yapfast-core/src/stream_audio.rs:1121–1164`, especially the `Turn::Wait` arm.

The loop initially drains `sounds.try_iter()` into `Playout`. When more sound is needed, it calls `sounds.recv_timeout(...)` but only handles `Disconnected`. An `Ok(frame)` is consumed and dropped instead of being pushed into the playout queue.

**Trigger/impact:** normal sparse arrival during startup, rebuffering or a gap. When the queue is empty and each next packet arrives during the wait, every packet can be discarded and priming may never happen. Other schedules create self-inflicted loss that is not network loss. Discarding during the separate deafened branch is intentional; discarding during `Turn::Wait` is not.

**Fix:** explicitly match `Ok(frame) => playout.push(frame, Instant::now())`, timeout and disconnect in the active wait path.

**Test:** exercise the **outer player loop**, with one packet arriving during each blocking wait and no prefilled channel. Assert receipt-to-playback conservation and bounded time to first sound. Existing tests that directly call `Playout::push` cannot expose this integration bug.

### F04 — 40–120 ms Opus packets repeat the beginning of their PCM buffer

**Evidence:** `crates/yapfast-core/src/stream_audio.rs:774–779, 973–988, 1142–1152`; `Turn` at `428–435`.

`Playout::turn` advances `self.left.start`, but returns only a sample count. The output loop always takes `&mut playout.pcm[..end]`, starting at zero. A 60 ms packet therefore produces first-chunk / first-chunk / first-chunk instead of its three consecutive 20 ms chunks. With non-unit volume, that same prefix is scaled repeatedly as well.

**Fix:** return a sample range/offset with `Turn::Play`, or provide a borrowed exact slice through a suitable API. Scale and write each chunk once. Do not introduce a new allocation or full-buffer shift per turn.

**Test:** decode a 60 or 120 ms packet with distinct contents in successive 20 ms segments; compare all emitted PCM samples and test non-unit volume. A small state model run during this review reproduced `A/A/A` for expected `A/B/C`; it was not a Rust/audio integration test.

### F05 — switching from GPU capture to a window/camera freezes outgoing video

**Evidence:** `crates/yapfast-core/src/stream_worker.rs:815–822, 1940–2001, 2634, 2733–2773`; encoder reinitialization at `1022–1127, 1180–1185`.

`capture_textures` sets `sink.footage = Footage::OnTheCard`. It returns that same sink via `After::Processor` when switching to a window/camera or falling back from a display that cannot reopen on the GPU. The CPU path changes `sink.capture` and starts `Capture`, but never resets `footage`. Every CPU frame runs the preview and then returns early because footage is not `InMemory`.

**Impact:** the local preview can appear to switch successfully while the remote picture freezes. In the non-display switch branch the old texture encoder is also not explicitly cleared; simply resetting the enum would be an incomplete fix when the dimensions/codec stay unchanged and `ready` reuses it.

**Fix:** make the transition explicit: drop the old texture encoder before the capture device goes away, set `InMemory`, reopen an appropriate CPU-fed encoder, and emit a keyframe. Reconcile offered codecs/fallback behavior without unnecessarily ending the transport or audio.

**Test:** GPU display → window; GPU display → camera; GPU display → display whose GPU open fails. Include equal dimensions and the same negotiated codec, and verify frames at the remote decoder, not merely the preview.

### F06 — output-buffer growth is not a safe retry of a stateful encoder

**Evidence:** `crates/yapfast-core/src/encode.rs:461–496`; `crates/yapfast-core/src/d3d11.rs:766–808`; `native/d3d11enc.c:1238–1266`; `native/hwenc.c:629–631`; `native/h264.c:96–101`; `native/vp8.c:75–84` (native paths under `crates/yapfast-core/`).

The Rust retry loop grows the pool after `TooSmall`/`-2` and encodes the same input again at the same timestamp. But the native calls have **already encoded** the input before they discover that the caller's output buffer is too small. NVENC increments its frame counter and unlocks/discards that bitstream; FFmpeg unreferences its packet; x264/libvpx have likewise advanced their encoder state.

**Trigger/impact:** a later inter frame exceeds the pool's current capacity (initially up to 512 KiB). The retry can reference the discarded frame, which the receiver never got. This is both a dependency-chain correctness problem and avoidable extra encode work, potentially several times for repeated doubling. A forced-IDR retry may avoid that particular dependency, but does not make the general inter-frame retry correct.

**Fix:** separate encode/produce from copy/drain. Preserve the produced access unit and report its required length, grow the caller buffer once, then copy the **same** bitstream without resubmitting the input. Respect each codec's output-lifetime rules; a bounded staging buffer is safer than discarding codec state. If retention is impossible, explicitly recover with a new keyframe rather than transparently retrying as an inter frame.

**Test:** force tiny output buffers so growth occurs after a successful keyframe, then decode every emitted access unit. Assert one input submission per encoded picture, consistent timestamps, and no missing reference. Pool-allocation tests alone do not validate this.

### F07 — static-desktop capture suppresses required recovery pictures

**Evidence:** `crates/yapfast-core/src/stream_worker.rs:2673–2717, 1125–1127, 2865–2901`; `crates/yapfast-core/src/dup.rs:95–99, 329`; `native/dup.cc:1108`.

The GPU loop calls `sink.textured` only for `Arrival::Frame`; `Arrival::Still` does nothing. Desktop Duplication returns `Still` when the desktop does not change. Keyframe requests are consumed in `ready`, reached through encoding a frame, not through the idle path.

**Trigger/impact:** after the initial picture, keep the desktop and pointer still, then join as a viewer or request recovery after packet loss. There is no guaranteed new keyframe until the screen changes. Similarly, connection readiness or a newly visible preview can arrive after the last capture update, and then wait for another update unnecessarily.

**Fix:** distinguish “capture changed” from “transport/presentation needs a picture.” Retain a valid last texture under an explicit lifetime policy and re-encode it for initial connection, codec changes and throttled keyframe requests; refresh previews/stills on demand. This should not restore unconditional capture/encode at full FPS for an unchanged desktop.

**Test:** use Desktop Duplication or a deterministic capture fake that emits one frame then only `Still`; connect/request a keyframe/show preview afterward. Require recovery within a bounded interval without moving the mouse.

### F08 — the audio drift correction does not shorten playback time

**Evidence:** `crates/yapfast-core/src/stream_audio.rs:839–851, 1000–1029, 1142–1152`.

When `skips(samples)` chooses a packet to skip, `play` zeroes its PCM and returns a normal-duration `Played::Silence` turn. The output still writes those samples and is paced for the same duration. One packet was consumed and one packet-duration was played, exactly as without the “skip.”

**Impact:** a faster sender is not caught up with by this mechanism; it instead gets periodic silent holes and eventually hits other queue trimming/reset behavior. This does not create an unbounded queue—the other bounds remain—but it invalidates the intended clock-drift control.

**Fix:** discard the selected decoded interval without spending an output interval on it, then select the next playable data, or implement bounded rate correction/resampling. Keep the work per output turn bounded and avoid obvious clicks.

**Test:** drive deterministic sender/device clocks with a slight positive mismatch. Assert stable held duration, actual input/output sample accounting and no periodic zero-filled replacement frames. Checking only the `skipped` counter is insufficient.

### F09 — stopped/refused streams remain in the thread-local GPU cache

**Evidence:** `src/ui/gpu.rs:106–110, 202–246, 254–289, 340–345`; shutdown path `src/ui/shell.rs:414–430`.

`Present.streams` owns imported textures and a fence. Dead entries are pruned only during a subsequent successful-adapter `Present::show` call. Stopping the final GPU stream, closing its window to the tray, or falling back permanently to pixels need not make such a call. Refusal also does not evict the existing entry.

**Impact:** last-use GPU objects survive long after they are useful, potentially until the UI thread exits. This is **bounded stale retention**, not the unbounded remote-handle leak in F01; fixing one does not fix the other. The four-texture cap does not ensure timely teardown.

**Fix:** explicitly retire a stream's presentation state on stop, refusal and window detach, with rendering-completion safety. Clear obsolete UI image properties too. Do not make cleanup depend on another stream producing a frame.

**Test:** start exactly one stream, stop it, remain idle/in tray and inspect resource counts without starting a replacement stream.

### F10 — an old fence notification can satisfy a newer wait

**Evidence:** `src/ui/gpu.rs:203–209, 375–392, 488–503`.

All streams reuse one auto-reset event. `SetEventOnCompletion` registers a future notification; a `WaitForSingleObject` timeout does not retract that registration. `ResetEvent` clears current state, not future notifications. If fence A times out, its later signal can wake the wait for fence B (or a later value on A). `wait` returns true solely from the event wake, without re-reading the requested fence value.

The initial `GetCompletedValue() >= value` also treats `UINT64_MAX` as success, although D3D12 documents it as device removal.

**Fix:** use the fence value as the source of truth after every wake, reject the device-removed sentinel, and keep waiting only within the original deadline. Consider per-stream outstanding waits or nonblocking fence polling so abandoned registrations and UI-thread waits are controlled. Combine this with F02, not instead of it.

**Test:** delay A past timeout, register a wait on B, then signal only A. B must not be accepted. Inject the device-removed sentinel and require fallback/error handling.

Microsoft references:
- [SetEventOnCompletion](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12fence-seteventoncompletion)
- [GetCompletedValue](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12fence-getcompletedvalue)
- [WaitForSingleObject](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)

### F11 — the imported texture's declared format is not the resource's format

**Evidence:** `crates/yapfast-core/native/d3d11enc.c:2370–2385`; `src/ui/gpu.rs:395–459`; supporting renderer code `vendor/i-slint-renderer-skia/wgpu_30_surface/dx12.rs:109–154`.

The producer creates `DXGI_FORMAT_B8G8R8A8_UNORM`. The importer documents an RGBA8 safety requirement and passes `wgpu::TextureFormat::Rgba8Unorm` to both `texture_from_raw` and `TextureDescriptor`. That metadata is false. The declared usage also deserves validation against the actual resource bind/usage flags, rather than assuming compatibility.

**Impact/qualification:** the raw-resource import contract is inconsistent. The current Skia importer consults `resource.GetDesc().Format`, so this review does **not** claim that every preview currently swaps red/blue. That downstream workaround does not make the upstream wgpu declaration correct.

**Fix:** validate actual description (format, size, samples, mip levels and supported usage) and import with matching metadata, or change the producer consistently. Prefer a shared descriptor/protocol rather than parallel assumptions.

**Test:** assert producer/importer metadata agreement, render a red/blue test pattern and run D3D/wgpu validation on the real path.

### F12 — safe Rust APIs can pass invalid/dangling native pointers

**Evidence:** `crates/yapfast-core/src/d3d11.rs:296–321, 486 onward, 668–705, 708–739, 766–797, 978–982, 1007–1017`; native use in `d3d11enc.c:2523–2541`.

`Texture` is public, `Copy`, and has public raw-pointer fields and a safe constructor. Safe methods such as `scale`, `readback`, `encode_texture` and `Shared::draw` pass it to native code that dereferences COM vtables. Returned textures are borrowed from a source/scaler/shared set but carry no lifetime, so safe callers can retain one after its owner is destroyed or its scaler replaces the resource. `open_on` similarly accepts a device pointer without a Rust-level lifetime contract.

**Qualification:** this is API unsoundness, not evidence that every current production caller is dangling. Comments asserting that the caller keeps a resource alive do not enforce that condition for a safe public API.

**Fix:** use typed borrowed/owned COM resources and lifetime-bound frame/texture leases. If a raw-pointer boundary is necessary, make it private or `unsafe` with a precise contract and ensure safe constructors cannot forge it. `AddRef`/owned COM wrappers can retain resources without a per-frame heap allocation; account for their pool-retention implications.

**Test:** compile-fail tests for a texture escaping its owner, plus source/scaler replacement tests. Do not execute a deliberately invalid pointer as a normal regression test.

### F13 — UI watch hints can be lost when replacing the watched stream

**Evidence:** `src/ui/callbacks.rs:2359–2413`; `crates/yapfast-core/src/discord_voice.rs:197–209, 229–278, 299–346`.

The UI caches the last visibility boolean and tile size per window, but not the watch identity. A new `Watch::start` resets to `shown: true` and `size: (0, 0)`. If stream A is replaced by B between ticks while the tile size/visibility stays the same, the UI does not resend those values. `Watch::open` replays only B's defaults; it preserves hints across a worker replacement within one watch, not across a new watch.

**Impact:** the replacement can keep the default presentation size, and can perform presentation work while hidden if the cached hidden state never transitions again. The lost-size case also occurs across stop/start if no tick resets the cached size.

**Fix:** key the cached hints by a stable watch/session identity and send an initial snapshot whenever it changes, or retain/apply desired presentation state above the watch lifecycle. Ensure zero/no-tile transitions do not leave stale size caches.

**Test:** A → B in an unchanged-size tile without an intermediate idle tick; repeat while hidden and while reopening the worker.

### F14 — local preview visibility omits window visibility/minimization

**Evidence:** `src/ui/callbacks.rs:201–208, 2375–2394`; `ui/yapfast.slint:779–786, 1615`.

The watched-stream path combines `watching` with `on_screen(&window)`. The local preview uses only `get_share_preview_shown()`, whose Slint expression tests layout/call state, not whether the window is minimized or hidden. Its timer runs while sharing. Minimizing a window with the share tile selected can leave preview production enabled even though no picture is visible.

**Fix:** combine local preview demand with actual window visibility, explicitly send false on detach, and restore it on return. Keep Discord's occasional still-image demand independent. Also consider whether watched-stream presentation should use actual tile/PiP visibility rather than just window visibility (see O06).

**Test:** minimize, restore and close to tray during a share. Local preview counters should stop while audio/video transmission and scheduled server previews continue.

### F15 — pool growth can retain idle buffers it will never reuse

**Evidence:** `crates/yapfast-discord/src/stream_connection.rs:235–251, 291–320`.

`Pool::grow` changes capacity but leaves `idle` untouched. Checkout pops one entry from this LIFO stack, discarding it if stale. Once a new-size buffer is returned, serial checkouts can cycle that top buffer forever, leaving old-size entries underneath. `Encoded::drop` correctly rejects old-size buffers still in flight, but does not clean these already-idle ones.

**Impact:** bounded, avoidable retention after a large frame. For three idle 512 KiB buffers growing to 1 MiB followed by serial use, two old buffers can remain stranded: **1 MiB**. A state model reproduced this ordering; actual workload concurrency determines whether it persists.

**Fix:** discard stale idle entries when growing under the pool mutex, or consistently filter them when returning/checking out buffers. Preserve the existing small retained-buffer cap.

**Test:** prefill three buffers, grow once, then cycle a single checked-out buffer; assert no idle entry has the old capacity and track live bytes, not only allocation count.

### E01 — existing startup failure leaks a running audio-capture lifetime

**Evidence:** `crates/yapfast-core/src/stream_audio.rs:282–295, 337–343, 351–357`; the same ordering exists in the review base.

`Capture::start` starts `listen(...)` before constructing the Opus encoder and before spawning the mixer. If either later step fails, the function returns without setting `stop` or joining the capture thread. Dropping a `JoinHandle` detaches it; no `Capture` was constructed to run its destructor. The listener can keep OS capture resources and its shared input buffers alive for the worker's remaining lifetime.

**Classification:** pre-existing on a changed hot path, not a regression introduced by this PR.

**Fix/test:** construct fallible codec state first where possible and put already-started resources behind a rollback guard. Inject encoder/thread-spawn failure and require all started threads/resources to stop before returning the error.

## Further allocation and performance opportunities

These are follow-ups to profile, not merge-blocking claims of measured improvement.

### O01 — separate preview scaling from an actual NVENC encoder session

`stream_worker.rs:2807–2849` opens a `D3d11Encoder` for the preview solely to scale/share/read back pictures. `Shared::draw` ultimately calls the scaler and copies a texture; it does not encode a preview bitstream.

Extract a D3D device/context scaler/readback component so a visible preview does not require another codec session, output pool and codec-specific resources. Preserve the already-fixed separation between stream-size and preview-size scaler destinations; merging those into one single-size cache would reintroduce per-frame texture churn. Measure VRAM, session count, startup time and texture creations before/after.

### O02 — optimize the receive path, not just packetization

`stream_connection.rs:1562–1602` constructs `Held` before codec/SSRC admission, and `rtp.rs:1630–1669` owns copied packet payloads. `Assembler::push` transfers its vector with `mem::take` (`rtp.rs:982–1004`), leaving the next frame to rebuild capacity. Audio's `Cow` becomes owned when queued.

First reject unsupported payload types/SSRCs before allocating. Then investigate a borrowed in-order fast path and a bounded owned-buffer pool for packets actually reordered, plus a small frame pool returned after decoding. Bounds must cover bytes as well as item counts, including damaged/oversized input. Keep the current ownership across async queues; replacing necessary ownership with dangling borrows is not an optimization.

**Measure:** receive allocations/second, allocated bytes/second, retained bytes after a large frame and CPU at 1080p/4K under normal traffic, loss, RTX and unsupported traffic. Parts of this are pre-existing opportunities.

### O03 — account for all retained memory, including deliberately bounded caches

The encoded pool keeps up to three buffers, but the send queue is four frames deep and can allocate beyond those three. RTX `History` has 2,048 reusable packet slots (`rtp.rs:1476–1553`); expiration makes packets unavailable for resend but does not release slot capacities. Reorder queues, AV1 scratch, codec surfaces, native contexts, mapped frames, imported textures and driver resources are separate costs.

Do not call these bounded pools leaks. Establish a per-stream budget and report current/high-water bytes per category. Prefer selective high-water trimming after a resolution reduction, a long idle period or a session transition, not `shrink_to_fit` on every frame. A smaller steady-state allocation rate is not automatically a smaller working set.

### O04 — quantify the per-frame mmap/locking trade-off

`Frames::take_frame_timed` (`frames.rs:573–605`) now opens a read mapping for each take so the writer can safely shrink the backing file. Retaining a permanent mapping would undo that safety improvement.

Profile mapping/unmapping and lock/syscall overhead at 30/60 FPS with multiple video sources. If material, consider versioned segments or a resize-generation handshake that allows a stable mapping until an actual resize, with explicit retirement. Count mappings, page faults and committed anonymous/shared memory; do not infer their cost solely from Rust allocation counters.

### O05 — broaden memory tests beyond `GlobalAlloc`

`crates/yapfast-discord/tests/allocations.rs:1–18, 30–91` intentionally measures Rust allocation calls/bytes for packet plans and pools. It does not measure current live bytes, peak live bytes, native allocations, GPU allocations, handle counts, cross-process memory or the actual encoder path.

Add targeted core integration tests for encode-growth continuity, native-resource construction/destruction counters, UI handle/resource ownership and whole sender/receiver paths. Extend the allocator counter with live/peak bytes and carefully controlled serial measurement windows. Keep tests warmed and isolate fixture loading, logging and unrelated test threads from the counted span.

### O06 — drive presentation demand from actual consumers

`src/ui/callbacks.rs:2387–2413, 3037–3066` already computes whether/where a watched tile is shown, but `WatchShown` only checks whether a window is visible. Use tile/PiP/fullscreen demand, with a stable identity and required pixel size, as one coherent state update. Avoid constructing `Focus::Stream(watching.to_owned())` on every 33 ms tick where a borrowed ID would do.

The string allocation is small; unnecessary frame conversion/copying is the larger target. Keep decoding reference pictures and audio where required even when presentation is suppressed. Coalesce layout/resize updates rather than remaking GPU surfaces for every pixel of a drag.

### O07 — move fence waiting off the critical UI path

`src/ui/gpu.rs:183–196, 488–503` allows up to 8 ms of CPU wait per late picture. Multiple streams can consume a noticeable part of the UI frame budget. After F02/F10, prefer nonblocking readiness checks and a bounded recheck mechanism, or proper GPU queue synchronization. Keep the previous valid image without busy-spinning; measure input latency and missed window frames, not just stream FPS.

### O08 — reduce callback-time growth and contention only where measured

`stream_audio.rs:120–143, 213–267` now bounds each application's sample backlog, which is good. Initial `VecDeque` growth and the shared `Inputs` mutex still occur on capture/mixing paths. Profile allocation and lock hold time with multiple active applications. Consider modest initial reserve or fixed-capacity per-source queues, but avoid preallocating a maximum backlog for every idle enumerated application. Any change must preserve finite-value sanitization and source cleanup.

### O09 — make retained-resource keys generation-aware

`src/ui/gpu.rs:350–371` keys imported textures only by numeric handle. Once F01 introduces correct closing, Windows may reuse handle values. Include an explicit stream/resource-set generation and validate format/size, rather than assuming handle integers identify objects forever. Apply the same rule to the cached fence. This is a design requirement for the ownership fix, not proof of a current handle-reuse failure while handles are leaked.

## Implementation PLAN — approved features 1–6 only

This replaces the feature wishlist. The selected `34` means **3 and 4**. Feature 7 (a standalone lifecycle/chaos harness) is **excluded**. Add focused regression tests to existing test modules; do not build a new testing framework. The F/E findings above remain the defect backlog; O01–O09 remain optional optimizations, not permission to expand this plan.

### Rules for the implementing LLM

1. Read the relevant F finding and current function bodies before editing. Line numbers above describe the reviewed commit; locate symbols if lines moved.
2. Implement **one step at a time**. Follow this order: **prerequisites → 1 → 5 → 4 → 3 → 2 → 6**. Preserve the feature numbers below.
3. Fix the affected correctness prerequisites before extending that path: audio **F03/F04/F08**; encoding **F05/F06**; shared GPU resources **F01/F02/F09/F10/F11/F12**; presentation demand **F13/F14**; pool cleanup **F15**. Feature 5 includes the F07 fix. Use each finding's specified regression test.
4. New GPU integrations target **Windows first**. Preserve existing Linux behavior and build guards. Do not rewrite unrelated capture, UI or transport systems.
5. Reuse existing reports, bounded queues, codec backends and settings. No per-frame diagnostic strings, unbounded history, busy polling, or extra full-frame CPU copies in the working zero-copy path.
6. A prerequisite that cannot be implemented or tested is a **reported blocker**, not a completed task. Never describe CPU fallback as successful GPU support, packet-arrival time as display time, or allocation count as live memory.
7. For each step: implement → run focused tests → update current-behavior docs → report changed files, actual test results and remaining blockers. Do not claim measurements/tests that were not run. Use the build/test rules below.

### 1 — End-to-end resource diagnostics

**Files:** `crates/yapfast-core/src/{voice.rs,stream_worker.rs,frames.rs,d3d11.rs,decode.rs}`, `crates/yapfast-discord/src/stream_connection.rs`, `src/ui/{call_link.rs,gpu.rs}`, `ui/voice.slint`.

**Implement:**
1. Extend `StreamStats` and the worker report path with a typed snapshot: capture backend, codec/encoder/decoder, capture/encode/presentation adapter LUIDs, CPU/shared-texture presentation, fallback reason, preview demand, queue delay, pool live/peak bytes and app-owned GPU-resource/handle counts. Represent unavailable values as unknown, not zero.
2. Collect counts at allocation/acquisition/release sites. Keep per-frame updates numeric and allocation-free. Publish at most once per second through a bounded, replaceable stats report; include watched streams, not only senders. Never make terminal/control reports replaceable.
3. Merge UI-owned presentation counters with worker counters under the same stream/session identity. Distinguish app-owned resources, OS process memory and driver GPU memory; do not add overlapping quantities together.
4. Extend the existing Stream tab and add a copy-diagnostics action that serializes only the latest snapshot. Exclude tokens, server credentials, process/window titles, media bytes and unbounded logs. Clear the snapshot on stream termination.

**Done when:** send/watch and CPU/GPU paths report their actual modes; absent data says unknown; stop/fallback updates counts; repeated snapshots do not grow retained memory; export redaction is tested. No new dashboard/history subsystem.

### 2 — End-to-end A/V timing and bounded synchronization

**Files:** `crates/yapfast-core/src/{stream_audio.rs,stream_worker.rs,frames.rs,audio_io.rs,audio_io/lasting.rs,audio_io/wasapi.rs,audio_io/pulse.rs}`, `crates/yapfast-discord/src/stream_connection.rs`, `src/ui/{gpu.rs,callbacks.rs}`.

**Prerequisites:** audio fixes and the renderer-completion/texture-retirement protocol from F02. Do not substitute a UI timer callback for renderer completion.

**Implement in two gated parts:**
1. **Timing:** propagate each decoded video's RTP timestamp through CPU and GPU frame publication. Record the last frame actually submitted/presented using the renderer's completion information; label submission-only timing as estimated. Expose the audio output's consumed-sample position and queued/device latency through `LastingOutput`, using backend facilities where supported. Unsupported/recovering devices return unknown.
2. Map audio and video positions onto the existing sender-report timeline. Keep separate fields for arrival skew and presentation skew. Reset mappings on SSRC/session/device changes. Mark presentation skew unknown if either mapping is missing or older than five seconds; account for timestamp wrap. Export timing confidence and mapping age through feature 1.
3. **Synchronization, only with valid timing:** use audible audio position as master. In a bounded presentation scheduler, drop already-decoded video pictures more than 80 ms late and defer early pictures by at most 80 ms. Never drop undecoded reference packets or sleep the UI/network thread. Retain at most one pending picture in addition to the displayed picture, with valid resource leases.
4. If the clocks are unavailable, the pending slot is full or the 80 ms hold budget is exhausted, keep bounded best-effort playback and report unsynchronized/estimated status. Do not grow latency or memory to chase synchronization. Reset scheduled presentation on stop, seek, deafen and clock discontinuity.

**Done when:** tests distinguish received from presented timestamps; PCM ordering stays correct; late pictures drop without breaking decode; early pictures use a bounded timer; absent/stale clocks disable correction; pending resources retire on reset. Validate lip-sync on real hardware. **80 ms is the initial correction budget, not a proven end-to-end guarantee.** Do not mark hardware timing validated from unit tests alone.

### 3 — Intel/AMD hardware encoding and explicit adapter selection

**Files:** `crates/yapfast-core/native/hwenc.c`, `crates/yapfast-core/src/{encode.rs,d3d11.rs,gpu.rs,stream_worker.rs}`, core `build.rs`/`Cargo.toml`, and the FFmpeg dependency/build configuration located from those files.

**Implement:**
1. Inventory the encoders enabled in the actual shipped FFmpeg build. **AMF already appears in `hwenc.c`; extend/verify it instead of creating a duplicate backend.** Add Intel QSV through FFmpeg and its required oneVPL/build support. Pin new dependencies and preserve platform guards. If dependencies cannot be built, report that backend blocked; runtime fallback is not completion.
2. Probe each backend/adapter by opening the requested codec with the required input format, dimensions and rate. Advertise only codecs that open successfully. Cache capability results per device/session, not per frame; invalidate on device loss.
3. Keep the native D3D11 zero-copy NVENC path. Use existing FFmpeg hardware paths for AMD/Intel first. Identify CPU-fed hardware encoding honestly; do not claim zero-copy for it or remove NVENC's vendor guard and pass another vendor's device into NVENC.
4. Apply explicit adapter preferences to the backend's actual device selection. Automatic mode prefers a working same-adapter capture/encode path; otherwise use a supported fallback and report the copy/fallback reason. Import a shared preview only when its LUID matches the UI's device. No implicit cross-adapter shared-texture import.
5. On backend failure, reopen a compatible encoder, preserve codec negotiation, force a recovery keyframe and report the fallback. If no hardware path succeeds, preserve the existing software path.

**Done when:** a capability/selection test covers supported/unsupported codecs and adapter mismatch; actual Intel QSV and AMD AMF output is decoded successfully on available hardware; NVENC and software paths still pass. Record missing hardware tests as outstanding. No direct QSV/AMF zero-copy backend rewrite in this step.

### 4 — GPU capture of the selected window

**Files:** `crates/yapfast-core/native/wgc.cc`, `crates/yapfast-core/src/{wgc.rs,capture.rs,stream_worker.rs}`; source-selection UI only if the existing ID cannot be carried safely.

**Implement:**
1. Add an explicit window target to the WGC native/Rust boundary using `CreateForWindow`; keep monitor targeting distinct. Resolve `Source::Window` to the selected live window and validate it. Never reinterpret its ID as a monitor or silently select another target.
2. Extend `on_the_card`/`card_capture` to route windows through WGC. Keep Desktop Duplication display-only. Reuse capture-device sharing, encoder/scaler logic and the fixed resource-lease protocol.
3. On resize, retire old resources safely, update dimensions and force a keyframe. While minimized or temporarily without a valid picture, pause picture production without spinning or showing another source. On close or capture exclusion, invalidate cached pictures and stop that share; do not keep refreshing stale protected content.
4. If WGC cannot open the selected window, use the existing CPU **window** capture path for the same target and report why. Apply F05's explicit GPU/CPU encoder transition. Never fall back to desktop capture for a window request.

**Done when:** remote output matches the selected window; resize/minimize/restore/close and exclusion behavior is checked; fallback preserves target identity; repeated switches do not grow handles/GPU memory. Existing display and camera sharing still work.

### 5 — Content-aware idle sharing

**Files:** `crates/yapfast-core/src/stream_worker.rs` (`capture_textures`, `Sink::ready`, preview scheduling), `crates/yapfast-discord/src/stream_connection.rs` (existing keyframe demand), capture texture ownership wrappers as required.

**Implement:**
1. Retain **one valid last captured picture** with an owned/lifetime-safe resource reference; never retain a pointer to a released capture frame. Invalidate it on completed source switch, device loss, close or exclusion.
2. Handle control/transport demand on every existing paced loop iteration, including `Arrival::Still`. Separate `source_changed`, `send_picture_needed`, `local_preview_needed` and `server_still_needed`.
3. Encode unchanged content only for initial connection readiness, a throttled keyframe request or codec/encoder restart. Keep the existing keyframe-request throttle and send-queue backpressure. Do not consume a pending request until a frame can be produced/queued; coalesce repeated requests.
4. Restore a local preview or refresh Discord's scheduled still from the retained picture without requiring source motion and without unnecessarily encoding network video. With no demand, do no encode/preview work; do not add a polling thread or raise the capture rate.
5. Report unchanged/idle separately from capture failure using feature 1. A missing or invalid cached picture must wait for valid capture, not refresh the previous source.

**Done when:** a source emitting one valid frame then only `Still` can serve a later connection, keyframe request and preview restoration. Demand is serviced at the next eligible paced iteration after throttling/backpressure, not after mouse movement. No-demand idle creates no new encoded frames or growing buffers.

### 6 — Memory-pressure policy

**Files:** a small platform sampler/policy module under `crates/yapfast-core/src/`, `stream_worker.rs`, `frames.rs`, `src/ui/{gpu.rs,callbacks.rs}`, `crates/yapfast-discord/src/stream_connection.rs` (pool maintenance).

**Prerequisites:** resource retirement fixes, F15 and feature 1 counters. Do not attempt to hide a leak by lowering resolution.

**Implement:**
1. Sample OS available physical memory and, where available, the active adapter's GPU budget once per second. On Windows use OS/DXGI budget APIs, not Rust heap size as a proxy. Missing GPU data stays unknown; unsupported platforms keep normal policy until a supported sampler exists.
2. Put policy in a pure, testable state machine with named constants. Initial policy: enter constrained mode when available system memory or GPU-budget headroom is below 10%; leave only after all available signals exceed 20% continuously for ten seconds. Treat invalid/zero budgets as unknown. These are initial tunable policy values, not measured optimal thresholds.
3. On entry, cap local preview dimensions to a 640×360 aspect-preserving box without upscaling; release inactive presentation caches after consumer completion; discard obsolete/idle oversized buffers at safe boundaries. Do not resize/free in-flight buffers or trim on every frame. Preserve the pool's minimum working set; perform maintenance once per transition, not once per sample.
4. Keep network resolution/bitrate, audio and decoder reference state unchanged. Restore normal preview sizing lazily on exit and expose constrained mode/reason through feature 1. Reset sampler/policy ownership on session shutdown; no detached monitor thread.

**Done when:** injected readings prove entry/exit hysteresis and unknown-data handling; a live pressure/recovery run reduces optional retained resources without playback corruption or handle growth; outgoing stream settings remain unchanged. Measure actual savings; do not infer them from the 640×360 cap.

## Documentation cleanup

- `docs/measurements/README.md:208–263` still describes GPU presentation as an unshipped spike and hardware decode as not built/gated, alongside code and `docs/discord/go-live.md` describing newer implemented paths. Separate historical measurements from current feature support; do not rewrite old experiment results as new benchmarks.
- `docs/discord/go-live.md:193–207` claims that a displayed picture is never overwritten. That claim needs the consumer-retirement protocol in F02, not merely triple buffering.
- `d3d11.rs:855–885` and comments around `native/d3d11enc.c:2300–2331` still describe two shared pictures while the code allocates three. Synchronize slot-count and ownership comments with the implementation.
- Keep the existing distinction between a whole-worker NVENC-vs-x264 comparison and a like-for-like capture-path comparison (`docs/measurements/README.md:299 onward`). Neither is a measured heap saving for this review.

## Validation and measurement plan

### First: deterministic correctness regressions

1. Fix F03/F04/F08 and test the **full player loop and emitted PCM**, not only queue counters.
2. Fix F05/F07 with fake capture state transitions and verify remote decoding.
3. Fix F06 with real codec encode/decode tests and artificially small output buffers.
4. Design F01/F02/F09/F10 together; test generation retirement, timeout/failure, partial construction and final-stream teardown.
5. Test watch-identity replacement and visibility transitions (F13/F14), then pool growth/live-byte accounting (F15).

When the prescribed environment is available, run only the affected tests while iterating, for example:

```sh
nix develop path:./nix --command cargo test --profile fast -p yapfast-core stream_audio
nix develop path:./nix --command cargo test --profile fast -p yapfast-core stream_worker
nix develop path:./nix --command cargo test --profile fast -p yapfast-core frames
nix develop path:./nix --command cargo test --profile fast -p yapfast-core encode
nix develop path:./nix --command cargo test --profile fast -p yapfast-discord --test allocations -- --test-threads=1
```

These commands are suggested follow-up commands, **not commands run/passed in this review**. Hardware-gated tests need the appropriate Windows/NVIDIA environment. Follow project policy for `scripts/test-fake-discord`, one final `scripts/check --fast`, and `--matrix` when fixing code shared with Matrix. Do not compile multiple UI configurations concurrently.

### Then: measure steady-state and retirement, separately

Use the project's **`measure` profile** for performance work. Record hardware, GPU/driver, OS, capture method, codec, resolution, FPS, renderer, tile size and warm-up. Alternate baseline/fixed runs and report spread.

| Scenario | Main observations |
| --- | --- |
| Idle before/after 50 share sessions | UI/worker private memory, handles, GPU memory, surviving threads |
| 720p, 1080p, 4K; 30/60 FPS where supported | allocation rate, current/peak live bytes, worker CPU, encode latency |
| GPU preview visible/hidden/minimized | preview count, sessions/textures, UI frame time, GPU/CPU memory |
| Repeated source switches, resize and fallback | resource generations, handle slope, remote frame continuity |
| Static desktop + late viewer/loss | time to recovery picture, idle CPU |
| 20/40/60/120 ms Opus with loss/reorder | exact PCM chunk order, played samples, startup, queue delay |
| Sustained slight audio clock mismatch | held-time trend, correction rate, discontinuities |
| Encoder output grows after warm-up | decode continuity, encode submission count, retained pool bytes |
| GPU timeout/device removal/partial init failure | correct fallback, no stale presentation, resource cleanup |
| Receiver under large frames, loss and RTX | packet/frame allocation rate, bounded retained memory |

Measure native/driver resources separately from Rust heap. Use Windows private working set and dedicated/shared GPU memory, or Linux anonymous memory plus swap as described in `docs/measurements/README.md`; do not present reclaimable RSS changes as allocator savings.

**Review-time checks actually performed:** source/base comparisons, targeted call-chain inspection, Microsoft fence API documentation lookup, and small illustrative state models for long-packet slicing and stale pool entries. The models reproduced the described state behavior; they are not substitutes for the Rust, codec, fake-server or GPU tests above.
