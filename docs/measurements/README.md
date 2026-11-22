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

