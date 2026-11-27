#!/usr/bin/env python3
"""Package the Windows executable, build inputs and dependency notices, as a
portable ZIP and an Inno Setup installer.

The Discord flavour is the release; the full one, with Matrix, goes out as a
separate preview. They install as the same app, so one replaces the other."""

import argparse
import json
import os
import shutil
import subprocess
import tempfile
from mmap import ACCESS_READ, mmap
from pathlib import Path

from release_package import (
    ROOT,
    WINDOWS_TARGETS,
    check_windows_executable,
    dependency_notices,
    digest,
    names_home,
    source_snapshot,
    update_checksums,
    zip_directory,
)

# Inno Setup's compiler, run under Wine; installed once into its own prefix.
INNO_SETUP_VERSION = "7.1.0"
INNO_SETUP_URL = (
    "https://github.com/jrsoftware/issrc/releases/download/is-7_1_0/"
    f"innosetup-{INNO_SETUP_VERSION}-x64.exe"
)
INNO_SETUP_SHA256 = "0362a383ed217d4c4239b5933866dd96d3eb2102737da92f80f6057a4b40df2f"
# Cargo features, the suffix of file names and release tag, and the name the
# installer shows, for each flavour scripts/build-windows builds.
FLAVOURS = {
    "discord": {
        "features": ["--no-default-features", "--features", "desktop,discord"],
        "suffix": "",
        "edition": None,
    },
    "full": {
        "features": ["--features", "matrix"],
        "suffix": "-full-preview",
        "edition": "full preview",
    },
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
        subprocess.run(
            [
                "curl",
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
                str(installer),
                INNO_SETUP_URL,
            ],
            check=True,
            timeout=600,
        )
        if digest(installer) != INNO_SETUP_SHA256:
            installer.unlink()
            raise SystemExit(f"{INNO_SETUP_URL} doesn't match its pinned SHA-256")
    # Its installer opens windows even when silent, so it gets a private X server.
    environment = dict(os.environ, WINEPREFIX=str(prefix), WINEDEBUG="-all")
    environment.pop("WAYLAND_DISPLAY", None)
    subprocess.run(
        [
            "xvfb-run",
            "--auto-servernum",
            "sh",
            "-c",
            'wine "$0" /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /NORESTART '
            '"/DIR=C:\\InnoSetup" /NOICONS && wineserver -w',
            str(installer),
        ],
        env=environment,
        check=True,
        timeout=600,
    )
    if not (prefix / "drive_c/InnoSetup/ISCC.exe").is_file():
        raise SystemExit(f"Inno Setup didn't install into {prefix}")
    return prefix


def compile_installer(package, version, flavour, output):
    """Compiles packaging/windows/yapfast.iss over the ZIP's files."""
    prefix = inno_setup()
    environment = wine_environment(prefix)

    def windows_path(path):
        return subprocess.check_output(
            ["winepath", "-w", str(path)], env=environment, text=True, timeout=60
        ).strip()

    # The file's version resource takes only numbers, without a nightly's suffix.
    definitions = [
        f"/DAppVersion={version}",
        f"/DVersionNumber={version.split('-')[0]}",
        f"/DSourceDir={windows_path(package)}",
    ]
    if FLAVOURS[flavour]["edition"]:
        definitions += [
            f"/DEdition={FLAVOURS[flavour]['edition']}",
            f"/DFileSuffix={FLAVOURS[flavour]['suffix']}",
        ]
    license_path = ROOT / "LICENSE"
    if license_path.is_file():
        definitions.append(f"/DLicenseFile={windows_path(license_path)}")
    subprocess.run(
        [
            "wine",
            "C:\\InnoSetup\\ISCC.exe",
            "/Q",
            *definitions,
            f"/O{windows_path(output)}",
            windows_path(ROOT / "packaging/windows/yapfast.iss"),
        ],
        env=environment,
        check=True,
        timeout=1800,
    )
    subprocess.run(["wineserver", "-w"], env=environment, check=True, timeout=120)
    return output / f"Yapfast-{version}{FLAVOURS[flavour]['suffix']}-windows-x64-setup.exe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--exe", type=Path, help="default: target/<MSVC target>/release/yapfast.exe"
    )
    parser.add_argument(
        "--flavour",
        choices=FLAVOURS,
        default="discord",
        help="the features the executable was built with (default: discord)",
    )
    parser.add_argument("--output", type=Path, help="default: target/dist/FLAVOUR")
    parser.add_argument(
        "--no-installer", action="store_true", help="skip the x64 Inno Setup installer"
    )
    args = parser.parse_args()
    arch = os.environ.get("YAPFAST_WINDOWS_ARCH", "x64")
    target = WINDOWS_TARGETS[arch]
    flavour = FLAVOURS[args.flavour]
    output = args.output or ROOT / "target/dist" / args.flavour
    executable = (args.exe or ROOT / "target" / target / "release/yapfast.exe").resolve()
    newest_input = max(
        path.stat().st_mtime
        for path in [
            ROOT / "Cargo.toml",
            ROOT / "Cargo.lock",
            ROOT / "build.rs",
            *list((ROOT / "src").rglob("*.rs")),
            *list((ROOT / "crates").rglob("*")),
            ROOT / "packaging/windows/yapfast.ico",
            ROOT / "packaging/windows/yapfast.rc",
        ]
        if path.is_file()
    )
    if executable.stat().st_mtime < newest_input:
        raise SystemExit(
            "Windows executable predates the current source; run scripts/build-windows first"
        )
    check_windows_executable(executable, arch)
    with executable.open("rb") as stream, mmap(stream.fileno(), 0, access=ACCESS_READ) as contents:
        if names_home(contents):
            raise SystemExit(
                f"{executable} names {Path.home()}; build it with scripts/build-windows"
            )
        # Both flavours build to the same path. The Matrix SDK's source paths tell them apart.
        if (contents.find(b"/matrix-rust-sdk-") >= 0) != (args.flavour == "full"):
            raise SystemExit(
                f"{executable} isn't the {args.flavour} flavour; "
                f"run scripts/build-windows {args.flavour}"
            )
    # Only the packages this flavour's features build.
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--locked",
                "--format-version",
                "1",
                "--filter-platform",
                target,
                *flavour["features"],
            ],
            cwd=ROOT,
            timeout=30,
        )
    )
    version = next(
        package["version"] for package in metadata["packages"] if package["name"] == "yapfast"
    )
    output.mkdir(parents=True, exist_ok=True)
    base = f"Yapfast-{version}{flavour['suffix']}"
    archive_path = output / f"{base}-windows-{arch}.zip"
    source_path = output / f"{base}-source.zip"
    with tempfile.TemporaryDirectory(prefix="yapfast-windows-package-") as temporary:
        temporary = Path(temporary)
        rtc = source_snapshot(metadata, base, temporary, source_path)

        package = temporary / archive_path.stem
        package.mkdir()
        shutil.copy2(executable, package / "yapfast.exe")
        shutil.copy2(ROOT / "LICENSE", package / "LICENSE.txt")
        # The source snapshot goes beside the packages, on the release page,
        # rather than into every install; SOURCE.txt says where.
        source_url = (
            f"https://github.com/yapfast/yapfast/releases/tag/v{version}{flavour['suffix']}"
        )
        (package / "SOURCE.txt").write_text(
            f"The source code of this build of Yapfast {version}"
            + (
                ", with the exact source of\nthe libraries it includes under the AGPL,"
                if rtc
                else ""
            )
            + " is published with it:\n\n"
            f"  {source_url}\n  {source_path.name}, SHA-256 in BUILD.json\n\n"
            "and in the repository, https://github.com/yapfast/yapfast. The licenses folder\n"
            "holds every dependency's own license and notices.\n",
            encoding="utf-8",
            newline="\r\n",
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
        renderer_note = (
            "Yapfast draws on the processor unless Settings > Advanced > Render with\n"
            "the GPU is on. If the GPU cannot start, Yapfast restarts on the processor\n"
            "and turns the setting off. run-yapfast.cmd --renderer software or\n"
            "--renderer gpu picks one for one start.\n\n"
        )
        (package / "README.txt").write_text(
            f"Yapfast{edition} for Windows 10/11 ({arch})\n\n"
            "1. Extract the entire ZIP to a folder.\n"
            "2. Double-click yapfast.exe.\n"
            + first_steps
            + "No Rust, Visual Studio, or separate app installation is required.\n"
            "Yapfast writes diagnostics to yapfast.log in its data folder,\n"
            "%LOCALAPPDATA%\\Yapfast\\Yapfast\\data, or to the console run-yapfast.cmd opens.\n"
            "Windows stores session secrets in Credential Manager and app data\n"
            "under your Windows user profile. Keep the first verified session.\n\n"
            "Voice: select a voice channel and Join voice; the gear chooses devices.\n"
            "Use headphones for the first two-machine audio test.\n"
            "If anything fails, record the error text, Windows version, GPU and\n"
            f"audio device model. Never send your {secrets} in logs.\n\n"
            "If Yapfast closes immediately, run run-yapfast.cmd to keep its error visible.\n\n"
            + renderer_note
            + "Yapfast is free software under the GNU General Public License, version 3\n"
            "or later, in LICENSE.txt. Dependency notices and original licenses are\n"
            "included; SOURCE.txt says where this build's source code is published.\n",
            encoding="utf-8",
        )
        (package / "run-yapfast.cmd").write_text(
            '@echo off\ncd /d "%~dp0"\nyapfast.exe %*\n'
            "if errorlevel 1 (\n  echo Yapfast exited with an error.\n  pause\n)\n",
            encoding="utf-8",
            newline="\r\n",
        )
        dependencies = dependency_notices(
            metadata, package, package / "licenses", Path(os.environ["LK_CUSTOM_WEBRTC"])
        )
        (package / "THIRD-PARTY.json").write_text(
            json.dumps(dependencies, indent=2) + "\n", encoding="utf-8"
        )
        build = {
            "version": version,
            "flavour": args.flavour,
            "target": target,
            "cargo_features": " ".join(flavour["features"]) or "default",
            "rustc": subprocess.check_output(["rustc", "--version"], text=True, timeout=10).strip(),
            "cargo_xwin": subprocess.check_output(
                ["cargo", "xwin", "--version"], text=True, timeout=10
            ).strip(),
            "ui_framework": "iced 0.14, with the patches in vendor/iced_wgpu and "
            "vendor/iced_tiny_skia",
            "renderer_default": "tiny-skia",
            "renderers": "tiny-skia, wgpu (Settings: Render with the GPU)",
            "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
            "executable_sha256": digest(executable),
            "source_archive_sha256": digest(source_path),
            "webrtc_tag": "webrtc-51ef663",
            "webrtc_archive_sha256": os.environ["YAPFAST_WEBRTC_ARCHIVE_HASH"],
        }
        if rtc:
            build["homeserver_default"] = "https://matrix.org"
        (package / "BUILD.json").write_text(json.dumps(build, indent=2) + "\n", encoding="utf-8")
        zip_directory(package, archive_path)
        installer_path = None
        if arch == "x64" and not args.no_installer:
            installer_path = compile_installer(package, version, args.flavour, output.resolve())
    artifacts = [archive_path, source_path, *([installer_path] if installer_path else [])]
    hashes = update_checksums(output, artifacts)
    print(f"Packaged {archive_path} ({archive_path.stat().st_size / 1024 / 1024:.1f} MiB)")
    if installer_path:
        print(
            f"Installer: {installer_path} ({installer_path.stat().st_size / 1024 / 1024:.1f} MiB)"
        )
    print(f"Source snapshot: {source_path}")
    print(f"Checksums: {hashes}")


if __name__ == "__main__":
    main()
