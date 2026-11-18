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

## The iced interface and the Slint one

Anomi's iced window (`anomi`) beside the Slint one (`yapfast`), on the same
core and the same fake account. CPU is the process's, with the main thread's,
where either interface updates, lays out and draws, in brackets; wakeups a
second are the main thread's and all threads'; memory is the anonymous memory
and the window's frames, as above.

| | Slint CPU | iced CPU | Slint wakeups | iced wakeups | Slint memory | iced memory |
| --- | --- | --- | --- | --- | --- | --- |
| Start: first frame | 0.12 s | 0.09 s | | | 28 MiB¹ | 20 MiB¹ |
| Idle on a DM | 0.52% (0.50) | 0.25% (0.23) | 10.7 / 24 | 2.3 / 17 | 37.2 MiB | 30.6 MiB |
| Image-heavy channel | 0.35% (0.35) | 0.00% (0.00) | 8.4 / 18 | 0.1 / 5 | 50.9 MiB | 44.0 MiB |
| Scrolling | 18.7% (17.9) | 27.1% (26.5) | 227 / 810 | 145 / 743 | 50.9 MiB | 45.2 MiB |
| In a call, at rest | 4.9% (0.82) | 3.7% (0.20) | 13 / 599 | 2.6 / 581 | 63.5 MiB | 56.6 MiB |
| In a call, three speaking | 6.4% (0.85) | 4.0% (0.23) | 13 / 648 | 2.6 / 633 | 63.7 MiB | 56.7 MiB |
| Watching a stream | 9.5% (4.6) | 6.2% (2.7) | 57 / 672 | 30 / 705 | 63.1 MiB | 56.0 MiB |
| Busy server², member list shown | 2.30% (1.77) | 3.27% (2.65) | 33 / 90 | 25 / 154 | 43.2 MiB | 36.8 MiB |
| Busy server², member list hidden | 1.15% (0.62) | 0.67% (0.03) | 27 / 78 | 0.1 / 85 | 43.2 MiB | 36.6 MiB |
| Busy server³, in front | 17.1% (16.2) | 5.0% (4.2) | 150 / 452 | 35 / 238 | 43.8 MiB | 37.2 MiB |
| Busy server³, behind another window | 1.07% (0.45) | 0.85% (0.15) | 26 / 79 | 1.0 / 111 | 44.0 MiB | 37.3 MiB |
| Busy server³, in the tray | 0.77% (0.15) | 0.67% (0.00) | 29 / 91 | 1.0 / 111 | 32.0 MiB | 22.2 MiB |

¹ Anonymous memory as the start settles, 0.25 s in for Slint and 0.17 s for
iced; RSS then is 118 and 91 MiB. The start's CPU until then: 0.18 s and
0.15 s.
² 300 more members, one going offline or coming back ten times a second.
³ The same, with someone starting to type twice a second and a message a
second.

The iced window holds back what a busy server sends that it doesn't show:
who's typing elsewhere, and the member list while it's hidden or the window
is behind another or in the tray, which then reach no `update` at all
(`docs/iced/README.md`, "The background rule"). Without that, a hidden list
took 2.25% (1.70 on the main thread, 25 wakeups a second), the busy server
behind another window 1.87% (1.27, 15 wakeups) and in the tray 0.75% (0.12,
18 wakeups). Slint's typing
dots move every frame for as long as someone types in front; iced's stop
after 15 s without input, hence its busy server in front.

Where the iced window costs more:

