# Yapfast on macOS

Yapfast runs natively on macOS: an Apple Silicon or Intel binary, a real
`.app` bundle, the standard menu bar, a menu bar status item, and system
notifications. Nothing about the UI is faked — the same Slint interface as
Linux and Windows, in a Mac window with a Mac icon.

A release build on one M3, idle on the linking screen, was reported at 77 MB
of physical footprint, and 87 MB on a second run (`footprint -p`). The README's
40 MB is the Linux claim for doing nothing. The published Discord-only
Linux sample, software renderer, channel open, is about 66 MiB PSS and 91 MiB
RSS at 1120×760. Physical
footprint is a different measure. The window prefers 1120×760. At 2× its
software pixel buffer is 13.0 MiB, and CoreAnimation's share of the window
is in this process.

## Build it

Requires Xcode (or the Command Line Tools), Rust, and a network connection for
the first build, which downloads a prebuilt libwebrtc and prebuilt Skia.

```sh
git clone https://github.com/yapfast/yapfast
cd yapfast
./scripts/build-macos
```

That produces `target/dist/macos/Yapfast.app` and a zipped copy of it. Add
`--fast` for a quicker, unoptimised build while developing, `--dmg` for a disk
image, and `--universal` for a single binary that runs on both Apple Silicon
and Intel (it needs both libwebrtc downloads).

To run it straight out of the target directory without packing a bundle:

```sh
source scripts/macos-env.sh
cargo run --no-default-features --features desktop,discord
```

`scripts/macos-env.sh` points `LK_CUSTOM_WEBRTC` at one libwebrtc under
`.local/`. `scripts/fetch-libwebrtc-mac` unpacks it before Cargo runs. The
Go Live codec shims compile against that tree and do not download it.
`webrtc-sys-build` locks its own download, which does not fill the directory
for a build script that only reads it.

The first launch from a download asks macOS's permission for the microphone
when you join a call. Camera access is asked for when you turn a camera on.

## What's Mac-specific

- **Menu bar.** Application menu with About, Settings…, Hide and Quit; an Edit
  menu so the standard Cut, Copy, Paste, Undo and Select All shortcuts work in
  the message box; a Window menu. Settings… and Quit carry ⌘, and ⌘Q.
- **Status item.** The Yapfast mark in the menu bar, drawn as a template so it
  follows the light and dark appearance. Clicking it opens the window;
  right-clicking gives Open, Disconnect when a call is up, and Quit.
- **Closing the window hides it.** Yapfast keeps running in the menu bar.
  Clicking the Dock icon brings the window back, the same job the tray icon
  does on Linux and Windows.
- **Notifications** go through `UserNotifications`, and clicking one opens the
  conversation it came from.
- **Start at sign-in** uses `SMAppService`, so the switch in
  **Settings → Advanced** registers Yapfast as a login item rather than
  writing a launch agent.
- **Paste from Finder** reads the file URLs on the pasteboard, so copying a
  file in Finder and pasting into a message attaches it.
- **H.264 decodes through VideoToolbox.** The archive's FFmpeg carries no
  video decoders on the Mac, so `native/videotoolbox.c` decodes H.264 for both
  Go Live streams and video files, and takes the hardware's NV12 apart into
  the I420 planes the rest of Yapfast works in. OpenH264 is in the archive, but
  only its encoder, so there was nothing else to use.
- **Audio** uses a CoreAudio HAL unit for both playback and recording, at the
  device's own rate and channel count, resampled into and out of the 48 kHz
  stereo rings the rest of Yapfast uses. WebRTC still mixes, echo-cancels and
  encodes; this only moves samples.

## How the macOS code is shaped

The rule for every file here: **macOS code is additive and scoped, so nothing
Linux or Windows runs changes behaviour.** Where upstream has a per-platform
shape, macOS joins it rather than inventing one. The minimum needed, in their
syntax.

**Per-platform modules.** `crates/yapfast-core/src/audio_io.rs` names each
backend and aliases it to `platform`:

```rust
#[cfg(target_os = "linux")]   mod pulse;     #[cfg(target_os = "linux")]   use pulse as platform;
#[cfg(target_os = "windows")] mod wasapi;    #[cfg(target_os = "windows")] use wasapi as platform;
#[cfg(target_os = "macos")]   mod coreaudio; #[cfg(target_os = "macos")]   use coreaudio as platform;
#[cfg(not(any(...)))]          mod unsupported;
```

`src/autostart.rs` and `src/tray.rs` use the same shape (`linux`, `windows`,
`macos`, `none`). `src/notify.rs` and `src/paste.rs` are thin enough to use
inline `#[cfg(target_os = …)]` blocks instead, which is what they did before
macOS existed.

**The `platform` contract.** A backend is `Output` and `Input` with
`open`/`write`/`drain`/`flush`/`latency` and `open`/`read`, and — since
`audio_io/lasting.rs` landed upstream — `pub fn endpoint(id: Option<&str>,
output: bool) -> String`, the device a stream for that id would open now.
CoreAudio's answer is the id itself, since its names don't change when a device
comes back, and empty for the default. New backends have to answer it too.

