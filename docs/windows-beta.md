# Windows

The portable package targets Intel/AMD PCs running 64-bit Windows 10 or 11.
It comes in two flavours. The release is Discord-only; the full preview adds
Matrix, with the homeserver prefilled as `https://matrix.org` and editable,
and goes out as a separate pre-release. Both install as the same app, so
installing one replaces the other, and both keep one data folder.

## Run the package

`target/dist/discord` holds the release's portable ZIP and installer,
`Yapfast-VERSION-windows-x64-setup.exe`; `target/dist/full` the preview's,
`Yapfast-VERSION-full-preview-windows-x64-setup.exe`. The installer needs no administrator
rights: it installs for the current user into `%LOCALAPPDATA%\Programs\Yapfast`,
adds Yapfast to the Start menu, optionally to the desktop, and starts it when
the person signs in unless they untick that. Settings → Advanced turns that
on and off later ([the tray](tray.md#starting-at-sign-in)). Uninstalling,
from Settings → Apps, removes the program and the sign-in entry and keeps the
data folder. Neither is signed, so SmartScreen warns before either runs.

For the ZIP:

1. Copy `target/dist/discord/Yapfast-VERSION-windows-x64.zip`, or the full
   preview's ZIP, to the tester's PC.
2. Extract the whole ZIP, then double-click `yapfast.exe` in the extracted folder.
3. Link Discord by its QR code, or, in the full preview, sign in with a Matrix
   account. Matrix voice needs a homeserver that offers MatrixRTC (see
   [voice](voice.md)).
4. Open an encrypted room for text, or a voice channel for voice.

The package needs no Rust or Visual Studio installation. It includes the native
media code and Skia renderer. Opened from Explorer, Yapfast shows only its
window and writes its diagnostics to `yapfast.log` in its data folder,
`%LOCALAPPDATA%\Yapfast\Yapfast\data`, or the `--data-dir` folder. The log
grows across starts and starts over past 4 MiB. Started from a console, as
`run-yapfast.cmd` does, Yapfast writes there instead. If it closes immediately,
run `run-yapfast.cmd` to keep the error visible. `run-yapfast.cmd --renderer
gpu` or `--renderer software` picks the renderer for one start; see [Rendering
on Windows](#rendering-on-windows).

Passwords, recovery keys and saved sessions are excluded from the package.
Session secrets use Windows Credential Manager. Encrypted SDK stores live under
the user's application-data directory. Keep the first verified session while
device verification and recovery are being tested.

## Rendering on Windows

Yapfast renders with Skia Software unless "Render with the GPU" in Settings →
Advanced is on. On Windows that setting asks Slint for Skia with Direct3D
required, which draws through wgpu's Direct3D 12; Linux uses OpenGL. NVIDIA's
OpenGL driver on Windows kept about one core busy while it waited for the
screen, which is why Windows doesn't. If the GPU can't start, Yapfast restarts
itself with Skia Software, says so in the window and turns the setting off
(`src/graphics.rs`, `src/ui.rs`).

## First two-machine check

With Alice signed in on Linux and Bob on Windows:

1. Exchange messages in general and confirm they survive restarting Yapfast.
2. Alice joins Lounge, stays muted and waits. Bob joins later and should see Alice.
3. Choose microphone and speakers in the voice settings. Use headphones.
4. Unmute both people and confirm each can hear the other.
5. Check mute, deafen, leaving, and rejoining. Browse general while voice stays connected.
6. Choose a camera in the voice settings, try "Test video", then turn the camera
   on in Lounge. Each should see the other's camera. Restart Yapfast and check the
   camera is still chosen.
7. Record Windows version, GPU/driver, audio devices, cameras, input/copy/paste behavior,
   window resizing, and any error text. Never include passwords or recovery keys.

## Discord voice on Windows

Discord voice, the input meter and the Discord input sensitivity meter,
ringing, Yapfast's sounds, and a watched stream's or a video's sound use WASAPI
on Windows, in `crates/yapfast-core/src/audio_io/wasapi.rs`. Each stream is
shared-mode and event-driven: the audio engine converts between Yapfast's 48 kHz
and the device's format, and reading or writing waits for its event on the
thread that already did the reading or writing on Linux, copying straight
between Yapfast's buffers and the engine's. WebRTC's Windows device module names
each device by its endpoint id (`{0.0.0.00000000}.{…}` for speakers,
`{0.0.1.00000000}.{…}` for microphones), so the chosen device opens directly
by that id; none is the console default. Go Live's sound is in
[Go Live on Windows](#go-live-on-windows).

Wine's capture starts 0.3–1 s after it's opened, so the first moments a
microphone hears are lost there. Real Windows needs checking for that, along
with Bluetooth headsets, a device unplugged during a call, a default device
changed during a call, exclusive-mode applications, and whether the
communications ducking setting turns other sound down during a call.

## Go Live on Windows

Sharing, watching and playing a video run in the stream worker, as on Linux:
Yapfast starts its own executable with `--stream-worker`, which writes its logs
where Yapfast does: Yapfast's console, or Yapfast's log file. The video frames reach
the window through a temporary file both processes map. Windows won't delete
a mapped file, so there Yapfast creates it with `FILE_FLAG_DELETE_ON_CLOSE` and
`FILE_ATTRIBUTE_TEMPORARY`, which keeps it in memory while it fits, and
Windows removes it once both processes close it. On Linux it's still unlinked once the worker has it open.

Windows has no screen sharing dialog for Yapfast to use, so the share menu
lists what there is to share above the resolution: the screens as pictures, two
to a row, named "Entire screen" (or "Screen 1", "Screen 2", … with several),
then each window with a title. Yapfast's own windows aren't listed. libwebrtc's
capturers make the list, on a short-lived thread as the menu opens, and the
worker finds the picked one by its id. The menu opens with the screens' names;
their pictures come from a worker job (`Job::Glance`) that takes one frame of
each screen, scaled down to its card, and exits, so capturing them takes memory
only until they're in. A window closed after it was listed ends sharing with
"That screen or window is gone." Without a pick, Yapfast shares the first
screen.

Go Live's sound comes from WASAPI process loopback: one stream of everything
Windows plays except Yapfast's own process tree, the Yapfast window and its
workers. It needs Windows 10's 2004 update or later. Unlike Linux, where
each application is heard on its own and Discord's own client is left out,
Windows mixes every other application into one stream and can leave out only
Yapfast. Where Windows refuses process loopback, Yapfast shares without sound and
logs why, as it does on Linux without a PulseAudio server.

The stream's H264 goes to NVIDIA's NVENC, then AMD's AMF, through FFmpeg in
`crates/yapfast-core/native/hwenc.c`, while "Encode streams with the GPU" is
on, and to x264 when neither opens or the setting is off. Intel graphics
encode with x264 on Windows. Where the card encodes AV1 or H265, the stream
offers those first, in the same way. The camera always uses x264. AMF has
run on AMD hardware only in a test of `native/hwenc.c` on its own, on an
RX 7900 XTX. [Encoding on the GPU](discord/go-live.md#encoding-on-the-gpu) has
the rest.

Real Windows needs checking for: the window list's contents and titles,
sharing a window that's minimized, moved to another screen or closed while
shared, screens on mixed-DPI setups, Windows Graphics Capture's yellow
border, process loopback hearing other applications and leaving out Yapfast's
own voices, AMF on an AMD card in a whole Yapfast (AV1 on an RX 7000 or
newer), and the worker's start-up time from a
packaged build.

## Cameras on Windows

Cameras film through libwebrtc's DirectShow capture; see
[Cameras](voice.md#cameras) for how, and how a camera's id is kept.

Only real Windows can check a real camera through DirectShow's kernel
streaming (MJPEG, YUY2 and NV12, and the capability chosen for 1280×720 at 30
frames a second), that a camera's id survives restarts and replugging, COM on
the threads that list and open cameras, a camera another application is
using, the camera privacy setting turned off, built-in laptop cameras that
only Media Foundation lists, and the memory and processor time a camera
takes.

## The tray on Windows

The notification-area icon is `Shell_NotifyIconW` on a hidden window of the
UI thread; see [the tray](tray.md#windows).

Under Wine, closing the window to the tray and opening it again from the
icon work. A right click reaches Yapfast (`WM_RBUTTONDOWN`, `WM_RBUTTONUP`) and
`TrackPopupMenu` runs, but Wine's Wayland driver never shows the menu,
most likely because its owner is a window that was never shown. Until the
menu closes, its loop holds Yapfast's UI thread, as every Win32 menu does; on
Windows a click anywhere else closes it. So Open Yapfast, Disconnect and Quit
in the menu are untested on Windows.

[The tray](tray.md#windows) lists what to check on real Windows.

## Dragging the window

From the press on the title bar or a border until the release, Windows moves or
sizes a window in a message loop of its own. winit still passes window messages
on, but its own loop doesn't run, and Slint advances its timers and animation
clock only when it does, so without help the window looks frozen while dragged:
animations stand still, and pictures that arrive wait. `src/ui/move_loop.rs`
starts a Win32 timer with the first move or resize winit reports while Windows
is moving or sizing the window (`GetGUIThreadInfo`, `GUI_INMOVESIZE`). Windows
calls it from its own loop every 16 ms, and it advances Slint's timers and
animations and redraws what animates, until the window stops moving.
`YAPFAST_LOG=yapfast=debug` logs when it starts and stops.

Wine doesn't reproduce the freeze. So check on real Windows: while the window is
held by its title bar, still and moving, "… is typing" should keep pulsing
and its dots rising; the same while resizing by a border.

## The window's size and place

Yapfast starts where it was left; see [Layout and
look](chat-ui.md#layout-and-look). Slint makes the native window only once its
event loop runs, and shows it straight away, so there's no window to give
`SetWindowPlacement` before it shows. Yapfast gives Slint the size and place
before `show()` instead, and keeps the un-maximized ones itself while the
window is maximized. `MonitorFromRect` and `GetMonitorInfoW`
(`src/ui/placement.rs`) check them against the monitors: if the title bar would
show on no monitor, Yapfast forgets the place and Windows puts the window on
the primary monitor, and a window too big for its monitor's work area shrinks
to it.

Check on real Windows: two monitors at different scales, starting on the
one that isn't the primary, maximized and not; a monitor unplugged while
Yapfast is closed; a window snapped to half the screen, which comes back at
the snapped size; and the taskbar at the top or the left.

## Rebuild from Linux

On this NixOS host, from the project folder:

```sh
nix develop path:./nix#windows --command scripts/build-windows        # Discord release
nix develop path:./nix#windows --command scripts/build-windows full   # full preview
```

Both flavours build to the same `yapfast.exe`, so each run packages straight
away; the packaging script tells them apart by the Matrix SDK's source paths
and refuses the wrong one. This installs the pinned Rust toolchain's Windows
target, cross-compiles with cargo-xwin and Clang, then packages the executable.
The first run downloads the Microsoft CRT/SDK and compiles the C++ media
bindings.

The Windows Nix shell supplies the matching `webrtc-51ef663` archive, LLVM 21,
GN, Protobuf, NASM, Wine and checksum-pinned Skia 0.153.3 source with its
dependencies. `scripts/build-windows` builds Skia with `/MT` to match WebRTC;
the upstream Skia Windows archive uses an incompatible dynamic CRT. This source
build uses two native jobs and is cached. Preparation then gives Skia's JPEG
table a private symbol name. It checks runtime directives and symbols, and
preserves every native section's code/data and relocations through the rename.
The executable links the C runtime statically, so it needs no separate Visual
C++ runtime DLL.

`scripts/build-windows` sources `scripts/release-env x86_64-pc-windows-msvc`
before `cargo build --release`: it takes `cargo xwin env`'s environment and
remaps source paths, so a binary people download names no folder of this
machine ([Building](building.md#windows)). Source it the same way before any
Windows binary you hand to someone.

A [one-line WebRTC patch](../vendor/webrtc-sys/README.yapfast.md) also fixes an
MSVC class/struct declaration mismatch, matching the upstream resolution.

The package contains dependency notices, original licenses and build hashes.
The source snapshot, `Yapfast-VERSION-source.zip` (the full preview's
`Yapfast-VERSION-full-preview-source.zip`), sits beside the packages in
`target/dist/FLAVOUR` rather than in them, and goes on the release page with
them, tag `vVERSION` or `vVERSION-full-preview`: `SOURCE.txt` in each package
names that page and `BUILD.json` the snapshot's SHA-256, which the AGPL
dependencies' source offer rests on, so a release's snapshot stays up as long
as its packages do. `SHA256SUMS.txt` in each folder covers the snapshot, the
ZIP and the installer. The notices and `THIRD-PARTY.json` list only the crates
the flavour builds. The snapshot holds only what git tracks; the full
preview's adds the exact upstream source of the AGPL MatrixRTC crates, which
the Discord release doesn't build. It excludes `.local`, ignored private material, account credentials, sessions,
logs and the Wine prefixes.

## Build for development

For work on Windows code, build with the `fast` profile through cargo-xwin in
the Windows shell. The app crate needs the prepared Skia archive, as
`scripts/build-windows` makes it:

```sh
nix develop path:./nix#windows --command bash -c '
  export SKIA_BINARIES_URL="file://$(python3 nix/prepare-windows-skia.py)"
  cargo xwin build --profile fast --target x86_64-pc-windows-msvc \
    --no-default-features --features desktop,discord'
```

Drop the two feature flags for the full build. The executable is
`target/x86_64-pc-windows-msvc/fast/yapfast.exe`. `yapfast.exe --help` and
`--smoke-test` run under Wine without a display; `--audio-devices` lists the
microphones, speakers and cameras Wine sees.

## Test under Wine

Build the test executable without running it, then run it with Wine.
`yapfast-core` needs no Skia:

```sh
nix develop path:./nix#windows --command \
  cargo xwin test --no-run --profile fast --target x86_64-pc-windows-msvc -p
  yapfast-core --lib
```

Cargo prints the path,
`target/x86_64-pc-windows-msvc/fast/deps/yapfast_core-HASH.exe`. Run it in its
own prefix, with the usual test filter and flags:

```sh
nix develop path:./nix#windows --command env WINEPREFIX=/tmp/wine-test
WINEDEBUG=-all \
  DISPLAY= WAYLAND_DISPLAY= wine
  target/x86_64-pc-windows-msvc/fast/deps/yapfast_core-HASH.exe frames
```

A new prefix takes `wineboot -i` first. The device tests are `#[ignore]`d;
run each by name with `--ignored`, and give it what its ignore reason asks
for:

- Sound (`audio_io` and `discord_audio` tests): `PULSE_SERVER` naming a private
  PulseAudio of null sinks, whose default input hears its default output, so
  nothing reaches real speakers. `a_call_sends_what_the_microphone_hears` also
  needs `bun testing/fake_discord.ts` and `YAPFAST_DISCORD_TEST_SERVER`.
- Screens and windows
  (`stream_worker::tests::shares_a_listed_screen_and_window`): an X display,
  such as `Xvfb`, with `wine notepad` open on it. Wine's GDI window capture
  gives black windows without a compositor; Windows uses Windows Graphics
  Capture.
- Cameras (`camera::tests::films_a_real_camera`, with `YAPFAST_TEST_CAMERA`
  naming it): Wine's DirectShow, built without libv4l2, reads only a V4L2
  device that supports `read()` and BGR24, such as a v4l2loopback device fed
  by `ffmpeg -re -f lavfi -i testsrc2=size=1280x720:rate=30 -pix_fmt bgr24 -f
  v4l2 /dev/videoN`. It lists a USB webcam too, which then fails to open.

`audio_io::tests::hears_other_applications` can only show the fallback under
Wine: Wine has no `VAD\Process_Loopback` device, so the capture fails with
"File not found" and sharing goes on without sound. AMF can't open either,
since Wine has no `amfrt64.dll`, so streams encode with x264 there.

## Run the app under Wine

`scripts/ui-session` runs a Windows build through `--bin`, a script that
starts Wine and waits on `wineserver -w`. Start the session from the Windows
shell, which has Wine, with `TMPDIR=/tmp`: the shell's own `TMPDIR` makes the
Wayland socket's path longer than its 108-byte limit.

```sh
nix develop path:./nix#windows --command env TMPDIR=/tmp \
  scripts/ui-session start --discord --bin /tmp/wine-yapfast.sh
```

There are two ways to show the window, and each misses something:

- Wine's Wayland driver, on the session's Weston: the script unsets `DISPLAY`
  and runs plain `wine yapfast.exe "$@"`. This is enough for most UI checks.
  `wine explorer /desktop=…` fails here with "xdg_surface geometry … is
  larger than the configured fullscreen state", since the desktop's frame
  doesn't fit the kiosk window. Screen capture gets no frames, the preview of
  a share is black, Weston's kiosk shell ignores moves and resizes, and the
  tray's menu never shows.
- Wine's X11 driver, on a rootful Xwayland inside the session: the script
  starts `Xwayland :58 -geometry 1120x760` (the session's size; not
  `-fullscreen`, which gives 640×480 stretched), then, with `DISPLAY=:58`,
  `WAYLAND_DISPLAY` unset and `SLINT_SCALE_FACTOR=1`, runs `wine explorer
  /desktop=yapfast,1120x760 yapfast.exe "$@"`. GDI then captures the real
  screen, and the window can be moved, resized and maximized. Stop the extra
  Xwayland by its PID after `scripts/ui-session stop`.

Wine on a bare X server such as Xvfb can't show the window: Xvfb reports its
monitor at 0 Hz, and Slint's winit backend divides by the refresh rate
(`frame_throttle.rs`), so it panics before the window opens. A Windows
display that reports no refresh rate would hit the same.

`--renderer gpu` draws Direct3D 12 through vkd3d on Mesa's software Vulkan,
which shows the path works but nothing about a real card. To check the
fallback to Skia Software, hide Vulkan with `VK_DRIVER_FILES=/nonexistent.json
VK_ICD_FILENAMES=/nonexistent.json`. Don't use `WINEDLLOVERRIDES=d3d12=d`:
`yapfast.exe` imports `d3d12.dll`, so it then won't start at all.

## What's open

Most of what this guide describes was checked under Wine, and each section
above ends with what real Windows still needs. Beyond those: text entry,
selection, copy/paste, keyboard focus, resizing, both renderers, restarts,
network interruptions and crashes on a tester's PC; whether Direct3D 12 keeps
an NVIDIA card's CPU use low; a display that reports no refresh rate, under
remote desktop or in a virtual machine; Intel GPU encoding; and a native
Windows build, which [Building](building.md#windows) describes but nobody has
run. The packages are unsigned.
