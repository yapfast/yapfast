# macOS

The macOS build runs on Apple silicon Macs with macOS 11 or later. Like the
Windows package it comes in two flavours: the release is Discord-only, and
the full preview adds Matrix. Both are the same app,
`io.github.yapfast.Yapfast`, so installing one replaces the other.

It is a first port. The window, both networks and screen sharing build from
the same code as Linux and Windows, but nothing on macOS yet plays or records
sound, films, shows a tray icon or notifications, or starts at sign-in; those
parts fall back to the stubs every system other than Linux and Windows gets.
See [what's missing](#whats-missing) and [what has been
checked](#checked-so-far).

## Build it

On the Mac, with [rustup](https://rustup.rs) and Apple's Command Line Tools
(`xcode-select --install`):

```sh
scripts/build-macos          # the release: Discord only
scripts/build-macos full     # the preview, with Matrix
```

The Nix shells are Linux's, so `scripts/build-macos` fetches the native
inputs they pin itself, each checked against its SHA-256, into `.local/macos`:
LiveKit's libwebrtc `webrtc-51ef663` built for macOS, protoc, and Skia's
prepared archive for macOS. It builds with `scripts/release-env`, so the
binary names no folder of the Mac that built it, and runs
`scripts/package-macos.py`, which puts in `target/dist/FLAVOUR`:

- `Yapfast.app`, to run from there;
- `Yapfast-0.1.0-macos-arm64.dmg`, or `Yapfast-0.1.0-full-preview-macos-arm64.dmg`,
  holding the app and a link to Applications;
- the source snapshot, as the Windows package has it, and `SHA256SUMS.txt`.

The app carries `LICENSE.txt`, `SOURCE.txt`, `BUILD.json`, `THIRD-PARTY.json`
and every dependency's licenses in `Contents/Resources`. On a Mac with 8 GB of
memory, set `CARGO_BUILD_JOBS=4` or less.

Two inputs need Apple's tools from the Command Line Tools 15, for macOS 13.5
and later: webrtc-sys compiles C++20 that needs clang 15 and its libc++, and
Skia's prebuilt Metal code calls Objective-C selector stubs, which only the
linker from Xcode 14 on makes. On older Command Line Tools, as macOS 12 has,
the script also fetches clang 18, libc++'s headers and lld from LLVM's own
build, which runs on macOS 11, compiles C++ with that clang and links with lld.
It turns on Apple's availability markings in those headers, as Apple's own
have them, so nothing they call is missing from the libc++ of macOS 11.

LiveKit builds libwebrtc against the macOS 26 SDK, and it weakly links three
ScreenCaptureKit symbols from macOS 14. SDKs older than 14.2 lack them, so
`.cargo/config.toml` and `scripts/release-env` let the linker leave them to the
system, which leaves them null on macOS before 14, where libwebrtc doesn't use
them.

## Run it

Open the disk image and drag Yapfast to Applications. The app has no
Developer ID signature, only the ad-hoc one Apple silicon requires, so the
first time macOS refuses to open a downloaded copy. Control-click the app and
choose **Open**; on macOS 15 and later, choose **Open Anyway** in **System
Settings → Privacy & Security** instead. A copy built on the same Mac opens
without asking.

Yapfast keeps its data in `~/Library/Application Support/io.github.yapfast.Yapfast`
and the Discord token and Matrix session secrets in the login keychain. Each
new build has a new ad-hoc signature, so macOS may ask once whether the new
build may read them.

Started from Finder, Yapfast logs nowhere visible. To see its log, run it from
Terminal:

```sh
/Applications/Yapfast.app/Contents/MacOS/yapfast
```

## Checked so far

On 2026-10-16, on an M1 Mac with macOS 12.4 and the Command Line Tools 13.4
(clang 13.1.6, macOS 12.3 SDK), so through LLVM's clang and lld, the Discord
flavour:

- passed `--smoke-test`, which starts Tokio, LiveKit's libwebrtc and Discord
  voice;
- opened its window, 1120×788, with Skia Software in `--preview`, at about
  80 MB resident, and closed on SIGTERM;
- against `testing/fake_discord.ts`, connected to the gateway, set its
  presence, cached a channel's messages and the avatars, and idled at 0% CPU
  and about 47 MB resident;
- opened with `--renderer gpu`, Skia on OpenGL, without falling back;
- as `Yapfast.app` from `scripts/build-macos`, opened through Launch Services
  (`open`) as Yapfast, `io.github.yapfast.Yapfast`; its disk image mounts with
  the app and the Applications link, and the app's signature verifies there.
- looked right by eye on the sign-in screen and in the local preview, which
  a click opened: text, layout and Retina sharpness.

`--preview` idled at about 10% of a core, where the session against the fake
idled at 0–0.5%; it hasn't been compared with Linux yet.

Not yet checked: typing; linking a real Discord account, which stores the
token in the keychain (the fake keeps its token in memory, so it doesn't
reach the keychain); the full preview with Matrix; and macOS 13 or later.

## What's missing

- **Sound.** `audio_io` has backends for PulseAudio and WASAPI only, so voice
  calls and streams' sound report that this system has no sound yet. A Core
  Audio backend would fill `crates/yapfast-core/src/audio_io/`.
- **Cameras**, which use native code for Linux and Windows only
  (`crates/yapfast-core/src/camera.rs`).
- **The tray icon, notifications and starting at sign-in**, each of which has
  a Linux and a Windows implementation and nothing for macOS
  (`src/tray.rs`, `src/notify.rs`, `src/autostart.rs`).
- A Developer ID signature and notarization, which need Apple's paid
  developer account.
