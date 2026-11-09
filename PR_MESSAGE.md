# Streaming on the graphics card: capture, encode, decode and presentation, with the sound kept with its picture

105 commits (101 ordinary, 4 merges of `yapfast/main`) against `yapfast/main`. Of the diff's 246 files, 170 and about 83,000 lines are the vendored FFmpeg (`vendor/ffmpeg-src`), which now carries the codecs Discord sends and the hardware encoders and decoders this branch uses; the repository's own files are 76 files, roughly +35,000 / −2,300.

## Summary

- **Codecs:** Discord's AV1, H265 and H264 streams, offered by what the card actually encodes and withdrawn with op 14 when one fails mid-stream; keyframe demand and resend history tightened.
- **GPU encode:** NVENC reads a captured texture where it lies (Windows), with AMD's AMF, Intel's Quick Sync and Linux's VA-API beside it, each reporting the format, colour and clock it took; x264 is the fallback.
- **GPU decode:** FFmpeg's `d3d11va` on Windows, one card decoder since Media Foundation's was removed; the picture is scaled to the tile in a video processor pass, so only the tile comes back. Software decoding remains the fallback everywhere.
- **Capture:** Windows Graphics Capture for displays and windows, Desktop Duplication where that can't be had, HDR tonemapped on the way to I420, an idle desktop served from its last picture.
- **Presentation:** pictures drawn from a shared texture the window samples without a copy, the window telling the worker slot by slot and generation by generation when it has finished with one.
- **Audio:** sequence-ordered playout with forward error correction and concealment, bounded by held time; sound and picture placed on the streamer's sender-report timeline, pictures more than 80 ms late dropped.
- **Diagnostics:** the Stream tab and *Copy diagnostics* — what captured, encoded, decoded and showed the stream, the two A/V skews, pool bytes, pressure — with "unknown" for anything nothing answered, rather than zero.
- **Memory:** a pressure policy samples system memory and the driver's GPU budget once a second and caps the local preview and the encoder's idle buffers, without touching what a viewer sees.
- **Linux foundations:** a DMA-BUF descriptor channel between the worker and the window, a VA-API decoder, portal/PipeWire and X11 DRI3 capturers, and an opt-in Skia Vulkan build with OpenGL the default and fallback. These are built and tested; the worker does not call the capture and decode modules yet (see the limitations).

## What changed

**The frame ring.** `frames.rs` is a memory-mapped triple buffer between two processes, carrying either pixels or a picture the card drew. Each slot carries a generation and a set number, the reader records what it took and what it has finished with, and the writer draws only into the slot both agree is his. A picture is never written over while it is on the screen or while the renderer may still be sampling it; a slot held too long (a stopped render queue) makes the share fall back to pixels, which is the escape from a ring that has stopped turning.

**Encoding.** `encode.rs` opens each codec's encoder once to prove the driver encodes it and remembers what opened; the settings are sorted into what the stream needs and what only improves it, and a refused optional setting is reported rather than failing the open. The native shims (`hwenc.c`) hand out a produced access unit and copy it into a caller-grown buffer, so a frame is never re-encoded at the same timestamp after the codec has already advanced. `native/d3d11enc.c` is the Windows zero-copy path: the capture's BGRA texture and the tile's share come from the same D3D11 device, so no frame is converted at all.

**Decoding.** `decode.rs` decodes on the card through FFmpeg's `d3d11va` where the build and the driver have it, keeps the picture in a texture and hands it to the tile pass; `native/hwdec.c` drains the decoder until it takes a frame, then keeps only the newest picture. Everything else — VP9, streams the card refuses, 10-bit content the tile pass has no curve for — decodes in software and is shown the same way.

**Capture.** `stream_worker.rs` shares a screen, a window or a camera, keeps the capture on the card where it can (Graphics Capture, Duplication), and on the processor's capturer elsewhere. It handles the screen's resize, a window's close and capture exclusion, a device the card took away and a source switch, each with the state that says whether the capture's texture still holds a picture. A still desktop is answered from that picture, at the next paced pass, for a viewer joining, a keyframe request or a preview coming back.

**Presentation.** A picture crosses to the window as a shared texture opened once, with a fence saying when the drawing finished; the window keeps a lease per picture it took and retires the slot only when its own queue has drawn past it. Where the adapters can't share, or the window draws in software, the tile is read back into one staging texture instead.

**Audio.** The stream's sound is sequenced, concealed with Opus's own forward error correction, bounded by held time; a share's sound is mixed from other applications with Yapfast's own left out. The player exposes what the device has played and what it still holds, so lip sync is measured against the sound rather than assumed.

**Diagnostics.** `StreamStats` is a typed snapshot — capture backend, encoder, decoder, adapters, how the pictures reach the window, queue delay, pool bytes, pressure, and for a watched stream the two skews and the pictures dropped as late. It is published once a second through a bounded, replaceable report, is redacted for export, and says "unknown" where nothing answered.

**Linux foundations.** `dmabuf.rs` sends a picture from the worker to the window as a `SOCK_SEQPACKET` message with its descriptors in `SCM_RIGHTS` (slot, generation, size, fourcc, modifier, up to four planes of offset and stride), closed exactly once on each side. `native/hwdec_vaapi.c` decodes through VA-API and maps the picture to a DMA-BUF rather than copying it back. `capture/portal.rs` + `capture/pipewire.rs` are the xdg-desktop-portal screencast handshake and a DMA-BUF-only PipeWire stream; `capture/x11.rs` is XComposite + DRI3 for X11 without a portal. `src/ui/vulkan.rs` builds a wgpu Vulkan device and selects the Skia Vulkan renderer where the build has it, falling back to OpenGL; Linux's Skia is built from source with `skia_use_vulkan` (`nix/skia-linux-vulkan.nix`) because the prebuilt archive is GL only.

