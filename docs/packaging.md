# Packaging and releases

How the Linux and Windows releases are made and published. Building and
checking Yapfast for development is in [building](building.md).

## Linux AppImage

`scripts/build-appimage` makes the Linux release in `target/dist/linux`:
`Yapfast-VERSION-x86_64.AppImage`, its `.zsync` file, the source snapshot and
`SHA256SUMS.txt`. All four go on the release page. AppImageUpdate, and AppImage
managers such as Gear Lever, find a newer release through the `.zsync` file,
named by the update information in the runtime:
`gh-releases-zsync|yapfast|yapfast|latest|Yapfast-*-x86_64.AppImage.zsync`.
The first build compiles every dependency in the Nix sandbox; packing takes
about 10 seconds after that.

Nix builds the binary, `nix/package.nix`'s Discord-only release build, and
`nix/appimage.nix` puts it in an AppDir with the desktop file, the icons and
the AppStream metainfo, `packaging/linux/io.github.yapfast.Yapfast.metainfo.xml`.
It sets the binary's interpreter to `/lib64/ld-linux-x86-64.so.2` and removes
Nix's RUNPATH, so the binary runs on the host's loader, glibc and libstdc++, as
one built on a distribution would. That's what lets the host's GPU driver,
Mesa or NVIDIA's, load into it: a driver needs the glibc it was built for, and
NVIDIA's must match the kernel module, so neither can come in the AppImage.
Nothing else is bundled either. Besides the C and C++ runtimes, the binary
links only GLib, PulseAudio's client, fontconfig and FreeType, which every
desktop has, and it loads EGL, Vulkan, Wayland, X11 and PipeWire at runtime.

The derivation fails if the binary links anything else, or asks for a glibc
newer than 2.39 or a libstdc++ newer than GCC 14's. That covers Ubuntu 24.04,
Fedora 40, Debian 13 and anything newer. nixpkgs' glibc is newer than that, so
a nixpkgs update can raise what the binary asks for. Raise the floors in
`nix/appimage.nix` only knowing which distributions that drops. Older ones,
such as Ubuntu 22.04, would need a build against their own glibc.

`scripts/package-linux.py`, which runs in the development shell, with
squashfs-tools and zsync added from the shell's nixpkgs, adds the
licenses and notices, `THIRD-PARTY.json`, `SOURCE.txt` and `BUILD.json` to
`usr/share/doc/yapfast`, as the Windows package has them, through the helpers
in `scripts/release_package.py` that both packaging scripts use. Skia's
prebuilt Linux archive carries only Skia's own license, so the notices of its
third-party code come from the pinned Skia source,
`nix build path:./nix#skia-source`. Then it packs the AppDir with mksquashfs
(zstd level 19, 1 MiB blocks, fixed owners and times, so the same AppDir packs
to the same file) behind AppImage's official type-2 runtime, pinned by hash.
The runtime is static: it mounts the image with FUSE 3's `fusermount3` and
needs no libfuse, and `--appimage-extract-and-run` runs it without FUSE at all.
The flake sees only the files Git tracks, so add a new file under `nix/` or
`packaging/` before building.

Screen sharing hasn't been tried from the AppImage.

## Arch Linux packages

