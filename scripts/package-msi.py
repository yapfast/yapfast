#!/usr/bin/env python3
"""Make an x64 or ARM64 MSI on Windows from a Yapfast portable release ZIP."""

import argparse
import json
import re
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

from release_package import (
    ROOT,
    WINDOWS_TARGETS,
    check_windows_executable,
    digest,
    update_checksums,
)

ARCHITECTURES = {target: arch for arch, target in WINDOWS_TARGETS.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument(
        "--output", type=Path, help="directory for the MSI (default: beside the ZIP)"
    )
    parser.add_argument("--wix", default="wix", help="path to the WiX 5 executable")
    args = parser.parse_args()
    output = (args.output or args.archive.parent).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="yapfast-msi-") as temporary:
        with zipfile.ZipFile(args.archive) as archive:
            archive.extractall(temporary)
        payload = Path(temporary) / args.archive.stem
        build = json.loads((payload / "BUILD.json").read_text(encoding="utf-8"))
        arch = ARCHITECTURES.get(build["target"])
        if arch is None or not args.archive.stem.endswith(f"-windows-{arch}"):
            raise SystemExit("ZIP name or architecture does not match BUILD.json")
        version = build["version"].split("-", 1)[0]
        if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) or any(
            int(field) > limit
            for field, limit in zip(version.split("."), (255, 255, 65535), strict=False)
        ):
            raise SystemExit(f"{build['version']} is outside the Windows Installer version range")
        executable = payload / "yapfast.exe"
        check_windows_executable(executable, arch)
        if digest(executable) != build["executable_sha256"]:
            raise SystemExit("Windows executable does not match BUILD.json")
        staged = Path(temporary) / f"{args.archive.stem}.msi"
        subprocess.run(
            [
                args.wix,
                "build",
                "-arch",
                arch,
                "-d",
                f"ProductVersion={version}",
                "-d",
                f"Payload={payload}",
                "-d",
                f"IconPath={ROOT / 'packaging/windows/yapfast.ico'}",
                "-o",
                staged,
                ROOT / "packaging/windows/yapfast.wxs",
            ],
            check=True,
            timeout=600,
        )
        # The build directory and the release directory can be on different drives.
        destination = output / staged.name
        shutil.copyfile(staged, destination)
    update_checksums(output, [destination])
    print(f"Packaged {destination}")


if __name__ == "__main__":
    main()
