# Native builds

## Shells

Three Nix shells cover the work:

- `nix develop path:./nix` is the development shell. Run every `cargo` command
  in it, as `nix develop path:./nix --command cargo ...`. It also provides
  `bun` for the fake Discord.
- `nix-shell --impure testing/display/shell.nix` adds Weston,
  PulseAudio, the ALSA plugins and FFmpeg to it, for the private display that
  `scripts/run-framework-isolated` starts.
- `nix develop path:./nix#windows` cross-builds for Windows; see the
  [Windows beta guide](windows-beta.md).

## Dependencies

Yapfast pins Rust 1.97.1, Slint 1.18.1 and Skia 0.153.3. Cargo.lock pins
the complete Rust graph. `vendor/i-slint-renderer-skia` is Slint's Skia
renderer 1.18.1 with four changes. Glyphs use Skia's slight (vertical-only)
hinting, because the default hinting spaced Satoshi's letters unevenly.
Smooth pictures up to 512×512 pixels sample mipmaps, because plain bilinear
sampling made pictures drawn smaller than their pixels, such as avatars, look
jagged. Larger ones, such as video frames, go without: each frame would build
its mipmaps anew, 4 ms more for a 1080p frame. So do pictures drawn no
smaller than their pixels, such as a call's camera, made at its tile's size.
Skia reads fonts where fontique keeps them rather than copying each file to
the heap, which took 10 MiB for the colour emoji font. It reads a picture made
in code, such as a camera's frame or an avatar, where Slint keeps it too,
rather than copying it for each new frame: a table on the UI thread keeps the
Slint buffer beside Skia's image until nothing else holds that image, and
Slint copies a buffer it shares before anything writes to it. Each change is
marked `Yapfast:` in `itemrenderer.rs`, `cached_image.rs`, `lib.rs` or
`font_cache.rs`. Copy the matching release over it, and reapply them, when
upgrading Slint.

`vendor/i-slint-core` is Slint's core 1.18.1 with `StyledText` made selectable,
which upstream Slint doesn't offer. Dragging selects, a double press selects a
word and a triple press a paragraph. Dragging past a text carries the selection
on through the texts and pictures around it: those sharing its nearest flickable,
such as a list's rows, in reading order. Pictures are the `Image`s with
an `accessible-label`, and copy as `[Picture]`. Ctrl+C copies the selection
as read, without its Markdown: texts level with each other joined by a space, the
rest by line breaks. Yapfast shows message authors and times as `StyledText` so the
selection takes them in. The selection is kept as positions in the list's model,
so it survives its rows scrolling away. Dragging near the top or bottom of the
list scrolls it, and Yapfast supplies the text of the rows in between through
`yapfast_selection::set_row_text`. A click that selects nothing still follows a
link and leaves the focus where it was. A left press anywhere clears the
selection, as does Escape. The highlight is the text's `link-color` at 35%
opacity, since `StyledText` has no property of its own for it.

The core of it is `yapfast_selection.rs`. The other changes are marked `Yapfast:` in:

- `items/text.rs`
- `items/image.rs`, which tints covered pictures
- `items/flickable.rs`, which keeps the pointer through a drag
- `window.rs`, for the press, Ctrl+C and Escape
- `item_rendering.rs`
- `textlayout/sharedparley.rs`
- `textlayout/sharedparley/shaping.rs`
- `textlayout/sharedparley/draw.rs`, which draws mention pills under their
  text and opaque covers over hidden Discord spoilers, links to `yapfast:spoiler/`
- `graphics/image.rs`, where no image equals no image; upstream they differ,
  so every list row without a picture counted as changed on each refresh

It also keeps a timeline's rows in place as older messages go in above them.
Upstream, a `ListView` keeps its rows where they are on screen when rows go in
above the first one it shows, but rows that go in right above it, as when the
top of the history is on screen, push everything shown down. Inside
`yapfast_anchor::hold`, which Yapfast wraps around a page of older messages, those
count as above it too. That row then keeps its bottom edge where it is in the
view, rather than its top, while its height settles: it loses its date, its
author line or the loading notice once the rows before it carry them, and
those parts change only when the list is next drawn. The anchor moves with
scrolling, follows its row as rows go in or out above it, and ends when the
row is no longer shown or goes. A wheel scroll animates
`content-y` and would carry on to where it was going, so the list stops that
animation when rows go in above inside `hold`. This is `yapfast_anchor.rs` and the
changes marked `Yapfast:` in `model/repeater.rs`.

