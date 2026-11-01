#!/usr/bin/env python3
"""Package the Windows executable, build inputs and dependency notices, as a
portable ZIP and an Inno Setup installer.

The Discord flavour is the release; the full one, with Matrix, goes out as a
separate preview. Both install as the same app, so one replaces the other."""
import argparse
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import urllib.parse

from release_package import ROOT, dependency_notices, digest, names_home, source_snapshot, zip_directory

TARGET = "x86_64-pc-windows-msvc"
# Inno Setup's compiler, run under Wine; installed once into its own prefix.
INNO_SETUP_VERSION = "7.1.0"
INNO_SETUP_URL = ("https://github.com/jrsoftware/issrc/releases/download/is-7_1_0/"
                  f"innosetup-{INNO_SETUP_VERSION}-x64.exe")
INNO_SETUP_SHA256 = "0362a383ed217d4c4239b5933866dd96d3eb2102737da92f80f6057a4b40df2f"
# Cargo features, the suffix of file names and release tag, and the name the
# installer shows, for each flavour scripts/build-windows builds.
FLAVOURS = {
    "discord": {"features": ["--no-default-features", "--features", "desktop,discord"], "suffix": "", "edition": None},
    "full": {"features": ["--features", "matrix"], "suffix": "-full-preview", "edition": "full preview"},
}


def wine_environment(prefix):
    environment = dict(os.environ, WINEPREFIX=str(prefix), WINEDEBUG="-all")
    # The compiler is a console program; Wine opens no window for it.
    environment.pop("DISPLAY", None)
    environment.pop("WAYLAND_DISPLAY", None)
    return environment


def inno_setup():
    """The Wine prefix holding Inno Setup, installed there on first use."""
    prefix = ROOT / ".local/windows/inno-setup" / INNO_SETUP_VERSION
    if (prefix / "drive_c/InnoSetup/ISCC.exe").is_file():
        return prefix
    installer = prefix.parent / f"innosetup-{INNO_SETUP_VERSION}-x64.exe"
    if not installer.is_file() or digest(installer) != INNO_SETUP_SHA256:
        installer.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error",
                        "--output", str(installer), INNO_SETUP_URL], check=True, timeout=600)
        if digest(installer) != INNO_SETUP_SHA256:
            installer.unlink()
            raise SystemExit(f"{INNO_SETUP_URL} doesn't match its pinned SHA-256")
    # Its installer opens windows even when silent, so it gets a private X server.
    environment = dict(os.environ, WINEPREFIX=str(prefix), WINEDEBUG="-all")
    environment.pop("WAYLAND_DISPLAY", None)
    subprocess.run(["xvfb-run", "--auto-servernum", "sh", "-c",
                    'wine "$0" /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /NORESTART '
                    '"/DIR=C:\\InnoSetup" /NOICONS && wineserver -w', str(installer)],
                   env=environment, check=True, timeout=600)
    if not (prefix / "drive_c/InnoSetup/ISCC.exe").is_file():
        raise SystemExit(f"Inno Setup didn't install into {prefix}")
    return prefix


