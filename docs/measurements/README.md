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

## GPU capture and encode, on Windows

The spike's result is now reproduced **inside the crate**: a D3D11 texture goes
into NVENC through `native/d3d11enc.c`, and a test that runs only where an
NVIDIA GPU is present encodes thirty frames of each codec from that texture.
That run reports 0.16 ms of CPU a frame for H.264 and H.265 and 0.47 for AV1 —
figures to read against the same 15.6 ms clock, since thirty frames is two
ticks.


An experiment, not shipped behaviour: the spike is in `spike/wgc-nvenc/` in the
working tree and is not committed. Machine: RTX 4090 (driver 617.14), Windows
build 26200, a 3440×1440 monitor in HDR, an Intel UHD 770 also present.

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

### What a source picker finds, on this machine

Same machine, Windows 11 25H2 build 26200, from the spike in
`spike/capture-enum/` (also not committed):

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

Whether a decoded stream can reach Skia's GPU renderer without being copied to
CPU memory first — spike in `spike/gpu-present/`, not committed, not shipped.

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

**What it would buy**: the UI's copy-on-write of one 1280×720 RGBA picture costs
**1.1–1.9 ms a frame**, 3–6 % CPU at 30 fps per stream. At a 480×270 tile it is a
fifth of that.

**What it would cost**: the keyed mutex does not survive the D3D12 hop
(`IDXGIKeyedMutex` on a D3D12 resource is `E_NOINTERFACE`), proved by a D3D11
reader without the mutex seeing zeros where a D3D12 reader saw the frame — so a
real implementation needs fences or double buffering. The producer also becomes
adapter-bound, and failures only appear at `OpenSharedHandle`, so a per-stream
LUID check is the fallback gate. Note the card enumerates twice here
(`0x1488A`, `0x2079F`), so LUIDs must be compared, not names.

### Assessed, measured, and not built

Section 12 was first assessed as "not buildable here"; it has since been
measured instead, and the numbers are in `docs/measurements/README.md`:

- **Windows GPU capture-to-NVENC works**, with no CPU pixel copy: a real desktop
  through Windows Graphics Capture into a D3D11 texture into NVENC. It is worth
  the conversion and upload that stop happening — a couple of milliseconds a
  frame at 1080p, and about 4.4 ms of CPU copy at 3440×1440 — not the twenty
  times first rumoured. **Windows Graphics Capture, borderless, with the
  libwebrtc capturer as fallback**, is the decision; Windows 10 needs Desktop
  Duplication, which cannot remove the border and breaks on HDR frames.
- **Hardware decode** still needs an FFmpeg with `d3d11va` — the vendored one
  has neither hwaccel nor a single parser — so it remains gated on that build
  and on a display path that doesn't read every frame back to the CPU.
- **Intel QSV** needs oneVPL headers the vendor lacks, and additionally
  `--enable-d3d11va` on Windows; on this machine's UHD 770 it would buy H.264
  and HEVC encode and, more usefully, AV1 decode — but no AV1 encode.
- GPU conversion and the Linux surface paths gain nothing further on their own.

### Encoder preset, colour claim, and Opus complexity

Three questions that looked like they needed a human. They were measured instead,
in `spike/codec-evidence/` (not committed), on this machine's RTX 4090.

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