`packaging/arch/PKGBUILD` builds the Discord-only release from source with
Arch's toolchain, as `nix/package.nix` builds it: `cd packaging/arch && makepkg
-si`. It clones the tag of its `pkgver`, which `scripts/set-version` sets to a
stable version, so run `scripts/set-version 0.2.0` before committing and
tagging `v0.2.0`. The tag's commit holds the PKGBUILD, which can't hold its own
checksum, so that source has none; the libwebrtc and Skia archives keep theirs.
Arch's `llvm21` is a later 21.x than Nix's, and a native Windows host's LLVM
is often newer still, which is why `nix/prepare-skia-native.py` takes LLVM 21
or later. Nothing builds this PKGBUILD automatically: build it in a clean
`archlinux:base-devel` container before a release, about 6 minutes from cold.

The releases carry a package of their own binary instead. `scripts/package-arch`,
run after `scripts/build-appimage`, packs the AppImage's binary, desktop file,
icon, metainfo and notices into `yapfast-VERSION-1-x86_64.pkg.tar.zst` beside
it, through `packaging/arch/release.PKGBUILD`, and adds it to
`SHA256SUMS.txt`. It's the AppImage's binary, which runs on the host's loader
and libraries, so the two packages can't differ. The script runs makepkg in an
`archlinux:base-devel` container with Docker, then installs the package there
and runs `yapfast --smoke-test`, in about 20 seconds. A nightly's version
becomes `0.2.0nightly.20261020.5`, which pacman sorts after 0.1.0 and before
0.2.0, as SemVer sorts the nightly; with a dot before `nightly`, pacman would
sort it after 0.2.0. `sudo pacman -U` installs the file or its link; there's no
repository, so `pacman -Syu` doesn't update it.

## Nightly releases

Releases are built locally, but a nightly of `main` builds on GitHub: Actions,
Nightly, Run workflow. `.github/workflows/nightly.yml` names it after the next
minor version, the day (UTC) and the workflow's run number, such as
`v0.2.0-nightly.20261020.5`, and titles it as the stable releases are, with the
commit: `Yapfast 0.2.0-nightly.20261020.5 (396abe2)`. The run number puts two
nightlies of one day in order, as a commit hash wouldn't, and gives a second
run on one commit a tag of its own. `scripts/set-version` writes that version
into `Cargo.toml` and `Cargo.lock` before building, so the app reports it and
the packages and `SOURCE.txt` name it. One job runs `scripts/build-appimage`
and `scripts/package-arch`, and another `scripts/build-windows` in the Windows shell, with the Windows Skia
build, cargo-xwin's MSVC files and the compiled Rust dependencies
(`Swatinem/rust-cache`) cached between runs. The Linux build has no cache: it
compiles in the Nix sandbox, which can't reach one. The last job publishes the
AppImage, its `.zsync` file, the Arch package, the Windows ZIP, the source snapshot and
`SHA256SUMS.txt` as a prerelease on that commit. Wine doesn't start on the
runner, so there's no installer. The release's notes list the commits since the
previous nightly, or the latest release, each with its author's GitHub account:
features and fixes by their Conventional Commit type, the rest folded away
(`.github/nightly-notes.jq`). Every nightly stays on the releases page. Both
builds zip the source snapshot with the commit's time on every file, so they
make the same file, and the release carries one. A prerelease is never
`latest`, so the AppImage's update information leaves stable installs on stable
releases, and takes a nightly AppImage to the latest stable release, not the
next nightly.

## Which build is running

Settings shows which build it's in, in two lines under its sections, and
`yapfast --version` prints it, so a report says at once whether it comes from
a stable release, a nightly or a build from source, and whether that has a fix
or a feature. A click on the lines copies them as one line for a bug report:

```
Yapfast v0.4.0-nightly.20261107.12 · Nightly · 59391f96 · 2026-11-07
```

The channel is Stable, Nightly, Pre-release (a release whose version has some
other prerelease part, such as `0.5.0-rc.1`) or Development build, and the
commit and its day, in UTC, follow. A development build's first line names its
commit, not the version of the release it grew from. `src/build_info.rs` makes
the text, and `build.rs` gives it the commit.

A build is a release only when its packaging says so. `Cargo.toml` holds a
stable release's version between releases too, so the version alone would call
every build from source a stable release. `scripts/build-windows`, the
`release` package in the flake (which the AppImage holds) and the Arch
PKGBUILD set `YAPFAST_RELEASE`; a plain `cargo run` doesn't, and is a
development build. A nightly says what it is in its version, which only the
Nightly workflow sets.

The commit comes from Git where the build runs in a checkout of this
repository. Nix's sandbox has no Git, so the flake gives it `self.rev` as
`YAPFAST_COMMIT`, and the commit's time as `YAPFAST_COMMIT_DATE`; a release
built after `scripts/set-version` has a dirty tree, and the flake's
`dirtyRev` names its commit as well. This makes Nix rebuild the program for a
new commit even when no file it reads changed. A build outside a repository,
such as a source archive, names no commit. A build doesn't look for edits to
files on top of the commit, so a development build can't say it has them.

## Newer releases

Settings' About page shows the build, as the lines under the sections do, and
says whether a newer release is out. A line under the sections tells too:
"Update available" with the release's tag, and a click opens About. About
shows what's new in that release (see below) and offers Update and restart
(when this copy can replace itself), View release, which opens the release's
page in the browser, and Skip this version, which hides that release until a
newer one comes. It also links to the website, the GitHub project and the
Discord server, each button with its mark.

The look is off until a person turns on Check for updates, so a fresh install
makes no request to GitHub. On, Yapfast looks when it starts and every six
hours while it runs, with one request for the list of releases
(`api.github.com/repos/yapfast/yapfast/releases`) that carries only
`Yapfast/VERSION` as its user agent and no account or identifier. A look nobody
asked for tells of a newer release in a notification, once for each release
(`announced_update` in `layout.json`), unless notifications are off or the
release was skipped; clicking it opens About. Check now looks once, whatever
the switch says, and doesn't notify. Nightlies count as newer only with
Include nightly builds, so a stable install isn't offered one. A build from
source is never offered an update: its version is that of the release it grew
from, so every later release would look new. A nightly is offered the stable
release that follows it.

`src/update.rs` compares versions as SemVer does and decides from the list
what to offer, in functions the tests call without a network;
`src/ui/updates.rs` keeps the last list for the session, looks on its timer and
fills the `Updates` global. The saved choices are `check_for_updates`,
`update_nightlies` and `skipped_update` in `layout.json`.

It also works out how Yapfast was installed, from the program's path and the
`APPIMAGE` and `FLATPAK_ID` variables: Scoop, Chocolatey, WinGet, the Microsoft
Store, Nix, Snap, Homebrew, Flatpak and a distribution's `/usr` are managed, and
About then says to update through that package manager.

### What's new

The text comes from the release's own description on GitHub, which the release
workflow writes: for a stable release a summary and a "What's new" list, for a
nightly the commits since the previous nightly. `update::notes_of` makes plain
lines of it: it leaves out the Install section, the folded "Other changes", the
link to the full comparison and the footer, and it takes the `feat(scope):`
before a commit line and the `by @name in <sha>` after it off. Only the newest
stable release and the newest nightly keep their text in memory. The release's
page has the rest.

### Updating itself

Update and restart is shown only when this copy can replace itself, and
installs only what a person asked for: there is no automatic install, and
About's "Install updates automatically" switch stays greyed out and off.
`src/update/install.rs` does the work, for two installs:

- **A Linux AppImage** that the person can write to. The release's AppImage
  streams to `.yapfast-update.part` beside the old one, hashed as it arrives.
  Unless its SHA-256 matches the release's `SHA256SUMS.txt` it is deleted. It
  then runs once with `--smoke-test` (a minute at most), so a build that
  can't start on this computer never replaces one that can. It is renamed
  over the old file, which is atomic on one filesystem, and Yapfast
  restarts into that path. The running program keeps the old file open
  until then.
- **Windows' installer**, when `unins000.exe` is beside the program (the
  installer wrote it) and the folder can be written to. The release's
  `Yapfast-VERSION-windows-x64-setup.exe` is downloaded to
  `%TEMP%\yapfast-update` and checked the same way. Yapfast quits, and a
  hidden `cmd` runs the installer with `/SILENT /SUPPRESSMSGBOXES /NORESTART
  /CLOSEAPPLICATIONS`, waits for it, starts Yapfast again and deletes the
  installer; the installer's log stays beside it. A silent run of the
  installer starts nothing itself, so that `cmd` does. A path that holds a `%`
  or `"` is refused, as `cmd` would read it as more than a name. If `cmd`
  can't start, the old Yapfast starts again.