def compile_installer(package, version, flavour, output):
    """Compiles packaging/windows/yapfast.iss over the ZIP's files."""
    prefix = inno_setup()
    environment = wine_environment(prefix)

    def windows_path(path):
        return subprocess.check_output(["winepath", "-w", str(path)], env=environment, text=True,
                                       timeout=60).strip()

    # The file's version resource takes only numbers, without a nightly's suffix.
    definitions = [f"/DAppVersion={version}", f"/DVersionNumber={version.split('-')[0]}",
                   f"/DSourceDir={windows_path(package)}"]
    if FLAVOURS[flavour]["edition"]:
        definitions += [f"/DEdition={FLAVOURS[flavour]['edition']}", f"/DFileSuffix={FLAVOURS[flavour]['suffix']}"]
    license_path = ROOT / "LICENSE"
    if license_path.is_file():
        definitions.append(f"/DLicenseFile={windows_path(license_path)}")
    subprocess.run(["wine", "C:\\InnoSetup\\ISCC.exe", "/Q", *definitions,
                    f"/O{windows_path(output)}", windows_path(ROOT / "packaging/windows/yapfast.iss")],
                   env=environment, check=True, timeout=1800)
    subprocess.run(["wineserver", "-w"], env=environment, check=True, timeout=120)
    return output / f"Yapfast-{version}{FLAVOURS[flavour]['suffix']}-windows-x64-setup.exe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=ROOT / "target" / TARGET / "release/yapfast.exe")
    parser.add_argument("--flavour", choices=FLAVOURS, default="discord",
                        help="the features the executable was built with (default: discord)")
    parser.add_argument("--output", type=Path, help="default: target/dist/FLAVOUR")
    parser.add_argument("--no-installer", action="store_true", help="make only the portable ZIP")
    args = parser.parse_args()
    flavour = FLAVOURS[args.flavour]
    output = args.output or ROOT / "target/dist" / args.flavour
    executable = args.exe.resolve()
    newest_input = max(path.stat().st_mtime for path in
                       [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "build.rs",
                        *list((ROOT / "src").rglob("*.rs")), *list((ROOT / "ui").rglob("*")),
                        *list((ROOT / "packaging").rglob("*"))]
                       # The installer's script doesn't go into the executable.
                       if path.is_file() and path.suffix != ".iss")
    if executable.stat().st_mtime < newest_input:
        raise SystemExit("Windows executable predates the current UI; run scripts/build-windows first")
    with executable.open("rb") as stream:
        header = stream.read(64)
        if header[:2] != b"MZ":
            raise SystemExit("Expected a Windows executable")
        stream.seek(struct.unpack_from("<I", header, 60)[0])
        pe = stream.read(6)
        if pe[:4] != b"PE\0\0" or struct.unpack_from("<H", pe, 4)[0] != 0x8664:
            raise SystemExit("Expected an AMD64 Windows PE executable")
    contents = executable.read_bytes()
    if names_home(contents):
        raise SystemExit(f"{executable} names {Path.home()}; build it with scripts/build-windows")
    # Both flavours build to the same path; the Matrix SDK's source paths
    # tell which one is there.
    if (b"/matrix-rust-sdk-" in contents) != (args.flavour == "full"):
        raise SystemExit(f"{executable} isn't the {args.flavour} flavour; run scripts/build-windows {args.flavour}")
    del contents
    # Only the packages this flavour's features build.
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET,
         *flavour["features"]],
        cwd=ROOT, timeout=30,
    ))
    version = next(package["version"] for package in metadata["packages"] if package["name"] == "yapfast")
    output.mkdir(parents=True, exist_ok=True)
    base = f"Yapfast-{version}{flavour['suffix']}"
    archive_path = output / f"{base}-windows-x64.zip"
    source_path = output / f"{base}-source.zip"
    with tempfile.TemporaryDirectory(prefix="yapfast-windows-package-") as temporary:
        temporary = Path(temporary)
        rtc = source_snapshot(metadata, base, temporary, source_path)

        package = temporary / f"{base}-windows-x64"
        package.mkdir()
        shutil.copy2(executable, package / "yapfast.exe")
        shutil.copy2(ROOT / "LICENSE", package / "LICENSE.txt")
        # The source snapshot goes beside the packages, on the release page,
        # rather than into every install; SOURCE.txt says where.
        source_url = f"https://github.com/yapfast/yapfast/releases/tag/v{version}{flavour['suffix']}"
        (package / "SOURCE.txt").write_text(
            f"The source code of this build of Yapfast {version}"
            + (", with the exact source of\nthe libraries it includes under the AGPL," if rtc else "")
            + " is published with it:\n\n"
            f"  {source_url}\n  {source_path.name}, SHA-256 in BUILD.json\n\n"
            "and in the repository, https://github.com/yapfast/yapfast. The licenses folder\n"
            "holds every dependency's own license and notices.\n",
            encoding="utf-8", newline="\r\n",
        )
        if args.flavour == "discord":
            first_steps = (
                "3. Link your Discord account: Yapfast shows a QR code, which Discord on\n"
                "   your phone scans (Settings, Scan QR Code). Discord doesn't allow apps\n"
                "   like Yapfast on personal accounts; linking puts your account at some\n"
                "   risk of being suspended.\n"
                "4. Open a channel, or a voice channel to talk.\n\n"
                "No account or saved session is included.\n"
            )
            secrets = "password or Discord token"
        else:
            first_steps = (
                "3. Sign in with your Matrix account; the homeserver is editable.\n"
                "   Or link Discord, which shows a QR code for Discord on your phone.\n"
                "4. Open a room, or a voice channel to talk.\n\n"
                "Passwords, recovery keys and saved sessions are not included.\n"
            )
            secrets = "password or recovery key"
        edition = f" ({flavour['edition']})" if flavour["edition"] else ""
        (package / "README.txt").write_text(
            f"Yapfast{edition} for 64-bit Windows 10/11\n\n"
            "1. Extract the entire ZIP to a folder.\n"
            "2. Double-click yapfast.exe.\n"
            + first_steps +
            "No Rust, Visual Studio, or separate app installation is required.\n"
            "Yapfast writes diagnostics to yapfast.log in its data folder,\n"
            "%LOCALAPPDATA%\\Yapfast\\Yapfast\\data, or to the console run-yapfast.cmd opens.\n"
            "Windows stores session secrets in Credential Manager and app data\n"
            "under your Windows user profile. Keep the first verified session.\n\n"
            "Voice: select a voice channel and Join voice; the gear chooses devices.\n"
            "Use headphones for the first two-machine audio test.\n"
            "If anything fails, record the error text, Windows version, GPU and\n"
            f"audio device model. Never send your {secrets} in logs.\n\n"
            "If Yapfast closes immediately, run run-yapfast.cmd to keep its error visible.\n\n"
            "Yapfast switches to software rendering if the GPU cannot start.\n"
            "To choose it yourself, run run-yapfast.cmd --renderer software.\n"
            "To retry the GPU after a fallback, run run-yapfast.cmd --renderer gpu.\n\n"
            "Yapfast is free software under the GNU General Public License, version 3\n"
            "or later, in LICENSE.txt. Dependency notices and original licenses are\n"
            "included; SOURCE.txt says where this build's source code is published.\n",
            encoding="utf-8",
        )
        (package / "run-yapfast.cmd").write_text(
            '@echo off\ncd /d "%~dp0"\nyapfast.exe %*\n'
            'if errorlevel 1 (\n  echo Yapfast exited with an error.\n  pause\n)\n',
            encoding="utf-8", newline="\r\n",
        )
        skia_location = os.environ.get("SKIA_BINARIES_URL")
        if not skia_location:
            archives = list((ROOT / ".local/windows/skia-static-production/prepared").glob("*-static.tar.gz"))
            if len(archives) != 1:
                raise SystemExit("Prepare the pinned Windows Skia archive before packaging")
            skia_location = archives[0].as_uri()
        skia_url = urllib.parse.urlparse(skia_location)
        if skia_url.scheme != "file":
            raise SystemExit("Packaging requires the pinned local Skia archive")
        skia_archive = Path(urllib.parse.unquote(skia_url.path))
        dependencies = dependency_notices(metadata, package, package / "licenses", skia_archive,
                                          Path(os.environ["YAPFAST_SKIA_WINDOWS_SOURCE"]),
                                          Path(os.environ["LK_CUSTOM_WEBRTC"]))
        (package / "THIRD-PARTY.json").write_text(json.dumps(dependencies, indent=2) + "\n", encoding="utf-8")
        build = {
            "version": version, "flavour": args.flavour, "target": TARGET,
            "cargo_features": " ".join(flavour["features"]) or "default",
            "rustc": subprocess.check_output(["rustc", "--version"], text=True, timeout=10).strip(),
            "cargo_xwin": subprocess.check_output(["cargo", "xwin", "--version"], text=True, timeout=10).strip(),
            "ui_framework": "Slint 1.18.1",
            "renderer_default": "skia-opengl, falling back to skia-software",
            "renderer_software": "skia-software",
            "skia_version": "0.153.3",
            "skia_source_revision": "61e7ca4e99062cdd0ab69445d5963fb3365778f6",
            "skia_source_archive_sha256": "8401d0de878582a03abc1029bb62330aea58cda89904ae6ab187302e6d48ba5d",
            "skia_runtime": "MT_StaticRelease",
            "skia_archive_prepared_sha256": digest(skia_archive),
            "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
            "executable_sha256": digest(executable),
            "source_archive_sha256": digest(source_path),
            "webrtc_tag": "webrtc-51ef663",
            "webrtc_archive_sha256": "0a56a5c91b3b7b4222082b8a09388d16a802cde22d67295beb24c55700755b30",
        }
        if rtc:
            build["homeserver_default"] = "https://matrix.org"
        (package / "BUILD.json").write_text(json.dumps(build, indent=2) + "\n", encoding="utf-8")
        zip_directory(package, archive_path)
        installer_path = None if args.no_installer else compile_installer(package, version, args.flavour, output.resolve())
    hashes = output / "SHA256SUMS.txt"
    artifacts = [archive_path, source_path, *([installer_path] if installer_path else [])]
    hashes.write_text("".join(f"{digest(path)}  {path.name}\n" for path in artifacts), encoding="utf-8")
    print(f"Packaged {archive_path} ({archive_path.stat().st_size / 1024 / 1024:.1f} MiB)")
    if installer_path:
        print(f"Installer: {installer_path} ({installer_path.stat().st_size / 1024 / 1024:.1f} MiB)")
    print(f"Source snapshot: {source_path}")
    print(f"Checksums: {hashes}")


if __name__ == "__main__":
    main()