A mouse wheel spun fast scrolls farther for each notch. Upstream, each notch
scrolls 60 pixels however fast the notches come, so a hard spin through a long
history crawls. Yapfast weighs the recent notches, each counting less with time,
and above 12 notches a second scales each notch by the speed over 12, at most
six times. Deltas smaller than a notch, as from touchpads that send no phases,
keep their pace. These are the changes marked `Yapfast:` around
`accelerate_wheel` in `items/flickable.rs`.

Reapply them to the matching release when upgrading Slint. Styled paragraphs now
have byte ranges as if joined by newlines, where upstream gives them all `0..0`.

`ui/scrollview.slint` is Yapfast's own `ScrollView` and `ListView`, in place of
the std-widgets ones: the Fluent style's from Slint 1.18.1, with its scroll
arrows copied to `ui/icons/scroll-*.svg`. The Fluent scrollbar starts a thumb
drag over each time `maximum` changes, dropping part of the pointer's travel,
and a `ListView`'s `maximum` changes all through a drag as rows are measured,
so the thumb fell behind the pointer. Yapfast's bar sets the viewport from where
the pointer has moved the thumb since the drag began, as a share of the track,
and again when `maximum` changes. The compiler virtualizes any component named
`ListView` with the viewport's lengths, so Yapfast's is virtualized as upstream's
is. Compare it with the Fluent style's when upgrading Slint.

The Matrix SDK revision follows the pinned MatrixRTC revision. Keep those two
sources together when upgrading.

libDF, DeepFilterNet's Rust implementation, comes from DeepFilterNet's
repository at `d375b2d`, since crates.io has only its 0.2.5. It asks for
tract `^0.21.4`, but tract 0.21.10 changed an API it uses, so Cargo.lock keeps
tract at 0.21.4, the version DeepFilterNet's own lockfile has.

LiveKit's Rust crates must stay on the mutually compatible versions in Cargo.lock.
Do not run an unrestricted `cargo update`. Upgrade against the MatrixRTC lockfile
and rebuild the native media stack. On Linux, the Nix shell supplies libwebrtc
`webrtc-51ef663`, matching webrtc-sys-build 0.3.18. Updating that crate requires
updating the archive URL and checksum in nix/flake.nix too, and the macOS
archive's in scripts/build-macos.

The Linux environment includes ALSA, PulseAudio, fontconfig, FreeType, GLib, Wayland, X11,
Vulkan, OpenGL, OpenSSL, Clang 21, CMake, pkg-config and Protobuf. It uses the
host's graphics driver under `/run/opengl-driver/lib`. A real display session
are required to open the window. Skia Software is the default; `--renderer gpu` uses Skia GPU and falls back to
Skia Software if the GPU fails. The Nix shell provides a checksum-pinned,
prepared Skia archive whose JPEG table is privately named to avoid a WebRTC
symbol collision. scripts/build-macos pins macOS's Skia archive by checksum
too, so upgrading Skia updates both. See [the Slint client](slint-client.md).

The first build downloads dependencies and compiles native C++. Cargo uses every
core. On a machine with 8 GB of memory, set `CARGO_BUILD_JOBS=2` so the Matrix
SDK and the C++ media bindings do not exhaust it.

Development builds omit debug symbols for dependencies; the app keeps line
tables. They optimize DeepFilterNet's crates and turn off tract-core's debug
assertions, which reject its model; see Cargo.toml. Release builds strip
symbols and use no LTO, which made both platforms' binaries bigger for no
measured speed. Use them for builds people run. The `fast` profile optimizes
like release, keeps symbols and builds incrementally. Use it for tests, local runs and
[measurements](measurements/README.md): `cargo build --profile fast` or
`cargo test --profile fast`.

### Dependency sources

Yapfast patches these crates in `vendor/`, through `[patch.crates-io]` or a path
dependency in Cargo.toml:

- `i-slint-core`: Slint's core 1.18.1 with selectable `StyledText`,
  timelines that keep their place as older messages load, and a wheel that
  speeds up when spun fast, above.
- `i-slint-renderer-skia`: Slint's Skia renderer 1.18.1 with slight hinting,
  mipmapped pictures, and uncopied fonts and pictures, above, and a whole
  frame after every resize: Windows restores a minimized window at its old
  size without its pixels, which partial rendering left black.
- `matrix-rtc-livekit`: Element's matrix-rust-rtc at `5831af8`, with native
  audio tracks and cancellable joins.
- `webrtc-sys`: webrtc-sys 0.3.35 with a Windows link fix, capture leveling,
  remote track volume and camera capture from pooled buffers.
- `davey`: davey 0.1.4, keeping its key packages when a Go Live stream's group
  id changes, and reading a frame's overlong nonce without panicking.
