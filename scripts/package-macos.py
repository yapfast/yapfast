#!/usr/bin/env python3
"""Package the macOS executable as Yapfast.app, with its dependency notices,
in a disk image, beside a source snapshot.

The Discord flavour is the release; the full one, with Matrix, goes out as a
separate preview. Both are the same app, so one replaces the other. Runs on the
Mac that built it, with macOS's own Python 3.8."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = "aarch64-apple-darwin"
# scripts/build-macos's inputs.
INPUTS = ROOT / ".local/macos"
SKIA_ARCHIVE = "skia-binaries-b7f043e0b1e2a850e702-aarch64-apple-darwin-ganesh-gl-jpegd-jpege-metal-pdf.tar.gz"
WEBRTC = "webrtc-51ef663"
SOURCE_FILES = ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "README.md", "LICENSE", ".gitignore")
SOURCE_DIRECTORIES = (".cargo", "src", "ui", "packaging", "crates", "scripts", "nix", "vendor", "testing", "docs")
# Cargo features and the suffix of file names and release tag, for each
# flavour scripts/build-macos builds.
FLAVOURS = {
    "discord": {"features": ["--no-default-features", "--features", "desktop,discord"], "suffix": ""},
    "full": {"features": [], "suffix": "-full-preview"},
}
# Mach-O's 64-bit magic number and CPU type for ARM64.
MACHO_MAGIC = 0xFEEDFACF
CPU_TYPE_ARM64 = 0x0100000C


def digest(path):
    hashed = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            hashed.update(chunk)
    return hashed.hexdigest()


def zip_directory(source, destination):
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as archive:
        for path in sorted(source.rglob("*")):
            if path.is_file():
                archive.write(path, source.name + "/" + path.relative_to(source).as_posix())


def license_file(path):
    name = path.name.upper()
    return path.is_file() and name.startswith(("LICENSE", "COPYING", "NOTICE", "OFL"))


def check_executable(executable, flavour):
    newest_input = max(path.stat().st_mtime for path in
                       [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "build.rs",
                        *list((ROOT / "src").rglob("*.rs")), *list((ROOT / "ui").rglob("*"))]
                       if path.is_file())
    if executable.stat().st_mtime < newest_input:
        raise SystemExit("macOS executable predates the current source; run scripts/build-macos first")
    contents = executable.read_bytes()
    magic, cpu = struct.unpack_from("<II", contents)
    if magic != MACHO_MAGIC or cpu != CPU_TYPE_ARM64:
        raise SystemExit(f"{executable} isn't an Apple silicon executable")
    # Panic messages and C's asserts hold source paths; built without
    # scripts/release-env they name this machine's home folder.
    if str(Path.home()).encode() in contents:
        raise SystemExit(f"{executable} names {Path.home()}; build it with scripts/build-macos")
    # Both flavours build to the same path; the Matrix SDK's source paths
    # tell which one is there.
    if (b"/matrix-rust-sdk-" in contents) != (flavour == "full"):
        raise SystemExit(f"{executable} isn't the {flavour} flavour; run scripts/build-macos {flavour}")


def collect_licenses(metadata, resources):
    """Each dependency's license files, as the Windows package has them."""
    dependencies = []
    licenses = resources / "licenses"
    licenses.mkdir()
    for item in sorted(metadata["packages"], key=lambda value: (value["name"], value["version"])):
        if item["name"] == "yapfast":
            continue
        directory = Path(item["manifest_path"]).parent
        target = licenses / (item["name"] + "-" + item["version"])
        files = set()
        for parent in (directory, directory.parent, directory.parent.parent):
            files.update(path for path in parent.iterdir() if license_file(path))
        files.update(path for path in directory.rglob("*") if license_file(path))
        copied = []
        for index, path in enumerate(sorted(files)):
            target.mkdir(exist_ok=True)
            name = f"{index}-{path.name}"
            shutil.copy2(path, target / name)
            copied.append((target / name).relative_to(resources).as_posix())
        dependencies.append({
            "name": item["name"], "version": item["version"], "license": item["license"],
            "source": item["source"], "repository": item["repository"], "license_files": copied,
        })
    with tarfile.open(INPUTS / SKIA_ARCHIVE) as native:
        for member in native.getmembers():
            if member.isfile() and Path(member.name).name.upper().startswith(("LICENSE", "COPYING", "NOTICE")):
                (licenses / ("skia-native-" + Path(member.name).name)).write_bytes(native.extractfile(member).read())
    shutil.copy2(INPUTS / WEBRTC / "LICENSE.md", licenses / "libwebrtc-LICENSE.md")
    (resources / "THIRD-PARTY.json").write_text(json.dumps(dependencies, indent=2) + "\n", encoding="utf-8")