- The member list's churn, shown: about 1 point of a core more than Slint
  at ten changes a second (the shown list costs iced 2.6 points over its
  hidden one, Slint 1.15). A presence change moves its member and shifts
  the rows between, which both interfaces paint again, and tiny-skia
  paints these rows (nameplates' gradients and pictures, rounded corners)
  dearly: profiled, the main thread spends about half its time painting
  and filling pixels, and about a third building, diffing and laying out
  the window; drawing the member rows once instead of twice
  (iced's `hover` lays out a row at rest and its look under the pointer)
  changed nothing measurable, so a `lazy` over the rest wouldn't either.
  What it would take: a renderer that keeps a picture drawn at another size
  scaled (upstream's renderer patch 24, examined but not ported to
  `vendor/iced_tiny_skia`), or a plate painted once into a picture per row.
- Scrolling: 8 points more, at the same frame rate. Each frame lays the
  timeline's rows out again and paints the whole timeline. Upstream's iced
  window damped the touchpad's reports to one scroll a frame (their
  72fcd542: 10.5% to 6.6%) and kept scaled pictures (patch 24: a scrolled
  frame 17 ms to 1 ms); neither is here yet.
- More wakeups of all threads in a busy server (154 against 90): each batch
  of the core's events wakes iced's executor thread (`anomi-ui`), held or
  not, about 0.1% of a core.

Two threads of the core cost the same in either window:

- In a call, the core's `yapfast-discord` threads take 3 to 3.5% of a core
  at rest, about 220 wakeups a second, and as much in push to talk with the
  key up (2.95% in front and 3.1% in the tray, iced's window, started with
  `scripts/ui-session start --layout '{"input_mode": "push_to_talk"}'`).
  The microphone's thread reads the microphone every 10 ms and runs
  WebRTC's echo canceller (both ways) and gain control, DeepFilterNet's
  encoder (`noise.rs` skips only its decoders when nothing is sent:
  `min_db_thresh` is infinite for `Sent::Never`) and the leveller, then
  sends nothing (`Processing::block` in `discord_audio.rs`); profiled, half
  its time is DeepFilterNet's network and a quarter the echo canceller and
  gain control. Muted, the capture stops, which is the 0.2% seen in
  the tray. Skipping the suppressor and the echo canceller while the key is
  up, and letting them settle for a block or two as it goes down, would
  take most of that; it is the core's to change.
- The `blocking` crate's pool keeps one idle thread waking every 500 ms for
  good (`blocking` 1.7 ends an idle thread only above its 500-thread
  limit): zbus 5 on async-io connects its Unix sockets through it
  (`Task::spawn_blocking` in `address/transport/mod.rs`), for the tray
  (ksni), notifications and the GlobalShortcuts portal. No setting parks
  it. zbus 5.19 built with `tokio` as well as `async-io` uses Tokio for a
  connection made inside a Tokio runtime (`abstractions::use_tokio`), whose
  blocking threads end after 10 s idle: `anomi-desktop` would open its
  session-bus connections inside the core's runtime (which runs I/O; iced's
  small executor runs only timers), and the tray through ksni's Tokio
  flavour. Both windows share `anomi-desktop`, so both would gain the same
  2 wakeups a second.

<details>
<summary>How these were measured</summary>

Measured on 19 November 2026 on one PC: an AMD Ryzen 9 9950X3D (16 cores, 32
threads) with 30 GB of RAM, under Arch Linux (kernel 7.3), each build
`measure`, by `testing/measure_tray.py --bin BUILD --parity dm busy` and
`--parity images scroll call`, on the private display of
`scripts/ui-session --tray` (a headless Weston 15 at 1400×860, its own
PulseAudio and D-Bus, `testing/tray_host.rs` as the tray), against
`testing/fake_discord.ts --servers 4 --dms 20 --members 300 --member-looks
--nitro --extra 400 --images 4`. Both draw on the processor (Slint's
software renderer, tiny-skia) and run the same core. Each figure is the mean
of two runs, each in a session of its own; runs of one build stayed within
0.3 points of CPU and 0.6 MiB of each other, but for Slint's call at rest
(1.3 points) and both scrolling (Slint 1.9, iced 0.3).

Each sample is 20 s, after 15 s for things to settle, with no input
meanwhile. CPU counts one core as 100%, and wakeups are voluntary context
switches (`scripts/measure-process`, which also gives each thread's CPU and
wakeups by name). Both windows draw their frames into shared memory, 9.2 MiB
at this size, gone in the tray. The first frame is the window's first new
buffer in Weston's log of surface commits (`testing/display/seat.c`), from
the process's start; the start settles when its CPU stays under 5% of a core
for a second.

- Idle on a DM: Bob's DM, which the window opens on.
- Image-heavy channel: #general scrolled up through its hundred pictures to
  its start and back down to the newest.
- Scrolling: #general with 60 touchpad scroll events a second, up and down
  by turns every two seconds, as `testing/measure_scroll.py` sends them.
- The call: the Lounge, joined in voice activity (the display's microphone
  is silent); then Bob, Carol and Dave speaking a tone without pause
  (`/_control/speakers-start`); then its stage, Bob's stream watched.
- Busy server: #general of Server. `/_control/churn-start` takes one of 300
  members offline or back ten times a second; with `/_control/typing-start`
  someone starts to type twice a second, in #general and in Bob's DM by
  turns, and `/_control/chat-start?every=1000` posts in #general every
  second. Behind another window: `weston-flower` opened over it takes the
  focus. In the tray: the window closed to it.

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
and prints the mean and peak RSS, mean PSS and CPU, with one core as 100%,
the main thread's CPU, and wakeups a second of the main thread and of all
threads, with each thread's CPU and wakeups by its name; it doesn't read
`RssAnon`. For a process's lifetime peak, read `VmHWM` from
`/proc/PID/status`. `testing/measure_tray.py` measures anonymous memory plus
swap and heap in use with #general open, closed to the tray and in a call;
it takes `--bin` for a saved build, and with `--parity` measures instead what
the table of the two interfaces holds, for either build. The other
`testing/measure_*.py` harnesses
and `testing/join_voice.py` cover cameras, scrolling, the network builds and
joining a call.

Alternate the builds over several runs and give the spread between runs of
one build. Keep the hardware, workload, warm-up, window state and memory
metric with every result. Dated records of past measurements are in the
private `records.md` beside this file.