Everything else is left alone, with the reason on the page: a package manager's
or Flatpak's copy (it updates it), a portable ZIP or a build of one's own, a
release without the file for this install (nightlies carry no Windows
installer, so a Windows install is offered View release for them), a release
without `SHA256SUMS.txt`, and a folder that can't be written to. Any failure
(no network, a sum that doesn't match, a failed start-up check) is shown on the
page with the old Yapfast still running and the downloaded file removed.
Cancel stops a download that is still going. The files come from GitHub over
HTTPS, and the sums from the same release: they protect against a broken
or cut-off download, not against GitHub itself.

The window ends with `ui::Exit::Update`, and `main::finish_update` finishes
it once the window, the tray icon and the session are gone. Test it with
`cargo test --features desktop,discord update`: the tests serve files from a
local HTTP server, and on Unix start a script as the AppImage.

### New tags

A setting added by an update shows a NEW tag until the person has been on its
page, and so does its section's entry in the list (`src/ui/new_tags.rs`).
Each tagged setting is listed there with the release it first shipped in.
A setting is new when that release is newer than the version the person's
`layout.json` counts as having met (`first_version`) and they haven't left its
section since; leaving the section or closing Settings saves it as seen
(`seen_new`). A fresh install has met every setting that exists, so it sees no
tags until an update brings a setting; a layout from before the tags existed
has met 0.3.0, so everything tagged since shows. Updating over several
releases without opening Settings shows what each one added.

To tag the next new setting, add its key, section and release to the list and
give its row `new-key: "<key>"`; a test fails if the list and the `.slint` files
disagree. Remove an entry once its release is old.

## The app icon

`packaging/icon.svg` is Yapfast's icon: the logo in marigold, its dots in
on-accent ink, so it reads on a light taskbar as well as a dark one. The UI
keeps drawing `ui/icons/logo.svg`.

On Linux the window's app ID is `io.github.yapfast.Yapfast` (`src/icon.rs`).
Wayland desktops show a window's icon only through the desktop entry of that
name, `packaging/linux/io.github.yapfast.Yapfast.desktop`;
`scripts/install-desktop-entry [EXECUTABLE]` installs it and the icon for
this user, running `target/release/yapfast` unless told otherwise. The
AppImage installs nothing, so Yapfast run from one writes the entry and the
icon to `~/.local/share` itself before it opens its window
(`src/desktop_entry.rs`), with Exec and TryExec naming the AppImage. It
rewrites them when the AppImage has moved, and leaves them alone when a package
has its own entry in `XDG_DATA_DIRS`, or an AppImage manager such as Gear Lever
made one that runs the same file. On X11 the window also carries the icon
itself. Notifications name the entry, so the notification server shows the
same name and icon.

On Windows, `build.rs` compiles `packaging/windows/yapfast.rc` with `llvm-rc`,
or the Windows SDK's `rc.exe` where there is none,
and links the icon into `yapfast.exe`, for Explorer and shortcuts; the window
loads it from there for its title bar and the taskbar, each at the size
Windows draws it. After changing `icon.svg`, run `scripts/make-windows-icon`
to draw `yapfast.ico` again.