def source_snapshot(metadata, destination):
    with tempfile.TemporaryDirectory(prefix="yapfast-macos-source-") as temporary:
        source = Path(temporary) / destination.stem
        source.mkdir()
        # Only what the repository tracks: the folders also hold private,
        # ignored material.
        tracked = subprocess.check_output(["git", "ls-files", "-z", "--", *SOURCE_FILES, *SOURCE_DIRECTORIES],
                                          cwd=ROOT, timeout=30).decode().split("\0")
        for name in filter(None, tracked):
            (source / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, source / name)
        # The exact upstream RTC source used by the AGPL dependencies, which
        # only the Matrix flavour builds.
        rtc = next((package for package in metadata["packages"] if package["name"] == "matrix-rtc-core"), None)
        if rtc:
            rtc_root = Path(rtc["manifest_path"]).parent.parent.parent
            shutil.copytree(rtc_root, source / "upstream/matrix-rust-rtc",
                            ignore=shutil.ignore_patterns(".git", "target", "node_modules"))
        zip_directory(source, destination)
    return rtc is not None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=ROOT / "target" / TARGET / "release/yapfast")
    parser.add_argument("--flavour", choices=FLAVOURS, default="discord",
                        help="the features the executable was built with (default: discord)")
    parser.add_argument("--output", type=Path, help="default: target/dist/FLAVOUR")
    args = parser.parse_args()
    flavour = FLAVOURS[args.flavour]
    output = (args.output or ROOT / "target/dist" / args.flavour).resolve()
    executable = args.exe.resolve()
    check_executable(executable, args.flavour)
    # Only the packages this flavour's features build.
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET,
         *flavour["features"]],
        cwd=ROOT, timeout=30,
    ))
    version = next(package["version"] for package in metadata["packages"] if package["name"] == "yapfast")
    output.mkdir(parents=True, exist_ok=True)
    base = f"Yapfast-{version}{flavour['suffix']}"
    image_path = output / f"{base}-macos-arm64.dmg"
    source_path = output / f"{base}-source.zip"
    with_rtc = source_snapshot(metadata, source_path)

    # Kept unpacked beside the image, to run from here.
    app = output / "Yapfast.app"
    shutil.rmtree(app, ignore_errors=True)
    contents = app / "Contents"
    resources = contents / "Resources"
    (contents / "MacOS").mkdir(parents=True)
    resources.mkdir()
    info = (ROOT / "packaging/macos/Info.plist").read_text(encoding="utf-8")
    (contents / "Info.plist").write_text(info.replace("@VERSION@", version), encoding="utf-8")
    shutil.copy2(executable, contents / "MacOS/yapfast")
    shutil.copy2(ROOT / "packaging/macos/yapfast.icns", resources / "yapfast.icns")
    shutil.copy2(ROOT / "LICENSE", resources / "LICENSE.txt")
    source_url = f"https://github.com/yapfast/yapfast/releases/tag/v{version}{flavour['suffix']}"
    (resources / "SOURCE.txt").write_text(
        f"The source code of this build of Yapfast {version}"
        + (", with the exact source of\nthe libraries it includes under the AGPL," if with_rtc else "")
        + " is published with it:\n\n"
        f"  {source_url}\n  {source_path.name}, SHA-256 in BUILD.json\n\n"
        "and in the repository, https://github.com/yapfast/yapfast. The licenses folder\n"
        "holds every dependency's own license and notices.\n",
        encoding="utf-8",
    )
    collect_licenses(metadata, resources)
    build = {
        "version": version, "flavour": args.flavour, "target": TARGET,
        "cargo_features": " ".join(flavour["features"]) or "default",
        "rustc": subprocess.check_output(["rustc", "--version"], text=True, timeout=10).strip(),
        "c++": subprocess.check_output([os.environ.get("CXX_aarch64_apple_darwin", "c++"), "--version"],
                                       text=True, timeout=10).splitlines()[0],
        "macos_sdk": subprocess.check_output(["xcrun", "--show-sdk-version"], text=True, timeout=10).strip(),
        "macos_minimum": "11.0",
        "ui_framework": "Slint 1.18.1",
        "renderer_default": "skia-software",
        "skia_version": "0.153.3",
        "skia_archive_sha256": digest(INPUTS / SKIA_ARCHIVE),
        "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
        "executable_sha256": digest(executable),
        "source_archive_sha256": digest(source_path),
        "webrtc_tag": WEBRTC,
        "webrtc_archive_sha256": (INPUTS / WEBRTC / ".unpacked").read_text().split()[0],
    }
    (resources / "BUILD.json").write_text(json.dumps(build, indent=2) + "\n", encoding="utf-8")
    # No Developer ID: an ad-hoc signature, which Apple silicon requires to run
    # at all, seals the bundle and names it by its identifier.
    subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True, timeout=120)
    subprocess.run(["codesign", "--verify", "--strict", str(app)], check=True, timeout=120)

    with tempfile.TemporaryDirectory(prefix="yapfast-macos-image-") as temporary:
        staging = Path(temporary) / "Yapfast"
        staging.mkdir()
        # ditto keeps the signature's extended attributes.
        subprocess.run(["ditto", str(app), str(staging / "Yapfast.app")], check=True, timeout=300)
        (staging / "Applications").symlink_to("/Applications")
        image_path.unlink(missing_ok=True)
        subprocess.run(["hdiutil", "create", "-quiet", "-volname", "Yapfast", "-srcfolder", str(staging),
                        "-fs", "HFS+", "-format", "UDZO", "-ov", str(image_path)], check=True, timeout=600)
    hashes = output / "SHA256SUMS.txt"
    hashes.write_text("".join(f"{digest(path)}  {path.name}\n" for path in (image_path, source_path)),
                      encoding="utf-8")
    print(f"Packaged {image_path} ({image_path.stat().st_size / 1024 / 1024:.1f} MiB)")
    print(f"App: {app}")
    print(f"Source snapshot: {source_path}")
    print(f"Checksums: {hashes}")


if __name__ == "__main__":
    main()