## Fixes

Closed in this branch, from the review that motivated it and from testing:

- GPU handles duplicated into the UI process leaked: each was closed as its resource was opened, every handle of a set is accounted for, partial construction rolls back, and a retired set is released after the consumer is done.
- A picture taken but not shown held its slot, so one late picture could end the card's picture path: such a picture is retired at once now.
- A fence notification from an old picture could satisfy a newer wait: the fence value is re-read after every wake, the device-removed sentinel is refused, and the wait keeps its original deadline.
- The window imported a stream's texture as RGBA while the producer drew BGRA: the import reads the resource's own description, and Slint's vendored importer accepts BGRA (this was the bug that stopped any shared picture being shown).
- A stopped or refused stream kept its opened textures until the UI process ended: an explicit retirement on stop, refusal and window detach.
- Accepted receives were discarded while the audio player waited, a 40–120 ms Opus packet replayed its first chunk, and the drift correction played silence instead of catching up: all three fixed and tested through the player loop and the emitted PCM.
- Switching from GPU capture to a window or camera left the sink in GPU-only mode: the texture encoder is dropped, the footage set to memory, a CPU-fed encoder opened and a keyframe forced, including when the size and codec are unchanged.
- Growing the encoder's output buffer re-encoded a frame the codec had already consumed: encode and copy are separate now, and the caller-grown buffer copies the same bitstream.
- An unchanged desktop couldn't answer a new viewer or a keyframe request; the capture's last picture serves them at the next pass.
- A pool that grew stranded idle buffers of the old size; growth trims them under the pool's mutex.
- A startup failure in audio capture could detach an already-running listener; a rollback guard stops and joins what it started.
- Swapping one watched stream for another could keep the previous stream's size and visibility hints, and a minimised window kept producing a preview: hints are keyed by a stream's identity and a detaching window says `Preview(false)`.
- The Windows build could not compile: two helpers had been deleted as dead code from a Linux-only build, a `cfg` named a variant libwebrtc's Windows capturer lacks, and an import was missing.
- Quick Sync was refused outright; it is opened through FFmpeg and oneVPL now.
- A second `EAGAIN` on a retried decoder send ended the card's path; the decoder is drained until it takes the frame.
- The Media Foundation decoder's half of the shared code was a second backend with no measured win; it is gone, one FFmpeg decoder remains.
- A test that failed once in four gate runs was not a race in the worker: the test installed a thread-scoped logger while other tests in the binary install a process-wide one, and the report thread only inherited the former by luck. It reads the sentence the warning writes now, deterministically.

## Tests and verification

Run on the Linux host: `scripts/check --fast` (formatting, doc links, Clippy with `-D warnings`, the workspace's tests, the build and the smoke run — the test and smoke steps need `libstdc++` on the loader path in this sandbox), `cargo test --workspace` (680 in `yapfast-core`, 356 in `yapfast-discord`, 288 in the app), `scripts/test-fake-discord`, and a Windows cross-check (`cargo xwin check --target x86_64-pc-windows-msvc`, which catches the Rust half of the Windows-only code; that SDK has no Graphics Capture header, so the WGC-compiled paths are not covered by it).

Deliberately unverified, and stated as such in `docs/discord/go-live.md`: nothing ran on a graphics card (d3d11va, the tile pass, the shared texture, NVENC from a texture, AMF and Quick Sync output, the D3D11 fences); nothing ran on Intel hardware; lip sync is not measured; window transitions need a real window on Windows; the Linux card paths are built but not wired into the worker; Vulkan was not run; and the receive-side allocation and mmap costs are not measured.

## Known limitations / follow-ups

- **Linux is not on the card yet.** The DMA-BUF channel, VA-API decoder and portal/PipeWire/X11 DRI3 capturers exist with tests and fallbacks, but `stream_worker.rs` still captures and decodes on the processor; wiring them is the next change, along with the vendored FFmpeg's `--enable-libdrm` and VA-API hwaccels that the decoder needs.
- Window transitions on Windows need a machine with a window the test owns.
- Nothing retries the card path after a stuck ring makes a share give it up (deliberate: the thing that would have to come back is what stopped), and the processor's capture doesn't time a frame's wait before encoding, so that row reads unknown; the receive side keeps no pool counters.
- The review's optional optimisations are not started and not measured: a scaler apart from the encoder session, receive-path allocations, retained-memory accounting, the per-frame map cost, presentation demand from the tile rather than the window, and fence waiting off the UI thread.

## Reviewer notes

Start with `frames.rs` and `src/ui/gpu.rs`: the generation and lease protocol is where the real bugs were, and it crosses a process boundary. Then `decode.rs` (the one card decoder and its fallback), `native/hwenc.c` (needed versus optional settings, and the four backends), `av_timing.rs` (skew mapping, staleness, wrap) and `dmabuf.rs` (fd ownership across processes). `stream_worker.rs` is the largest change and holds the paced loop everything hangs from. The untestable-in-CI parts are the native COM/GPU lifetimes and every hardware path.
