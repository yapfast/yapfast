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

## The camera's encoders, card against processor

A camera filmed in memory encodes with x264, not with the card's encoder, even
with "Encode streams with the GPU" on: at a call's camera size the upload eats
what the card saves, and the picture is worse.

| encoder | kbit/s | VMAF | SSIM | ms/frame |
| --- | --- | --- | --- | --- |
| x264 | 1496 | 70.47 | 0.5733 | 1.51 |
| h264_nvenc | 1565 | 70.48 | 0.5409 | 1.22 |

1280×720 at 30 frames a second and 1.5 Mbit/s, `Effort::Light`, 60 frames of the
test's own moving scene, on an RTX 4090. The two are level on VMAF, and SSIM is
about 6% behind — a softer picture for the same judged quality — while the card
is only 0.3 ms a frame faster, since every picture is uploaded to it first. So
frames that come from memory stay on x264. A camera filmed on the card
(`cfg(dmabufenc)`, with the switch on) still encodes on NVENC, which reads the
descriptor where it lies.

`the_camera_s_encoders` in `crates/yapfast-core/src/encode.rs` is where these
came from, ignored because it needs the machine's own encoders and FFmpeg's
`libvmaf` and `ssim` filters. Run it outside the dev shell: the shell's library
path has no `libcuda.so.1`, so the GPU engine only opens outside it, and inside
the shell the test measures x264 alone and says so.

```sh
cargo test --profile measure -p yapfast-core -- --ignored --nocapture the_camera_s_encoders
```

It writes the source's pictures and each engine's stream to
`target/tmp/camera-encoders/`, leaves them there to be looked at, and prints the
table above.

## GPU capture and encode, on Windows

What ships now is the path these numbers argued for: on Windows, Windows
Graphics Capture, or Desktop Duplication where Graphics Capture can't be had,
fills a D3D11 texture, and where the card opens its encoder on the capture's own
device, NVENC reads that texture where it lies, so no pixel of the frame is
copied by the processor. Where it can't, the libyuv conversion comes back.
[Go Live](../discord/go-live.md) says how a worker chooses between them. What
follows is what the spikes measured on the way here, and then the same path as
the crate's own test measures it; neither is a benchmark of the shipped build.

The spike's result is now reproduced **inside the crate**: a D3D11 texture goes
into NVENC through `native/d3d11enc.c`, and a test that runs only where an
NVIDIA GPU is present encodes thirty frames of each codec from that texture.
That run reports 0.16 ms of CPU a frame for H.264 and H.265 and 0.47 for AV1 —
figures to read against the same 15.6 ms clock, since thirty frames is two
ticks. The encoder can also open on a device a capture already
has (`yapfast_d3d11enc_new_on`, `Capture::open_on`), so a stream costs one D3D11
device rather than two: an ignored test captures with Windows Graphics Capture
and encodes that very 4520×1920 texture on the capture's own device — 250,984
bytes, keyframe first, and not one pixel of the frame copied by the processor.


These numbers came from `.local/spike/wgc-nvenc/`, an experiment that was never
committed: machine RTX 4090 (driver 617.14), Windows build 26200, a 3440×1440
monitor in HDR, an Intel UHD 770 also present.

A real desktop captured by Windows Graphics Capture into a D3D11 texture and
handed to NVENC (`NV_ENC_DEVICE_TYPE_DIRECTX`) encodes with no CPU pixel copy:
`Map`, `Unmap`, `UpdateSubresource`, `CopyResource` all zero over 300 frames, one
`nvEncMapInputResource` before the loop. A control run with a deliberate CPU copy
per frame fires the counter exactly 300 times and costs 5.36 ms of CPU a frame
against 0.42 without it, so the counter is live rather than silent.

| leg | CPU ms/frame | spread |
| --- | --- | --- |
| D3D11 texture → NVENC, 1080p | 0.34 | 0.10–0.57, 10 runs |
| WGC monitor, 3440×1440 | 0.82 | 0.63–1.20, 3 runs |
| WGC window, 1922×1112 | 0.64 | 0.37–0.94, 3 runs |
| Desktop Duplication, 3440×1440 | 1.07 | 0.78–1.35, 2 runs |
| CPU copy of a 3440×1440 BGRA frame, as a control | 4.38 | one run |
| FFmpeg `h264_nvenc`, CPU BGRA in, 1080p | 1.51 | 3 runs |
| FFmpeg `h264_nvenc`, yuv420p planes in, 1080p | 0.38 | 3 runs |

