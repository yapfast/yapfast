# Yapfast on macOS

A native Mac app: a `.app` bundle, the standard menu bar, a status item,
Notification Center alerts and a login item, around the same Slint interface
as Linux and Windows. It draws with Metal and plays and records through
CoreAudio, and idles at 0 frames a second.

## Build and run

Needs Xcode or its Command Line Tools, and Rust. The first build downloads a
prebuilt libwebrtc and prebuilt Skia.

```sh
./scripts/build-macos            # target/dist/macos/Yapfast.app, and a zip
./scripts/build-macos --dmg      # also a disk image
./scripts/build-macos --fast     # unoptimised, for development
./scripts/build-macos --measure  # optimised with symbols, for profiling
./scripts/build-macos --full     # with Matrix as well as Discord
./scripts/build-macos --arch x86_64  # for Intel Macs; arm64 for Apple Silicon
./scripts/build-macos --universal  # one binary for both
```

It builds for the Mac it runs on unless told otherwise. Each architecture
builds against its own libwebrtc, and Intel against the macOS 26 SDK when it's
installed: the 27 SDK links some of CoreMedia to a Swift library macOS 26
lacks. Intel needs `nasm` for x264. To run without packing a bundle:

```sh
source scripts/macos-env.sh
cargo run --no-default-features --features desktop,discord
```

`scripts/fetch-libwebrtc-mac` unpacks the libwebrtc that `Cargo.lock` pins
into `.local/`, and `scripts/macos-env.sh` points the build at it, in place
of the Nix shell the other platforms use.

## What's Mac-specific

- **Drawing.** Skia's Metal backend, in a `CAMetalLayer` paced to the display,
  so scrolling runs at 120 Hz on ProMotion. It's the Mac's default renderer,
  where the other desktops default to Skia Software; `--renderer software` or
  the setting still chooses that.
- **Sound.** A CoreAudio HAL unit each for the speakers and the microphone,
  with a lock-free ring to the real-time thread. The speakers run only while
  there's sound to play. A microphone that won't take 48 kHz is heard at its
  own rate and converted; no device's rate is changed. WebRTC still mixes,
  cancels echo and encodes.
- **Video.** The Mac's libwebrtc has no video decoders and no AAC, so
  VideoToolbox decodes H.264 (`native/videotoolbox.c`) and AudioToolbox AAC
  (`native/audio_aac.c`). VideoToolbox takes 8-bit 4:2:0 only, so 10-bit,
  4:2:2 and 4:4:4 H.264 don't show; VP9 and AV1 at those do.
- **Menu bar.** Application, Edit and Window menus, with ⌘, for Settings, ⌘W to
  close to the menu bar, and ⌘Q to quit. Quitting from the Dock or at log out
  saves the window's place, as ⌘Q does.
- **Status item.** A template image that follows the menu bar's appearance.
  Click to open; right-click for Open, Disconnect during a call, and Quit.
  Closing the window leaves Yapfast here, and the Dock icon brings it back.
- **Notifications** through `UserNotifications`, grouped by conversation; a
  click opens it.
- **Start at sign-in** registers the bundle with `SMAppService`. One turned
  off in System Settings needs approval there, so turning it on opens Login
  Items.
- **Paste from Finder** attaches the files copied there.

## Where the code is

macOS joins the per-platform shapes already there, so Linux and Windows don't
change.

- `crates/yapfast-core/src/audio_io/coreaudio.rs` and `native/coreaudio.c`:
  the `audio_io` backend, aliased to `platform` like `pulse` and `wasapi`. Its
  `endpoint` answers a device's id, which stays the same when it comes back.
- `crates/yapfast-core/native/videotoolbox.c` and `audio_aac.c`: the decoders,
  called from `video.c` and `decode.c`.
- `native/appkit.m` and `src/appkit.rs`: menu bar, status item,
  notifications, login item and pasteboard, behind `tray.rs`, `notify.rs`,
  `autostart.rs` and `paste.rs`.
- `build.rs`'s `macos()` compiles `appkit.m` and adds `-ObjC`: libwebrtc
  needs NSString categories nothing names, and Cargo honours webrtc-sys's own
  `-ObjC` only for that crate. `crates/yapfast-core/build.rs` compiles the
  native shims and links their frameworks.
- `packaging/macos/` holds `Info.plist` and the icon; `scripts/build-macos`
  packs and signs the bundle.

## Known gaps

- **Not notarized.** Builds are signed ad hoc: right-click → Open the first
  time. A download that opens without a warning needs a Developer ID.
- **Keychain prompts after a rebuild.** Each ad-hoc signature is new, so a new
  build asks to use the Discord token the last one stored.
- **Notifications and the login item need the bundle**, not a bare `cargo run`
  binary.
- **Go Live shares a camera, not the screen.** ScreenCaptureKit isn't wired up.
- **Intel is checked only under Rosetta.**
- **The development scripts assume Nix.** `scripts/check`, `scripts/dev` and
  the test stack want Linux; building and running the app doesn't.
- **Link warnings.** Skia and libwebrtc both define `jpeg_natural_order`, the
  same JPEG table, and `__eh_frame` is too large for compact unwind. Neither
  affects the app.
