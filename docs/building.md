# Native builds

## Shells

Three Nix shells cover the work:

- `nix develop path:./nix` is the development shell. Run every `cargo` command
  in it, as `nix develop path:./nix --command cargo ...`. It also provides
  `bun` for the fake Discord.
- `nix-shell --impure testing/display/shell.nix` adds Weston,
  PulseAudio, the ALSA plugins and FFmpeg to it, for the private display that
  `scripts/ui-session` starts.
- `nix develop path:./nix#windows` cross-builds for Windows; see the
  [Windows beta guide](windows-beta.md).

The repository's own `flake.nix` builds no shell, only the package that
`nix profile add github:yapfast/yapfast#yapfast` installs: the Discord-only
release build, from `nix/package.nix`. It shares libwebrtc and the
system libraries with the development shell through `nix/native.nix`, and pins
the same nixpkgs in its own `flake.lock`. `nix build` puts it in `result`.
It also builds the AppDir that the [AppImage](packaging.md#linux-appimage) is packed from.
`nix/package.nix` holds one hash per Git dependency's repository and commit,
in `outputHashes`. Moving a Git dependency to another commit needs a new
hash there; a `nix build` with the old one prints it.

## Dependencies

Yapfast pins Rust 1.97.1 and iced 0.14.0. Cargo.lock pins the complete Rust
graph. iced's two renderers and its text shaper, cosmic-text, are patched in
`vendor/`; see [dependency sources](#dependency-sources) and [the vendored
toolkit](client/toolkit.md).

The Matrix SDK revision follows the pinned MatrixRTC revision. Keep those two
sources together when upgrading.

Every emoji Yapfast draws, in the picker, reactions, suggestions, names and
message text, is drawn from Twemoji's pictures, the artwork Discord's client uses: `vendor/twemoji` is
`assets/72x72` from https://github.com/discord/twemoji at the revision
`vendor/twemoji/REVISION` names, under CC-BY 4.0, which asks for the attribution
`vendor/twemoji/README.yapfast.md` gives. `crates/yapfast-iced/build.rs` makes them
one font the window loads (`emoji_font_tables.rs` beside it writes the font's
tables). The font maps each single emoji's own code point and joins each sequence
of several, such as a flag or a skin tone, into its picture with GSUB ligatures, so
text draws Twemoji's picture for an emoji as typed; no system emoji font is used,
and the picker lists only the emoji Twemoji has a picture for. `scripts/release_package.py` copies the
licences and attribution into each package and `THIRD-PARTY.json`. Update by
taking the new commit's files over these, as `vendor/twemoji/README.yapfast.md`
says.

LiveKit's Rust crates must stay on the mutually compatible versions in
Cargo.lock. Do not run an unrestricted `cargo update`. Upgrade against the
MatrixRTC lockfile and rebuild the native media stack. On Linux, the Nix shell
supplies libwebrtc `webrtc-51ef663`, matching webrtc-sys-build 0.3.18. Updating
that crate requires updating the archive URL and checksum in `nix/native.nix`
(Linux) and `nix/flake.nix` (Windows) too.

The Linux environment includes ALSA, PulseAudio, fontconfig, FreeType, GLib,
Wayland, X11, Vulkan, OpenGL, OpenSSL, Clang 21, CMake, pkg-config, Protobuf
and NASM. It uses the host's graphics driver under `/run/opengl-driver/lib`. A
real display session is required to open the window. It draws on the CPU, with
tiny-skia, by default; `--renderer gpu` or the GPU setting draws with wgpu, and
a GPU that can't start falls back to the CPU. See [the
window](client/window.md#building-and-running).

The first build downloads dependencies and compiles native C++. Cargo uses every
core. On a machine with 8 GB of memory, set `CARGO_BUILD_JOBS=2` so the Matrix
SDK and the C++ media bindings do not exhaust it.

Development builds omit debug symbols for dependencies; the app keeps line
tables. They optimize DeepFilterNet's crates and turn off tract-core's debug
assertions, which reject its model; see Cargo.toml. Release builds strip
symbols and use no LTO, which made both platforms' binaries bigger for no
measured speed. Use them for builds people run. The `measure` profile
optimizes like release, keeps symbols and builds incrementally. Use it for
[measurements](measurements/README.md) and profiling: `cargo build --profile
measure`. The `fast` profile has the same settings in a target folder of its
own, `target/fast/`. Use it for tests, local runs and checks: `cargo build
--profile fast` or `cargo test --profile fast`. Builds to measure then stay
apart from the ones everyday work replaces.

A plain `cargo build`, `cargo build --workspace` (`scripts/check`'s) and
`scripts/ui-session`'s build give `yapfast-core`, the window and the binary
the same features, so after an edit only the first of them compiles: Cargo
compiles a crate once per set of features, and these three are the slowest.
Two things keep them alike. The root's `discord` feature also turns on
`yapfast-core/default`, which `--workspace` turns on as it selects the core.
And the programs ui-session runs beside Yapfast, `testing/tray_host.rs` and
`testing/record_host.rs`, are the package `testing/hosts`, not examples of
the root: an example builds with the root's dev-dependencies, which turn on
`yapfast-core/test-fixtures`. Tests do build with that feature, so `cargo
test` compiles those crates once more, apart from these.

### Dependency sources

Yapfast patches these crates in `vendor/`, through `[patch.crates-io]` or a path
dependency in Cargo.toml:

- `matrix-rtc-livekit`: Element's matrix-rust-rtc at `5831af8`, with native
  audio tracks and cancellable joins.
- `webrtc-sys`: webrtc-sys 0.3.35 with a Windows link fix, capture leveling,
  remote track volume and camera capture from pooled buffers.
- `davey`: davey 0.1.4, keeping its key packages when a Go Live stream's group
  id changes, and reading a frame's overlong nonce without panicking.
- `tungstenite`: tungstenite 0.26.2, giving back the read buffer a large frame
  such as Discord's READY grew.
- `iced_tiny_skia`: iced 0.14's CPU renderer, 0.14.1: pictures drawn from the
  app's own pixels and copied
  onto the frame with their round corners, glyphs copied the same way, clips
  by rectangle instead of a window-sized mask, quads, gradients and shadows
  filled only inside the clip, damaged regions merged, and a hook through
  which the client draws video frames itself with libyuv, which the libwebrtc
  archive already links in.
- `iced_wgpu`: iced 0.14's GPU renderer, 0.14.0: round image corners where
  the quad shader puts them, each corner's own radius, and pictures up to 4K
  uploaded in the frame that draws them, so a stream's frames show.
- `cosmic-text`: cosmic-text 0.15.0, the text shaper, telling
  the client which characters no loaded font has, so it can load a system
  font that covers them.
- `deep_filter`: libDF, DeepFilterNet's Rust implementation, from its
  repository at `d375b2d` (crates.io has only its 0.2.5), with the model it
  builds in, ported from tract 0.21 to tract 0.23.

`vendor/x264-src` isn't a patched crate but x264's own C source, unchanged and
trimmed to what an 8-bit encoder library uses, with a build script in place of
its configure and Makefile. Go Live and the camera encode H264 with it; NASM
assembles its x86-64 code, so every shell, package and native build needs
`nasm`.

`vendor/ffmpeg-src` is FFmpeg's source trimmed to its hardware encoders
(NVENC, AMF on Windows, VA-API on Linux) and its HEVC decoder, with
nv-codec-headers', AMF's and libva's headers and configure's output for
Linux and Windows, all made by its `update.sh`, which configures Windows
with clang-cl against the MSVC runtime and SDK cargo-xwin downloads; three
patches let NVENC, AMF and VA-API change their bitrate without a keyframe.
Its build script compiles it, and NASM assembles its x86-64 assembly, with
every symbol prefixed, apart from the FFmpeg in libwebrtc, and it loads
libva, NVIDIA's driver and AMD's AMF runtime only when an encoder opens.
Its `README.yapfast.md` explains the rest. Targets without a `gen/` folder build without hardware
encoding or H265 decoding.

Each has a `README.yapfast.md` with its changes.

Crates that aren't vendored are in
`~/.cargo/registry/src/index.crates.io-*/NAME-VERSION/`. The Matrix SDK is a git
dependency; its checkout is `~/.cargo/git/checkouts/matrix-rust-sdk-*/c6ad52e/`,
the short form of the revision pinned in Cargo.toml. libwebrtc's headers are in
`$LK_CUSTOM_WEBRTC/include` inside the development shell.

### Media libraries

Three libraries decode and encode media, each for its own formats. The C
files named here are in `crates/yapfast-core/native/`.

- LiveKit's libwebrtc archive, `$LK_CUSTOM_WEBRTC` in the development shell,
  holds libvpx, dav1d, libyuv and Chromium's FFmpeg, which has H264 and
  common audio codecs but no HEVC decoder. `decode.c` decodes watched VP8
  and H264 streams with it, and `av1.c` watched AV1 with its dav1d.
  `video.c` plays video files: libavformat demuxes, libavcodec decodes
  H264 and the sound, libvpx VP8 and VP9, dav1d AV1, and libyuv converts to
  RGBA. It also decodes the soundboard's MP3
  and Ogg sounds, which have no pictures. `vp8.c` and `clip.c` encode VP8 and VP9
  with libvpx, `picture.c` scales with libyuv, and `camera.cc` captures
  through libwebrtc's capture module.
- `vendor/ffmpeg-src` holds only the GPU encoders, which `hwenc.c` drives,
  and the HEVC decoder for watched H265 streams, in `hevc.c`.
- `vendor/x264-src` encodes H264 on the CPU, in `h264.c`, for Go Live and
  the camera.

Playing a file never touches `vendor/ffmpeg-src`, so what plays is what
libwebrtc's FFmpeg was built with. Check a symbol in the archive:

```sh
nm $LK_CUSTOM_WEBRTC/lib/libwebrtc.a | grep ' T avformat_open_input'
```

On NixOS, a binary run outside `nix develop` opens NVENC only with
`LD_LIBRARY_PATH=/run/opengl-driver/lib`, which the dev shell and the Nix
package's wrapper add. Yapfast names the libva driver for each render node
itself, but `vainfo` or FFmpeg's command line on a host that sets
`LIBVA_DRIVER_NAME=nvidia` needs `env -u LIBVA_DRIVER_NAME` to reach AMD's
or Intel's driver.

## Checks

`scripts/check` enters the development shell itself when Nix is installed, and
uses the tools already installed otherwise. `scripts/check --fast`
builds with the `fast` profile, as local work does. It runs, in order:
formatting; `scripts/check-doc-links`, which fails on a Markdown link to a
missing file or heading; Clippy with warnings denied and the tests for the
whole workspace; a build; and the smoke test, which runs
`target/<profile>/yapfast` from that build. These build the default features,
the release's: the window with Discord (see [Networks](#networks)).

Clippy checks more than its defaults: the root `Cargo.toml`'s
`[workspace.lints]` adds lints against clones, copies and collections the
code doesn't need, against leftovers such as `dbg!` and `todo!`, and asks a
reason on every `#[allow]` and a `// SAFETY:` comment on every `unsafe`
block. Each crate takes them with `[lints] workspace = true`. An allow that
only one build needs, such as dead code outside Windows or a `&mut` only
Discord uses, goes in `#[cfg_attr(<that build>, allow(...))]`.

`scripts/check --matrix` adds the builds with Matrix, after those and one at
a time, as two compiles at once can run a machine out of memory: Clippy and
the tests with Matrix beside Discord; then, in `target/check-combinations/`
so their binary doesn't replace the desktop one, the builds without the
window: Clippy with Matrix alone, in the program and the core, and the
headless build's Clippy and smoke test with both networks. Run it before a
release and after a change to code that Matrix shares, such as
`#[cfg(feature = "matrix")]` branches in `yapfast-core` and the app. Each
step prints `=== step` and `rc=N` when it ends, with Rust's errors when it
fails, and keeps its full log under `target/check/`. The script runs every
step and exits non-zero if any failed.

A new worktree starts with an empty `target/`, and its first check compiles
every dependency. `scripts/warm-worktree`, run inside it
before the first build, copies the main checkout's `target/fast/` and
`target/check-combinations/` as reflinks, which share disk space with the
originals, and cleans the crates whose sources are in the repository, so those
build from the worktree's own files. The first check then takes about 3.5
minutes. Reflinks need a filesystem such as btrfs.

After a change under `vendor/ffmpeg-src` or `vendor/x264-src`, each check
step compiles it again; Clippy alone takes about 4 minutes. Run Clippy on the
changed crate first, `cargo clippy --profile fast -p <crate> --all-targets --
-D warnings`, to catch lints in seconds before `scripts/check`.

`git config core.hooksPath .githooks` turns on the pre-commit hook, which
checks formatting only, so commits stay quick.

Tests open no sound device. The core's tests, and the clients' once they
make their window's state, play to and record from silent stand-ins that
take and give samples at a device's pace (`audio_io::silence_audio`), so a
test is never heard and never hears the room. The ignored device tests use
the system's devices when `YAPFAST_TEST_AUDIO` is set; set it only with
`PULSE_SERVER` naming a private PulseAudio of null sinks, as
`scripts/run-framework-isolated --audio` makes, never the desktop's own.

## Seeing the app

`scripts/ui-session` runs Yapfast on a private Weston display, for screenshots and
checks by hand, without touching the desktop or a real account:

```sh
scripts/ui-session start      # the preview; --discord for the fake Discord
scripts/ui-session do 'click 40 100' 'type hello' 'key enter' 'shot sent'
scripts/ui-session relaunch   # rebuild and restart Yapfast on the same display
scripts/ui-session stop
```

`start` builds the default build, the release's, into `target/fast/yapfast`
and runs a copy of it: on the preview, or with `--discord` against the fake
Discord. Screenshots go to `.local/ui-session/shots/`, and `scripts/ui-session
--help` lists every option and command. `start --tray` adds a stand-in tray on
the session's bus; see [the tray](tray.md#testing).

For demo footage, the session records its display at 60 frames a second:

```sh
scripts/ui-session start --discord --size 1920x1080 --zoom 1.2 --cursor 32
scripts/ui-session do 'record start tour' 'glide 400 600' 'click 170 264 glide' \
  'glide 1000 500' 'scroll -1200 1.5' 'sleep 1' 'record stop'
```

`testing/record_host.rs` takes the frames through Weston's capture protocol
and ffmpeg encodes them to `.local/ui-session/videos/tour.mp4`, which later
starts keep. `--zoom` opens Yapfast at that zoom level, `--cursor` shows the
pointer, `glide` and `click … glide` move it as a hand would, `scroll
PIXELS SECONDS` scrolls smoothly as a touchpad whose fingers stay down,
`fling PIXELS SECONDS` as one at a steady speed whose fingers then lift,
and `wheel NOTCHES SECONDS` turns a mouse wheel notch by notch. Weston's headless display repaints about
58.5 times a second at most, so about one frame in 40 repeats the one
before.

To see how smoothly a list moves, `start --refresh HZ` gives the display
another refresh rate, and `steps` records every repaint's row sums instead
of a video:

```sh
scripts/ui-session start --discord --size 1920x1080 --refresh 180 --bin target/fast/yapfast
scripts/ui-session do 'move 900 500' 'steps start notch 400 1300' 'wheel -1' 'sleep 1' 'steps stop'
testing/frame_steps.py .local/ui-session/videos/notch.rows --from 60 --to 1000
```

`testing/frame_steps.py` matches each repaint's rows against the one
before and prints how far the list moved on each repaint, the time between
repaints and between moves, the repaints that showed no move while it
moved, and how even the steps were. Weston's headless output times its
repaints in whole milliseconds and waits at least one more, so a busy
display repaints every `floor(1000 / HZ) + 1` ms or a little over: 180
gives about 164 repaints a second, 250 about 196. The window's own
timings come from its frame trace (`YAPFAST_LOG=debug`, target
`yapfast::frames`; see [the window](client/window.md#building-and-running)).

## Networks

Matrix and Discord are Cargo features, `matrix` and `discord`, and the window
is `desktop`, which turns `discord` on: the window shows Discord in every
build. The default build is the release's, the window with Discord; a build
can add Matrix:

```sh
cargo build --release                    # Discord only
cargo build --release --features matrix  # Discord and Matrix
```

Releases are the Discord-only build: `nix/package.nix` (`buildFeatures`) and
`scripts/package-linux.py` (`FEATURES`) name its features, `desktop,discord`,
rather than rely on the defaults. The Matrix tools, such as `--voice-demo`,
`--call-experiment`, the session commands and `scripts/test-voice`, need
`--features matrix`. Measure with the release's features.

A Discord-only build signs in by linking Discord by QR code, and connects with
the account linked on the device from then on. Voice calls
still need LiveKit's libwebrtc, which provides the audio processing and the
device list for both networks. A build without `desktop`, such as
`--no-default-features --features matrix,discord`, has no window: it runs only
the commands that need none.

`YAPFAST_DISCORD_TEST_SERVER=http://127.0.0.1:18400` points Discord at
`testing/fake_discord.ts` (`bun testing/fake_discord.ts --port 18400`). It uses
a test token instead of the linked account, so the real token never reaches it.
`scripts/test-fake-discord [FILTER]` runs the ignored tests in
`crates/yapfast-discord/tests/fake_server/` with the `fast` profile, or those
a filter names, such as `forum` or `calls_in_dms`, a second or so apiece. Each starts a
fake of its own, so none depends on another and a filter runs any alone. `cargo
nextest`, in the development shell, runs them side by side, eight at a time
(`.config/nextest.toml`), and the whole file takes about 15 seconds; without
it, `cargo test` runs them one after another in about a minute. The script then
runs yapfast-core's Discord tests against one fake, and
`crates/yapfast-discord/tests/fake_env.rs` against a fake that runs the example
environment.

`--env PATH` replaces the fake's built-in content with an environment, for
demos, promo recordings and screenshots. An environment is a JSON file, or a
TypeScript module whose default export is the environment. It sets up the
account, people with avatar pictures, statuses and friendships, DMs and
group DMs, and servers. A server has roles, members, categories, text
channels with history and unread counts, and voice channels with people in
them. One of those people can stream a video file with Go Live.
`testing/fake_discord_env.ts` defines the format as TypeScript types, and
`testing/envs/example.json` uses every part of it:

```sh
scripts/ui-session start --discord --fake-args '--env testing/envs/example.json'
```

Media paths are relative to the environment file. Videos need `ffprobe` and
`ffmpeg`. A streamed video is converted once to the fake's stream formats,
and the result is cached in the system's temporary directory. Without
`--env`, the fake serves its built-in content unchanged, and the tests rely
on it.
`YAPFAST_DISCORD_TEST_UNLINKED=1` starts Yapfast unlinked, to link against the fake.
[Network memory](measurements/README.md) compares the three builds.

With [direnv](https://direnv.net/), the repository's `.envrc` loads the Nix
development shell when you enter the folder, so commands need no
`nix develop` prefix. On NixOS, `programs.direnv.enable = true;` installs direnv
with nix-direnv, which caches the shell.

## Windows

The Linux cross-build and portable ZIP route is documented in the
[Windows beta guide](windows-beta.md). It supplies the matching Windows WebRTC
archive through the Windows Nix shell. The executable uses static CRT linkage
to match LiveKit.
`scripts/build-windows [discord|full] [options]` builds the Discord-only release,
the default, or the full preview with Matrix, and passes the options to
`scripts/package-windows.py`, such as `--no-installer`.
`scripts/package-windows.py`, which `scripts/build-windows` runs, makes the
flavour's portable ZIP and compiles the installer, `packaging/windows/yapfast.iss`, with
Inno Setup 7.1.0 under Wine. The first run downloads Inno Setup's installer,
pinned by SHA-256, and installs it into its own Wine prefix in
`.local/windows/inno-setup`, under `xvfb-run`, since it opens windows even
when silent. `--no-installer` makes only the ZIP.

Binaries people download name no folder of the machine that built them.
Rust embeds source paths for panic messages, and C's `__FILE__` reaches
AWS-LC's errors and asserts, so a plain build names its builder's home folder
thousands of times, through `~/.cargo`. `scripts/release-env TARGET`, sourced
before `cargo build --target TARGET`, maps the home folder, Cargo's folder and
the repository to `/home`, `/cargo` and `/yapfast` with `--remap-path-prefix`
and Clang's `-ffile-prefix-map`. On Windows it takes `cargo xwin env`'s
environment and adds the C flags to it, since `cargo xwin build` sets the
`CFLAGS_<target>` the `cc` crate reads first; `scripts/build-windows` uses it.
It also gives bindgen the real target: thorvg-sys's build script passes
`--target=x86_64-none-elf` on every cross build, which can't read the MSVC
headers.
`scripts/package-windows.py` refuses an executable holding the home folder's
path in UTF-8 or UTF-16. A Linux build from the dev shell needs the same, and
its linker also writes a RUNPATH of `$PWD/outputs/out/lib` and Nix store
folders into the binary; the [AppImage](packaging.md#linux-appimage) avoids both by
building in the Nix sandbox.

The supported and packaged build route remains the Linux MSVC cross-build.
A native Windows build is for development only: it names this machine's folders
and must not be shipped. It needs Rust 1.97.1, Visual Studio C++ tools and the
Windows 10/11 SDK, LLVM with `clang-cl`, `llvm-rc` and `libclang` on `PATH`
(the SDK's `rc.exe` can replace `llvm-rc`), and NASM on `PATH`. bindgen, which
`thorvg-sys` runs, finds `libclang` only through `LIBCLANG_PATH`, `llvm-config
--prefix` or `LIBRARY_PATH`, never `PATH`, so set `LIBCLANG_PATH` to LLVM's
`bin` where `llvm-config` is not on `PATH`. A network connection is needed
once for the libwebrtc archive.
`vendor/ffmpeg-src/build.rs` and `crates/yapfast-core/build.rs` select
`clang-cl` for MSVC targets only when `CC`/`CXX` are unset, because FFmpeg's x86
assembly and the camera shim's Clang warning flags need it; an explicitly
selected compiler wins.

```powershell
cargo build --release
```

The downloadable package needs none of these tools to run.

Before accepting the Windows build, check text entry, selection, copy/paste,
keyboard focus, navigation, both renderers, resizing, normal close, and reopen
on the tester's actual GPU.

## Logs

Yapfast logs only its own operational events at info level and above. Dependency
logs are disabled on purpose, because they can expose authentication data or
message content. Yapfast ignores `RUST_LOG`; `YAPFAST_LOG=debug` turns on Yapfast's own
debug logs (`crates/yapfast-core/src/logging.rs`).
Typed text never enters the log. Future errors must use fixed event
names and safe classifications rather than raw protocol responses or SDK Debug
output.

## Initial resource measurement

After a `measure` build with the release's features (see
[Networks](#networks)), open `target/measure/yapfast` inside the Nix shell;
measure `release` only for the shipped build's absolute numbers. Leave it
inactive for at least 15 seconds, then sample its PID on Linux.
`scripts/ui-session` writes the PID of the Yapfast it runs to
`.local/ui-session/yapfast.pid`.

```sh
scripts/measure-process PID --seconds 30
```

The JSON reports mean and peak RSS, mean PSS, CPU use relative to one core,
and GPU engine counters when the driver exposes them through `/proc/PID/fdinfo`.
An unavailable GPU metric is reported explicitly. Record hardware, account and
room counts, renderer, build mode, warm-up, window size and activity alongside
the output.

## Local backend

Synapse and the authorization service communicate over an internal TLS listener
using a throwaway self-signed certificate. LiveKit and authorization share a
network namespace, and use the same signaling port inside and outside Docker.
Authorization uses the standalone `/get_token` mode from lk-jwt-service 0.7.0.
Yapfast's native experiment sets the local authorization fallback explicitly,
because this stack does not advertise a production RTC discovery endpoint.
See [sessions](sessions.md) for credential storage and headless sign-in.

The service only allows the local `synapse` homeserver to create SFU rooms. This
does not establish a production friend-group or room-membership access policy.
That policy remains part of the encrypted calling experiment.