`GetProcessTimes` ticks at 15.625 ms, so 300 frames at 0.3 ms is six ticks: a
sub-millisecond figure carries about 0.05 ms of quantisation, and most of the
spread above is that rather than the encoder. The FFmpeg legs drive `ffmpeg.exe`
over a pipe — no prebuilt libavcodec was available to link — and include the
spike's own frame generation, so they are upper bounds. An earlier spike
reported 0.365 ms and an 11.09 ms FFmpeg baseline; neither survives a rerun, and
its logs did not contain them.

So the change is worth the conversion and the upload that stop happening — a
full-frame CPU copy at 3440×1440 costs about 4.4 ms, and at 1080p the whole
saving is a couple of milliseconds a frame. Real, and far smaller than the
"twenty times" first reported.

What the platform said, which matters more than the percentages:

- The yellow capture border is a screen-space overlay that **never enters the
  captured texture**; sampling at the outline found no outline. It can be turned
  off anyway with `IGraphicsCaptureSession3::IsBorderRequired` after
  `RequestAccessAsync(Borderless)` answers `Allowed`, with no manifest or consent
  needed on this build. Windows 10 has no such interface, so there the border
  cannot be removed.
- `WDA_EXCLUDEFROMCAPTURE` is honoured by all three methods: WGC monitor capture
  and Desktop Duplication show the background where an excluded window was, and
  WGC *window* capture of an excluded window fails closed with no frame.
  Desktop Duplication's first `AcquireNextFrame` is all black with
  `AccumulatedFrames` at zero, which reads as "blacked out" until it is retried.
- **HDR breaks Desktop Duplication**: it hands back `R16G16B16A16_FLOAT` scRGB
  with a rotation, and registering that as ARGB runs clean but produces invalid
  video, so it needs a GPU conversion pass. WGC hands back BGRA8, already
  tonemapped, and encodes correctly.
- **Withdrawn: a WGC picture of an HDR desktop is *not* the desktop's SDR
  composition.** The comparison below took two captures of the same screen and
  found them 1.95 levels apart, which was read as "Windows maps HDR for WGC". A
  bright SDR window on this desktop, seen through a share, is over-bright and its
  colour is off by the transfer curve — so both captures carried the same HDR
  codes, and comparing them could not tell. What the numbers below do say is that
  the two *pipelines here* agree; what they don't say is that either is SDR.
  Tone-mapping an HDR screen therefore still has to happen, on the processor's
  capture today and on the card next. The original note follows, with that
  correction.
  The same frame taken twice — once through `native/wgc.cc`'s Graphics Capture,
  read out as the stream's still (`512×212`, the pipeline's own conversion and
  scale), and once through a GDI screen copy of the 3440×1440 screen — differs by
  **1.95 levels of Rec.709 luma on average** (99th percentile 33.5, from motion
  between the two captures: the mean per cell is 0.0–4.2 everywhere but the
  scrolling terminal, at 10.7). Nothing is systematically shifted: means 27.0
  against 27.1, medians 17.3 and 17.7, 99th percentiles 201.1 and 201.0, pixels
  at or above 250 0.48 % against 0.41 %. So a share of an HDR screen carries what
  an SDR screen would show it as, and a curve over it maps SDR a second time —
  which is what the processor's capturer used to do.
  What an SDR composition loses is everything above SDR white, and this desktop
  had nothing there while it was measured (0.5 % of pixels clipped), so how much
  a tone map of the card's own FP16 picture would recover is **not measured**;
  that needs HDR content — a game or an HDR video — on the captured screen.

### What a source picker finds, on this machine

Same machine, Windows 11 25H2 build 26200, from the spike in
`.local/spike/capture-enum/` (also not committed):

- **Both displays are driven by the RTX 4090**; the Intel UHD 770 drives none.
  So capture here is born on the discrete adapter and NVENC needs no
  cross-adapter copy — the zero-copy result above is not being quietly undone
  by the hybrid setup. Two outputs: `\.\DISPLAY1` 3440×1440 HDR10 and
  `\.\DISPLAY2` 1080×1920 rotated, sRGB.
- **The card is enumerated twice** — two LUIDs for the same 4090 — so a picker
  must key rows on LUID rather than on the description, which is identical.
  The Basic Render Driver also appears; it must never be chosen.
- **398 top-level windows, 12 shirable** by the usual filter (visible, titled,
  not `WS_EX_TOOLWINDOW`, not cloaked).
- **`WDA_EXCLUDEFROMCAPTURE` arrived at the export boundary**: excluding a
  *foreign* window fails with `ERROR_ACCESS_DENIED` — only the owning process
  may set or clear it. Reading it, via `GetWindowDisplayAffinity`, works for
  any window, and caught another process's excluded window on an early run.
