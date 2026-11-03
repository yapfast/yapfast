# Slint desktop client

Yapfast's desktop UI is Slint 1.18.1. The Matrix/session/voice services and
application model are in `yapfast-core`.

## Delivered behavior

- Password sign-in, saved-session restoration, recovery-key entry, trust refresh,
  sync retry and sign-out. Passwords and recovery keys are masked and cleared
  after starting the corresponding action or leaving the session.
- DMs at the top of the server rail, joined Spaces below, and only the chosen
  server's channels in the sidebar. Filtering, per-room drafts and each server's
  remembered channel use the shared application model.
- A selectable text timeline, earlier-message loading, encrypted sending and
  preserved send retries. New messages follow the bottom when already there;
  loading earlier history retains the reading position. Unencrypted rooms remain
  readable; the core continues to require a trusted device for sending.
- Persistent voice view and controls: join unmuted or muted, wait alone, participants,
  leave, mute/deafen, native device selection and explicit channel-switch
  confirmation. Browsing text, DMs or another server retains the backend session
  and voice connection. Preview voice never opens an audio device.
- A Discord-style layout set in Atkinson Hyperlegible Next (see [Chat
  UI](chat-ui.md#layout-and-look)), Lucide SVG icons, custom text fields, and
  window sizing from 1120×760 down to 800×560. Settings cover the window in
  sections: account, appearance (the [themes](themes.md)), voice and video,
  messages (text size, clock and link previews), notifications, sounds, Discord
  and advanced (the renderer, closing to the tray), closed by its close button
  or Escape. Desktop notifications for DMs and mentions show while Yapfast
  isn't in front; clicking one opens the conversation. Closing the window
  leaves Yapfast in the [tray](tray.md).

`ui/yapfast.slint` defines the window, with its controls and parts in the other
files under `ui/`; `src/ui.rs` and the modules under `src/ui/` adapt application
state and effects. The `yapfast-ui` crate compiles the interface. Its generated code
is large, so it lives apart from the application: changing Yapfast's Rust code does
not recompile it. Backend events wake Slint's executor through the Tokio
receiver. There is no idle polling timer. Stable widget models update changed
rows, and shutdown cancels event reception before releasing the backend actor.

## Renderers and native builds

Skia Software is the default. Settings → Advanced → Render with the GPU switches
to Skia GPU, saved as `gpu_rendering` in `layout.json`. Slint selects its
platform and renderer once per process, and glutin keeps the EGL library loaded
until the process ends, so the setting applies on the next start; the section
offers a restart, which replaces the process as the GPU recovery below does.
Software mode never loads EGL or the GPU driver. `--renderer gpu|software`
overrides the setting for one start. Start the local preview from the repository
folder:

```sh
nix develop path:./nix --command cargo run --locked -- --preview
```

For Skia GPU:

```sh
nix develop path:./nix --command cargo run --locked -- --preview --renderer gpu
```

Omit `--preview` for normal sign-in or session restoration. The Skia renderer
carries Yapfast's patches in `vendor/i-slint-renderer-skia`
([dependencies](building.md#dependencies)).

Software is the default because in a call it used about half the memory of GPU
rendering at the same CPU, and scrolling a long history cost less CPU
([renderer measurements](measurements/README.md)).

### GPU recovery

The GPU setting and `--renderer gpu` request Skia GPU through an explicit
`skia-opengl` renderer on Linux, and on Windows through the `skia` renderer
with Direct3D required, which Slint draws through wgpu's Direct3D 12. With
OpenGL on Windows, NVIDIA's driver kept a core busy while Yapfast drew.
The generic `skia` renderer without a required API would silently substitute a
software surface when the GPU fails, so Yapfast owns the fallback instead:

- If window creation, the GPU surface or the event loop fails, Yapfast logs the
  technical reason, releases the session and runtime, and restarts itself with
  `--renderer software`. Linux replaces the process; Windows starts the new
  process and returns its exit code. The window shows a dismissible notice.
- Three seconds after the event loop starts, Yapfast checks that the native window
  exists. Slint 1.18 waits up to 500 ms for the desktop appearance query before
  creating the first window, and a GPU surface failure at that point is stored
  without stopping the event loop, which leaves a running process with no window.
  The check uses Slint's version-pinned `unstable-winit-030` accessor rather than
  a rendering notifier, because Skia disables partial repainting while one is
  installed.
- A GPU start that came from the setting turns the setting off, so later starts
  don't fail and restart again; the person can turn it back on. Without the
  setting or the flag, Yapfast does not try the GPU at all.

`testing/probe_renderer_fallback.py` checks this policy on the private compositor.
It hides every EGL vendor from libglvnd to simulate a missing OpenGL driver:

```sh
nix-shell --impure testing/display/shell.nix --run 'scripts/run-framework-isolated --no-services --compositor-log .local/fallback.weston.log -- python3 testing/probe_renderer_fallback.py --output .impeccable/review/renderer-fallback'
```

The Nix shells supply checksum-pinned native inputs. Linux uses the upstream
Skia 0.153.3 archive. Windows builds the same pinned Skia source and
dependencies with `/MT`, matching WebRTC's static release runtime; the [Windows
guide](windows-beta.md) covers that build. Both libraries define
`jpeg_natural_order`, so the Skia archive preparation privately renames that
one table and its references (`nix/prepare-skia-native.py`). libwebrtc's
libjpeg-turbo prefixes its functions with `chromium_`, but not that table:
Chromium lists only functions to prefix. A Linux build without the Nix shell,
which leaves `SKIA_BINARIES_URL` unset and gets rust-skia's own archive, links
with `--allow-multiple-definition` instead (`build.rs`); the two tables hold
the same 320 bytes. Every build with the prepared archive links without that
flag, so a new clash still fails there. Production Cargo disables implicit
Skia source compilation.

The Windows package includes the Slint UI build sources and Skia notices.
`run-yapfast.cmd --renderer software` selects the CPU mode.

## Validation and limits

The automated GUI harness runs only on a private headless Weston compositor,
with a virtual input seat, private D-Bus and private XDG directories. Connected
fixtures disable D-Bus service activation, exercise the existing Linux private-file
credential fallback and use disposable local accounts. They open no audio devices
and never access the user's cursor, focus or desktop session.

The capture script is `testing/probe_slint_ui.py`. It covers only the Matrix
fixtures: the local preview, or with `--connected` a disposable account on the
local stack. It has no Discord fixture. Preview captures exercise local
sending, scoped navigation, draft restoration, voice persistence, mute/deafen,
the device popover, sign-out and password masking. Coordinate actions are checked
against their screenshots; a successful capture alone is not a functional result.

The connected fixture checks actual UI password sign-in, server/channel
loading, encrypted sending, process restart and session restoration, and
sign-out. It asserts that the server received one `m.room.encrypted` event,
that its content contains no plaintext fixture, and that sign-out removes
active session metadata. Screenshots verify the decrypted text and the restored
timeline. This is a single-account UI integration check; the shared-core tests
provide the broader trust, plaintext refusal and retry checks. The adapter
tests include actual pointer dispatch into the navigation controls, per-room
drafts, secret clearing and disabled sending under the security gates.

To reproduce the isolated checks after a desktop build:

```sh
nix develop path:./nix --command cargo build --locked --profile fast
nix-shell --impure testing/display/shell.nix --run 'scripts/run-framework-isolated --compositor-log .local/slint-ui.weston.log -- python3 testing/probe_slint_ui.py --output .impeccable/review/slint-ui --renderer gpu --size 1120x760'
scripts/test-stack up
nix-shell --impure testing/display/shell.nix --run 'scripts/run-framework-isolated --no-services --compositor-log .local/slint-connected.weston.log -- python3 testing/probe_slint_ui.py --output .impeccable/review/slint-connected --renderer gpu --size 1120x760 --connected'
```

Use `--renderer software --size 800x560` for the minimum-size capture. GUI
evidence lives under the ignored `.impeccable/review` directory, with binary
hashes and assertion results in `actions.json`. The probe runs
`target/fast/yapfast` unless `--exe` names another build. Add `--connected
--long-name` to exercise long labels against the local stack.

For screenshots and checks by hand, `scripts/ui-session` runs Yapfast, or a
Discord-only build against `testing/fake_discord.ts`, on the same kind of
private display and drives it from the command line;
[Seeing the app](building.md#seeing-the-app) shows it, and `--help` lists its
commands.

The Slint package still needs actual Windows rendering,
keyboard/IME/copy-paste, resize, graphics compatibility and Linux–Windows audio
checks by the tester. Native audio/backend regression evidence does not prove
the new desktop controls on every platform. The [tray](tray.md) is checked on
Linux against a stand-in tray and cross-built for Windows; real Windows still
needs its checks. SSO/emoji verification, room creation, ringing Matrix DM
calls and screen sharing on Matrix are not implemented; Matrix calls carry
cameras (`crates/yapfast-core/src/matrix_video.rs`). Attachments and the other
conversation features are described in [the chat guide](chat-ui.md).