- `tungstenite`: tungstenite 0.26.2, giving back the read buffer a large frame
  such as Discord's READY grew.
- `gpui-pre-windows`: a Windows build fix for the archived GPUI client. Nothing
  uses it now.

Each has a `README.yapfast.md` with its changes, except the two Slint crates, which
this section describes. The Slint crates keep their `.rs` files at the crate
root, with no `src/`, both in `vendor/` and in Cargo's registry.

Crates that aren't vendored are in
`~/.cargo/registry/src/index.crates.io-*/NAME-VERSION/`. The Matrix SDK is a git
dependency; its checkout is `~/.cargo/git/checkouts/matrix-rust-sdk-*/c6ad52e/`,
the short form of the revision pinned in Cargo.toml. libDF's is
`~/.cargo/git/checkouts/deepfilternet-*/d375b2d/libDF/`. libwebrtc's headers are in
`$LK_CUSTOM_WEBRTC/include` inside the development shell.

## Checks

`scripts/check` enters the development shell itself. `scripts/check --fast`
builds with the `fast` profile, as local work does. It runs, in order:
formatting; `scripts/check-doc-links`, which fails on a Markdown link to a
missing file or heading; Clippy with warnings denied and the tests for the whole workspace;
a build; and the desktop smoke test, which runs `target/<profile>/yapfast`, the
desktop build, from that build. Alongside those, in
`target/check-combinations/` because Cargo lets one build at a time use a
target directory, it runs Clippy for the Discord-only, Matrix-only and headless
builds and the headless smoke test, whose binary stays there. Each step prints `=== step` and `rc=N` when it ends,
with Rust's errors when it fails, and keeps its full log under `target/check/`.
The script runs every step and exits non-zero if any failed.

A new worktree starts with an empty `target/`, and its first check compiles
every dependency. `scripts/warm-worktree`, run inside it
before the first build, copies the main checkout's `target/fast/` and
`target/check-combinations/` as reflinks, which share disk space with the
originals, and cleans the crates whose sources are in the repository, so those
build from the worktree's own files. The first check then takes about 3.5
minutes. Reflinks need a filesystem such as btrfs.

`git config core.hooksPath .githooks` turns on the pre-commit hook, which
checks formatting only, so commits stay quick.

## Seeing the app

`scripts/ui-session` runs Yapfast on a private Weston display, for screenshots and
checks by hand, without touching the desktop or a real account:

```sh
scripts/ui-session start      # the preview; --discord for the fake Discord
scripts/ui-session do 'click 40 100' 'type hello' 'key enter' 'shot sent'
scripts/ui-session relaunch   # rebuild and restart Yapfast on the same display
scripts/ui-session stop
```