- A window can exclude itself and then clear it with `WDA_NONE`; a picker
  therefore has to re-read affinity during a share, not only when the list is
  built.

### A decoded picture straight to the screen, without the processor

This one ships. `src/ui/gpu.rs` shows a stream's own D3D12 texture through the
window's device, and [Go Live](../discord/go-live.md) describes how the worker
and the window hand pictures to each other now — shared handles, one fence per
stream, and slots the window hands back only after its rendering queue has drawn
a frame without them. It began as a spike in `.local/spike/gpu-present/`, never
committed, and everything below is that spike's own record of what it measured
then, not a description of the current code and not a fresh benchmark.

**Both hops work on this machine.** A D3D11 texture created with
`SHARED_NTHANDLE | SHARED_KEYEDMUTEX` (either flag alone fails) crosses a process
boundary by `CreateSharedHandle` + `DuplicateHandle` and opens as both a D3D11
and a D3D12 resource, on the same adapter only — a wrong adapter is
`E_INVALIDARG`. A `wgpu::Texture` made from it draws through the Skia renderer via
`slint::Image::try_from`. The vendored Slint compiles with the two
`unstable-wgpu-30` features in 75 s, no patch.

**The device does not need a rendering notifier.** `BackendSelector::
require_wgpu_30(WGPUConfiguration::Manual{..})` hands back a device we created,
so the texture import never touches `set_rendering_notifier`.

**And the notifier would have been free anyway**: `Surface::use_partial_rendering()`
is `false` for every surface but the software one, so on the GPU renderer there
is no partial-rendering state for a notifier to switch off. Measured at
3440×1440 with an image animating at 30 fps: 17.29 % CPU without a notifier
(16.87–18.75) against 13.23 % with one (11.77–15.94) — the notifier measured
*cheaper*, which is what "partial repainting was never active here" predicts.
Idle was 0.00 % either way.

**What it bought**: the UI's copy-on-write of one 1280×720 RGBA picture cost
**1.1–1.9 ms a frame**, 3–6 % CPU at 30 fps per stream. At a 480×270 tile it is a
fifth of that.

**What it cost**: the keyed mutex does not survive the D3D12 hop
(`IDXGIKeyedMutex` on a D3D12 resource is `E_NOINTERFACE`), proved by a D3D11
reader without the mutex seeing zeros where a D3D12 reader saw the frame — so the
shipped path uses the worker's fence instead. The producer also becomes
adapter-bound, and failures only appear at `OpenSharedHandle`, so the worker's
LUID decides before anything is opened, and a mismatch is a decision rather than
a failure to retry. Note the card enumerates twice here
(`0x1488A`, `0x2079F`), so LUIDs must be compared, not names.

### Assessed, measured, and what came of it

Section 12 was first assessed as "not buildable here"; most of it has since been
measured and shipped instead, and the numbers are in the sections above:

- **Windows GPU capture-to-NVENC works**, with no CPU pixel copy: a real desktop
  through Windows Graphics Capture into a D3D11 texture into NVENC. It was worth
  the conversion and upload that stop happening — a couple of milliseconds a
  frame at 1080p, and about 4.4 ms of CPU copy at 3440×1440 — not the twenty
  times first rumoured. **Windows Graphics Capture, borderless, with the
  libwebrtc capturer as fallback**, was the decision, and it is what ships,
  windows as well as displays: a chosen window is captured with Graphics
  Capture's `CreateForWindow`, Desktop Duplication stays display-only, and a
  window the card can't take goes through the processor's capturer for that same
  window. Windows 10 still needs Desktop Duplication, which cannot remove the
  border and breaks on HDR frames.
- **Hardware decode** was expected to wait for an FFmpeg with `d3d11va`, which
  the vendored one has neither hwaccel nor a single parser for. It shipped
  another way: `native/d3d11dec.cc` decodes H264, HEVC and AV1 with FFmpeg's
  Direct3D 11 decoder on a Direct3D 11 device, leaves each picture in a texture,
  and scales it to the tile in the video processor's pass, so only the tile is
  ordered back to the processor. A stream the machine has no decoder for, a
  codec the card's pass can't take, and a session the card gives up on all
  decode in software, as before. No measurement of the card's decoder
  is recorded here.
- **Intel QSV** is built now, for H.264, HEVC and AV1 encode, on Linux and
  Windows: `update.sh` pins oneVPL 2.14.0 and configures FFmpeg with
  `--enable-libvpl`, and `src/vpl.c` loads the dispatcher when an encoder first
  runs, so the binary still starts on a machine without it. Nothing of it is
  measured: this machine has no Intel card, so whether a UHD 770's or an Arc's
  encoder runs here, and how fast, is untested; AV1 encode also needs a driver
  that has it. QSV's decoders are not in the build — pictures keep coming from
  the software decoders and Direct3D 11's — so the AV1 decode this bullet
  expected is still not there.
