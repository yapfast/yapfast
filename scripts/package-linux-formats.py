#!/usr/bin/env python3
"""Build DEB, RPM and Flatpak packages beside a native Yapfast release AppImage."""

import argparse
import json
import platform
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from release_package import ROOT, digest, update_checksums

APP_ID = "io.github.yapfast.Yapfast"
RUNTIME = "25.08"
FLATHUB = "https://dl.flathub.org/repo/flathub.flatpakrepo"
DEB_ARCH = {"x86_64": "amd64", "aarch64": "arm64"}


def run(*args, **options):
    subprocess.run(args, check=True, timeout=900, **options)


def deb(payload, output, version, arch):
    debian = payload / "DEBIAN"
    debian.mkdir()
    (debian / "control").write_text(
        f"""Package: yapfast
Version: {version}
Architecture: {arch}
Maintainer: Yapfast maintainers <https://github.com/yapfast/yapfast/issues>
Section: net
Priority: optional
Depends: libc6 (>= 2.39), libstdc++6 (>= 14), libgcc-s1, libglib2.0-0t64, libpulse0,
 hicolor-icon-theme, libasound2t64, libwayland-client0, libxkbcommon0,
 libx11-6, libxcb1, libxcursor1, libxi6, libxkbcommon-x11-0
Recommends: libpipewire-0.3-0t64, libdrm2, libgbm1, libvulkan1
Homepage: https://github.com/yapfast/yapfast
Description: Native Discord client
 Yapfast is a native desktop client for Discord text, voice and video.
""",
        encoding="utf-8",
    )
    license_file = payload / "usr/share/doc/yapfast/LICENSE"
    shutil.copyfile(license_file, license_file.with_name("copyright"))
    try:
        run("dpkg-deb", "--root-owner-group", "--build", payload, output)
    finally:
        license_file.with_name("copyright").unlink()
        shutil.rmtree(debian)


def rpm(payload, output, version, work):
    run(
        "rpmbuild",
        "-bb",
        "--define",
        f"_topdir {work / 'rpm'}",
        "--define",
        f"yapfast_payload {payload}",
        "--define",
        f"yapfast_version {version}",
        "--define",
        "_build_id_links none",
        ROOT / "packaging/linux/yapfast.spec",
    )
    (package,) = (work / "rpm/RPMS").glob("*/*.rpm")
    shutil.move(package, output)


def flatpak(payload, output, branch, arch, work):
    build = work / "flatpak"
    repo = work / "repo"
    run(
        "flatpak",
        "build-init",
        f"--arch={arch}",
        build,
        APP_ID,
        "org.freedesktop.Sdk",
        "org.freedesktop.Platform",
        RUNTIME,
    )
    shutil.copytree(payload / "usr", build / "files", dirs_exist_ok=True)
    run(
        "flatpak",
        "build-finish",
        "--command=yapfast",
        "--share=network",
        "--share=ipc",
        "--socket=wayland",
        "--socket=fallback-x11",
        "--socket=pulseaudio",
        "--device=dri",
        "--filesystem=xdg-run/pipewire-0",
        "--talk-name=org.freedesktop.secrets",
        "--talk-name=org.freedesktop.Notifications",
        "--talk-name=org.kde.StatusNotifierWatcher",
        build,
    )
    run("flatpak", "build-export", "--disable-sandbox", f"--arch={arch}", repo, build, branch)
    run(
        "flatpak",
        "build-bundle",
        f"--arch={arch}",
        f"--runtime-repo={FLATHUB}",
        repo,
        output,
        APP_ID,
        branch,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("appimage", type=Path)
    parser.add_argument(
        "--format",
        action="append",
        choices=("deb", "rpm", "flatpak"),
        help="format to build (repeatable, default: all three)",
    )
    args = parser.parse_args()
    image = args.appimage.resolve()
    arch = platform.machine()
    suffix = f"-{arch}.AppImage"
    if (
        arch not in DEB_ARCH
        or not image.name.startswith("Yapfast-")
        or not image.name.endswith(suffix)
    ):
        raise SystemExit(f"Expected a Yapfast release AppImage for {arch}")
    version = image.name.removeprefix("Yapfast-").removesuffix(suffix)
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*)?", version):
        raise SystemExit(f"Invalid release version in {image.name}")
    native_version = version.replace("-", "~", 1)
    branch = "nightly" if "-nightly." in version else "stable" if "-" not in version else "beta"
    formats = args.format or ("deb", "rpm", "flatpak")
    with tempfile.TemporaryDirectory(prefix="yapfast-linux-formats-") as temporary:
        work = Path(temporary)
        run(image, "--appimage-extract", cwd=work, stdout=subprocess.DEVNULL)
        payload = work / "squashfs-root"
        build = json.loads((payload / "usr/share/doc/yapfast/BUILD.json").read_text())
        if (
            build["version"] != version
            or build["target"] != f"{arch}-unknown-linux-gnu"
            or build["executable_sha256"] != digest(payload / "usr/bin/yapfast")
        ):
            raise SystemExit("AppImage payload does not match its release metadata")
        # AppImage contents are read-only, and some upstream notices are owner-only.
        payload.chmod(0o755)
        (payload / "usr").chmod(0o755)
        for path in (payload / "usr").rglob("*"):
            path.chmod(0o755 if path.is_dir() or path == payload / "usr/bin/yapfast" else 0o644)
        artifacts = []
        if "deb" in formats:
            name = f"yapfast_{version}_{DEB_ARCH[arch]}.deb"
            deb(payload, work / name, native_version, DEB_ARCH[arch])
            artifacts.append(work / name)
        if "rpm" in formats:
            name = f"yapfast-{version}-1.{arch}.rpm"
            rpm(payload, work / name, native_version.replace("-", "_"), work)
            artifacts.append(work / name)
        if "flatpak" in formats:
            name = f"Yapfast-{version}-{arch}.flatpak"
            flatpak(payload, work / name, branch, arch, work)
            artifacts.append(work / name)
        destinations = [Path(shutil.move(path, image.parent / path.name)) for path in artifacts]
    update_checksums(image.parent, destinations)
    for package in destinations:
        print(f"Packaged {package}")


if __name__ == "__main__":
    main()