Screenshots go to `.local/ui-session/shots/`, and `scripts/ui-session --help`
lists every option and command. `start --tray` adds a stand-in tray on the
session's bus; see [the tray](tray.md#testing). The scripted probes are in
[the Slint client's validation notes](slint-client.md#validation-and-limits).

For demo footage, the session records its display at 60 frames a second:

```sh
scripts/ui-session start --discord --size 1920x1080 --zoom 1.2 --cursor 32
scripts/ui-session do 'record start tour' 'glide 400 600' 'click 170 264 glide' \
  'glide 1000 500' 'scroll -1200 1.5' 'sleep 1' 'record stop'
```

`testing/record_host.rs` takes the frames through Weston's capture protocol
and ffmpeg encodes them to `.local/ui-session/videos/tour.mp4`, which later
starts keep. `--zoom` opens Yapfast at that zoom level, `--cursor` shows the
pointer, `glide` and `click … glide` move it as a hand would, and `scroll
PIXELS SECONDS` scrolls smoothly. Weston's headless display repaints about
58.5 times a second at most, so about one frame in 40 repeats the one
before.

## Networks

Matrix and Discord are Cargo features, `matrix` and `discord`, and the default
build has both. A build can leave either one out:

```sh
cargo build --release --no-default-features --features desktop,discord  # Discord only
cargo build --release --no-default-features --features desktop,matrix   # Matrix only
```

A Discord-only build signs in by linking Discord by QR code, and connects with
the account linked on the device from then on. Voice calls
still need LiveKit's libwebrtc, which provides the audio processing and the
device list for both networks. The `--no-default-features` build without either
network only opens `--preview`.

`YAPFAST_DISCORD_TEST_SERVER=http://127.0.0.1:18400` points Discord at
`testing/fake_discord.ts` (`bun testing/fake_discord.ts --port 18400`). It uses a
test token instead of the linked account, so the real token never reaches it.
`scripts/test-fake-discord [FILTER]` starts the fake on a free port, waits until
it answers, runs the ignored tests in `crates/yapfast-discord/tests/fake_server.rs`
with the `fast` profile, and stops it. It then runs
`crates/yapfast-discord/tests/fake_env.rs` against a second fake that runs the
example environment.

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
archive and checksum-pinned Skia source through the Windows Nix shell.
`scripts/build-windows` prepares and caches a static-runtime Skia build before
compiling Yapfast. The executable uses static CRT linkage to match LiveKit.
`scripts/build-windows [discord|full]` builds the Discord-only release, the
default, or the full preview with Matrix.
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
`scripts/package-windows.py` refuses an executable holding the home folder's
path in UTF-8 or UTF-16. A Linux release needs the same, and the dev shell's
linker also writes a RUNPATH of `$PWD/outputs/out/lib` and Nix store folders
into the binary, which a package for other distributions has to replace.
GPUI's FXC shader build is no longer used.

The supported build route for this migration is the Linux MSVC cross-build.
Native Windows development needs Rustup, Visual Studio C++ tools and SDK, CMake,
LLVM and Protobuf, plus the same prepared Skia archive through
`SKIA_BINARIES_URL`. Opus comes from the libopus in libwebrtc's archive.
That native-host preparation/build route has not been checked.
The downloadable package needs none of these tools to run.

Before accepting the Windows build, check text entry, selection, copy/paste,
keyboard focus, navigation, both renderers, resizing, normal close, and reopen
on the tester's actual GPU.
Windows execution has not been demonstrated by a Linux build.

## macOS

`scripts/build-macos [discord|full]` builds on an Apple silicon Mac, outside
Nix, and packages `Yapfast.app` and a disk image into `target/dist/FLAVOUR`. It
fetches the native inputs the Nix shells would pin, checked against their
SHA-256, into `.local/macos`. The [macOS guide](macos.md) covers what it
fetches and why, running the app, and what the port lacks so far.

## The app icon

`packaging/icon.svg` is Yapfast's icon: the logo in marigold, its dots in
on-accent ink, so it reads on a light taskbar as well as a dark one. The UI
keeps drawing `ui/icons/logo.svg`.

On Linux the window's app ID is `io.github.yapfast.Yapfast` (`src/icon.rs`).
Wayland desktops show a window's icon only through the desktop entry of that
name, `packaging/linux/io.github.yapfast.Yapfast.desktop`;
`scripts/install-desktop-entry [EXECUTABLE]` installs it and the icon for
this user, running `target/release/yapfast` unless told otherwise. On X11 the
window also carries the icon itself. Notifications name the entry, so the
notification server shows the same name and icon.

On Windows, `build.rs` compiles `packaging/windows/yapfast.rc` with `llvm-rc`
and links the icon into `yapfast.exe`, for Explorer and shortcuts; the window
loads it from there for its title bar and the taskbar, each at the size
Windows draws it. After changing `icon.svg`, run `scripts/make-windows-icon`
to draw `yapfast.ico` again.

On macOS, `Yapfast.app` names `packaging/macos/yapfast.icns` in its
`Info.plist`, for Finder and the Dock. `scripts/make-macos-icon` draws it from
`icon.svg` with a margin, as macOS icons have, on a Mac with resvg installed.

## Logs

Yapfast logs only its own operational events at info level and above. Dependency
logs are disabled on purpose, because they can expose authentication data or
message content. Yapfast ignores `RUST_LOG`; `YAPFAST_LOG=debug` turns on Yapfast's own
debug logs (`crates/yapfast-core/src/logging.rs`).
Typed text never enters the log. Future errors must use fixed event
names and safe classifications rather than raw protocol responses or SDK Debug
output.

## Initial resource measurement

After a `fast` build, open `target/fast/yapfast` inside the Nix shell; measure
`release` only for the shipped build's absolute numbers. Leave it inactive for
at least 15 seconds, then sample its PID on Linux. `scripts/ui-session` writes
the PID of the Yapfast it runs to `.local/ui-session/yapfast.pid`.

```sh
scripts/measure-process PID --seconds 30
```

The JSON reports mean and peak RSS, mean PSS, CPU use relative to one core,
and GPU engine counters when the driver exposes them through `/proc/PID/fdinfo`.
An unavailable GPU metric is reported explicitly. Record hardware, account and
room counts, renderer, build mode, warm-up, window size and activity alongside
the output. The earlier empty GPUI build-check baseline had no account or sync;
it does not measure the current connected Slint client.

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