- GPU conversion and the Linux surface paths gain nothing further on their own.

### Encoder preset, colour claim, and Opus complexity

Three questions that looked like they needed a human. They were measured instead,
in `.local/spike/codec-evidence/` (not committed), on this machine's RTX 4090.

**NVENC preset.** 1080p 30 fps at 6000 kbit/s, 300 frames, every setting held
except the preset, three runs each:

| preset | CPU ms/frame | wall ms/frame | PSNR-Y | SSIM-Y |
| --- | --- | --- | --- | --- |
| p1 | 0.218 | 1.52 | 46.168 | 0.994986 |
| p2 | 0.229 | 1.53 | 46.181 | 0.995012 |
| **p4** | 0.296 | 2.42 | **46.657** | **0.995414** |
| p6 | 0.299 | 3.31 | 46.666 | 0.995466 |
| p7 | 0.280 | 3.30 | 46.666 | 0.995466 |

**`p4` stays.** p6 and p7 produce byte-identical bitstreams and buy 0.009 dB for
37 % more wall time; p2 is 37 % cheaper in wall for half a dB. CPU per frame could
not rank presets at all — two sessions inverted the order, because synchronous
NVENC spins while the GPU works — so wall time is the cost to read.

**And no card knob buys a sharper picture.** "Higher quality" was carried to the
card as p7 for a while, so the other levers were measured the same way before it
there: AV1 NVENC at a fixed bitrate, 480 frames of 1920x1080 at 60 fps against a
lossless (`ffv1`) reference, once of `testsrc2` and once of this desktop panning
through a window — screen content's text and UI edges. PSNR-Y and SSIM-Y read the
whole frame.

| lever, screen content at 6000 kbit/s | wall s | kbit/s | PSNR-Y | SSIM-Y |
| --- | --- | --- | --- | --- |
| shipped row: p4, vbr, no AQ | **1.22** | 5991 | **56.692** | **0.99825** |
| p7 | 1.78 | 5974 | 56.869 | 0.99832 |
| `refs 4` | 1.25 | 5992 | 56.775 | 0.99828 |
| `spatial-aq 1 -aq-strength 8` | 1.32 | 6127 | 55.004 | 0.99797 |
| `spatial-aq 15` | 1.32 | 6209 | 53.283 | 0.99764 |
| `temporal-aq` | 1.35 | 6140 | 55.548 | 0.99804 |
| `multipass fullres` | 1.91 | 5305 | 54.616 | 0.99781 |

Nothing there is worth a setting. p7 is **0.18 dB and 0.00007 of SSIM for 46 %
more wall time**; at 2000 kbit/s the same rows are 53.277 dB for the shipped row
and 53.240 for p7, so the lever doesn't buy more where the picture is starved
either. Adaptive quantization is *worse* than nothing on screen content at every
bitrate tried (at 2000: 52.123 against 53.277, and it overspends the cap by 9 %),
while on `testsrc2` it trades 0.20 dB of PSNR for 0.0003 of SSIM. Four reference
frames is within noise (+0.08 dB at 6000, +0.04 at 2000). `multipass` costs half
again the time and underspends the cap by 12 %, which puts its picture lower, not
higher.

So the card's picture is its bitrate and its codec, and the box that asks for a
sharper one stays the processor's: the card's encoder has one tuning.

**At this desktop's own size, 3440x1440 at 60 fps**, the same levers against the
same desktop panning through a 3440x1440 window, at the 20805 kbit/s ceiling
Settings' experimental switch declares for it:

| lever | wall s | kbit/s | PSNR-Y | SSIM-Y |
| --- | --- | --- | --- | --- |
| shipped row: p4, vbr | **0.82** | 16485 | 58.974 | 0.99893 |
| p7 | 1.14 | **14427** | 58.660 | 0.99886 |
| `multipass fullres` | 1.18 | **13545** | 57.329 | 0.99883 |
| `refs 4` | 0.81 | 16529 | 58.976 | 0.99893 |
| `spatial-aq 4` | 0.83 | 20608 | 59.761 | 0.99915 |
| `spatial-aq 8` | 0.83 | 20744 | 59.431 | 0.99919 |
| `rc cbr` | 0.81 | 19371 | **61.627** | **0.99928** |