**Device ids.** `voice::NAMED` (`"name:"`) prefixes a name where a platform has
no real id. CoreAudio accepts a bare id, `name:` + a name, or a plain name.

**Build scripts.** The root `build.rs` is one `match` on the target OS
dispatching to `windows_icon()`, `linux_skia()` and `macos()`; the AppKit
compile and the `-ObjC` link argument live in `macos()`.
`crates/yapfast-core/build.rs` lists every native source in one
`for file in […]` for `rerun-if-changed`, then compiles per OS with
`cc::Build` blocks gated on `target_os`. A new `.c` goes in the list, and in
the block that owns it.

**Where files go.** The app's own Objective-C is `native/appkit.m` at the root,
because the root `build.rs` compiles it; core shims live in
`crates/yapfast-core/native/`. `packaging/macos/` sits beside `linux/`,
`windows/` and `arch/`. `scripts/build-macos` mirrors `scripts/build-windows`;
it packs the bundle itself rather than having a separate `package-macos.py`,
which is the one place the three platforms aren't symmetrical.

## Known gaps

- **AAC and H264 come from the system, not the archive.** The archive's FFmpeg
  decodes FLAC, MP3, Opus, PCM and Vorbis, and no video at all, so an MP4 whose
  sound is `mp4a` and whose picture is `avc1` — a screen recording, a clip from
  a phone — would otherwise play silently and blank. `native/audio_aac.c` and
  `native/videotoolbox.c` put AudioToolbox and VideoToolbox behind those two
  paths. Neither is a packet-in, samples-out decoder, which is what the C shim
  hides:
  - An `AudioConverter` fills an output buffer and asks for as many packets as
    it takes through a callback, and it stops making samples for good once it
    asks and finds none. So packets queue and go over four at a time, which is
    also what one call is asked for, and handing them one by one decodes only
    the first.
  - It won't take a file's `AudioSpecificConfig` either: setting
    `kAudioConverterDecompressionMagicCookie` answers `'!dat'` however the
    config is shaped, and the converter decodes from the rate, the channel
    count and the packets alone.
  - A `VTDecompressionSession` hands pictures back in the order it decoded
    them, not the order they play, and only reorders when the sample's decode
    time says a later sample presents sooner. `native/videotoolbox.c` reorders
    them itself, holding four back, because a file's pictures arrive with the
    times to reorder by but the decoder doesn't use them.
  - A seek resets both: what either was holding belongs to what came before.
    The VideoToolbox session itself stays, since a file keeps its parameter
    sets in `avcC` rather than repeating them in the stream.
- **Go Live shares the camera, not the whole screen.** Screen CaptureKit
  capture isn't wired up yet, so screen sharing in a Go Live call doesn't
  offer the Mac's display.
- **Notifications need the bundle.** `UserNotifications` requires a real
  `CFBundleIdentifier`, so notifications don't arrive when running the bare
  `cargo run` binary. Use `target/dist/macos/Yapfast.app`.
- **Keychain prompts while re-building.** Yapfast's Discord token is stored in
  the keychain under an access list bound to the code-signing identity, and
  local builds are ad-hoc signed, so a new build asks to use the item the
  previous one stored. "Always Allow" clears it until the next rebuild. A
  Developer ID signature makes this a one-time prompt.
- **A duplicate JPEG symbol at the link.** Skia and libwebrtc each carry a
  libjpeg, and both define `jpeg_natural_order`, so every link warns and keeps
  one of them. The table is the JPEG standard's zigzag order, the same 64
  bytes either way, so nothing can tell which it got. Windows has the same
  collision and renames Skia's copy in `nix/prepare-windows-skia.py`; macOS
  has no such step yet and the warning is safe to ignore.
- **`__eh_frame` is over the compact unwind limit.** The same link reports
  that section too large for the compact unwind table, so unwinding falls back
  to DWARF at runtime. Nothing fails; it's the cost of libwebrtc, Skia and the
  noise-suppression models in one binary.
- **Two checks still fail for reasons of their own.**
  `custom_emoji::tests::skia_draws_the_glyphs_in_colour` can't get a typeface
  out of `FontMgr::new().new_from_data` on macOS, so the generated colour
  emoji font is never drawn in the test. And
  `ui::tests::the_composers_menu_cuts_copies_and_pastes_through_the_application`
  finds the draft unchanged after Select All then Copy, so what Copy puts on
  the pasteboard in a headless test never comes back.
- **Releases aren't notarized here.** The build ad-hoc signs so Gatekeeper
  accepts the app on the machine that built it. Distributing it needs a
  Developer ID and `--notarize`-style notarization, which needs an Apple
  developer account.
- **Intel is untested.** Everything is built and checked on Apple Silicon;
  `--universal` and the `x86_64-apple-darwin` target are wired up but nobody
  has run them.
- **The development scripts still assume Nix.** `scripts/check`,
  `scripts/dev` and the test stack want the Linux toolchain. Building and
  running the app on macOS doesn't.