The numbers that matter are the third column. Every lever that "won" on quality
spent more of the ceiling — `cbr` 17 % more, `spatial-aq` the lot — and at equal
spend none of them wins: `cbr` held to 16500 spends 15359 and measures 58.637,
less than the shipped row's 58.974 for 16485, and held to 14200 on a still
desktop it spends 13209 for 58.428 against 58.823. p7 and `multipass` **underspend**
by 12 % and 18 % and measure lower in both bitrate and picture, which is the
worst of both: "Higher quality" was sending fewer bits than the box beside it.

What `cbr` does buy is the ceiling spent: on a **still** desktop at 20805 it
spends 18561 of 20805 where vbr spends 14193, and measures 62.149 against
58.823 — a picture a third better fed by bits nobody looks at, which is why the
shipped row leaves a calm screen below the cap. And a share sending 2 Mbit/s
under a 20.8 Mbit ceiling is a screen that isn't changing: this still desktop
already spends 14 Mbit/s of it on text and edges at 58.8 dB.

**Capped quality is not the answer either, though it looks like one.** NVENC's
target quality (`-rc vbr -cq N`) spends far fewer bits for a small loss — a still
3440x1440 desktop at **2782 kbit/s for 55.327 dB**, where vbr spends 14193 for
58.823, and the panning one at 8068 for 57.719 against vbr's 16485 for 58.974 —
and on easy content it stays under a ceiling (7814 against 9000 at `-cq 20`).
On hard content it does not: **25810 kbit/s under a 20805 ceiling at `-cq 15`**,
and **12377 under 9000 at `-cq 20`**. Target quality overrides the maxrate rather
than being bounded by it, and a share can't use a mode that breaks the ceiling it
declares: that ceiling is what Discord and the uplink are promised. Enforcing it
would be Yapfast's own work — watching what the encoder spends and taking the
cap down as it goes — not the encoder's.

**Sharpening: no, not at these scales.** The decoded picture is 1.3 % softer in
edge energy than the source at 3440x1440@60 and 1.6 % at 1920x1080@60 — so what a
sharpening pass would have to add is a *hundredth*, and nothing in FFmpeg's
filters goes that small. `cas` at its gentlest (`0.02`) lifts edge energy to
**1.155** of the source with PSNR-Y **49.992** where the shipped row is 58.974;
`0.05`, `0.1` and `0.2` are 1.155, 1.158 and 1.167; `unsharp 0.2` is 1.147 at
48.408 and `0.4` 1.225 at 42.993. At 1920x1080@60 and 6000 kbit/s `cas 0.05` is
1.140 at 51.923 against 56.692. Each also costs a pass: 6.79 to 7.31–7.55 ms a
frame at 1440p, 2.55 to 2.74–3.11 at 1080p. Every step overshoots into halos and
spends bits doing it. A 1 % sharpening would have to be our own kernel — the card's
video processor or a CUDA one — tuned against a reference, not a filter's
smallest setting.

**The colour claim.** The question assumed BT.709; `hwenc.c` and `d3d11enc.c`
declare **SMPTE 170M (BT.601) limited**, and that is what the pixels are:
measured mean absolute error 0.33/0.33/0.41 codes through NVENC's own BGRA→NV12
and 0.60/0.54/0.68 through libyuv. Reading them as BT.709 instead would put green
out by up to 41 codes. Declaration and pixels agree, so there is nothing to fix;
whether BT.601 is the *right* matrix to choose is a separate question that only
a viewing comparison can answer.

**Opus complexity.** 10 is reachable and costs nothing measurable: 4.56 ms per
second of audio against 4.85 at complexity 9, well inside the spread. On the
content measured it changed **no packets at all** — the bitstreams were
byte-identical — so there is no case for moving the default off 9.

### A real share, card against processor

The whole worker — capture, on-card scale, preview, shared sound and RTP — sending
to the fake Discord for 20 s at 1920×802 and 30 fps with a preview shown, CPU per
frame from `GetProcessTimes` over the frames the fake counted, four runs:

| path | encoder | CPU ms/frame | frames |
| --- | --- | --- | --- |
| card | AV1 NVENC from a D3D11 texture | 3.82 / 2.86 / 2.46 / 3.10 (mean **3.06**) | 597–602 |
| processor | x264 H.264 | 14.56 / 11.10 / 12.01 / 15.91 (mean **13.40**) | 601–602 |

**Read this with care**: the two rows differ in *two* things, because the setting
turns hardware encoding off as well as capture — so the card row is AV1 on NVENC
and the processor row is H.264 on x264. It is the cost of a share as a user
experiences the two settings, not the cost of the capture path alone. A
like-for-like comparison (FFmpeg's NVENC fed by CPU frames against NVENC fed by a
captured texture) has not been run.

These are also whole-worker figures and must not be compared with the
encoder-only numbers above.

