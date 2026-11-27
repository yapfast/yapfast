"""What scripts/package-windows.py and scripts/package-linux.py share: the
source snapshot each release carries, and the dependencies' licenses and
notices each package holds."""

import hashlib
import shutil
import struct
import subprocess
import time
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_FILES = (
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "rust-toolchain.toml",
    "flake.nix",
    "flake.lock",
    "README.md",
    "LICENSE",
    ".gitignore",
)
SOURCE_DIRECTORIES = (
    ".cargo",
    "src",
    "ui",
    "packaging",
    "crates",
    "scripts",
    "nix",
    "vendor",
    "testing",
    "docs",
    "themes",
)
WINDOWS_TARGETS = {"x64": "x86_64-pc-windows-msvc", "arm64": "aarch64-pc-windows-msvc"}
PE_MACHINES = {"x64": 0x8664, "arm64": 0xAA64}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def check_windows_executable(executable, arch):
    with executable.open("rb") as stream:
        header = stream.read(64)
        if len(header) != 64 or header[:2] != b"MZ":
            raise SystemExit("Expected a Windows executable")
        stream.seek(struct.unpack_from("<I", header, 60)[0])
        if stream.read(6) != b"PE\0\0" + struct.pack("<H", PE_MACHINES[arch]):
            raise SystemExit(f"Expected a Windows PE executable for {WINDOWS_TARGETS[arch]}")


def update_checksums(output, artifacts):
    """Refresh the sums for these files, keeping the other packages beside them."""
    hashes = output / "SHA256SUMS.txt"
    sums = {}
    if hashes.exists():
        for line in hashes.read_text(encoding="utf-8").splitlines():
            value, name = line.split("  ", 1)
            sums[name] = value
    sums.update({path.name: digest(path) for path in artifacts})
    hashes.write_text(
        "".join(f"{value}  {name}\n" for name, value in sorted(sums.items())),
        encoding="utf-8",
        newline="\n",
    )
    return hashes


def zip_directory(source, destination, timestamp=None):
    # Nix inputs use an epoch timestamp, which predates ZIP's 1980 minimum.
    with zipfile.ZipFile(
        destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False
    ) as archive:
        for path in sorted(source.rglob("*")):
            if not path.is_file():
                continue
            name = source.name + "/" + path.relative_to(source).as_posix()
            if timestamp is None:
                archive.write(path, name)
            else:
                info = zipfile.ZipInfo.from_file(path, name, strict_timestamps=False)
                info.date_time = timestamp
                info.compress_type = zipfile.ZIP_DEFLATED
                with path.open("rb") as original, archive.open(info, "w") as member:
                    shutil.copyfileobj(original, member)


def license_file(path):
    name = path.name.upper()
    # FTL.TXT is the FreeType License, which FreeType's LICENSE.TXT only names.
    return path.is_file() and (
        name.startswith(("LICENSE", "COPYING", "NOTICE", "OFL")) or name == "FTL.TXT"
    )


def names_home(contents):
    """Whether a binary names this machine's home folder, as Rust's panic
    messages and C's asserts do when built without scripts/release-env."""
    return any(
        contents.find(str(Path.home()).encode(encoding)) >= 0 for encoding in ("utf-8", "utf-16-le")
    )


def source_snapshot(metadata, base, temporary, destination):
    """Zips the repository's tracked source to destination, with the exact
    upstream RTC source the AGPL dependencies came from when the build has
    them. Returns whether it does."""
    source = temporary / f"{base}-source"
    source.mkdir()
    # Only what the repository tracks: the folders also hold private,
    # ignored material.
    tracked = (
        subprocess.check_output(
            ["git", "ls-files", "-z", "--", *SOURCE_FILES, *SOURCE_DIRECTORIES],
            cwd=ROOT,
            timeout=30,
        )
        .decode()
        .split("\0")
    )
    for name in filter(None, tracked):
        (source / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, source / name)
    # Only the Matrix flavour builds the AGPL RTC dependencies.
    rtc = next(
        (package for package in metadata["packages"] if package["name"] == "matrix-rtc-core"), None
    )
    if rtc:
        rtc_root = Path(rtc["manifest_path"]).parent.parent.parent
        shutil.copytree(
            rtc_root,
            source / "upstream/matrix-rust-rtc",
            ignore=shutil.ignore_patterns(".git", "target", "node_modules"),
        )
    # Every file takes the commit's UTC time, so builders in different time
    # zones zip the same snapshot.
    committed = int(
        subprocess.check_output(["git", "log", "-1", "--format=%ct"], cwd=ROOT, timeout=30)
    )
    zip_directory(source, destination, time.gmtime(committed)[:6])
    return rtc is not None


def dependency_notices(metadata, package, licenses, webrtc):
    """Copies each crate's and libwebrtc's licenses and notices into licenses,
    and returns the crates' entries for THIRD-PARTY.json, with paths relative
    to package."""
    dependencies = []
    licenses.mkdir()
    font_notices = licenses / "fonts"
    font_notices.mkdir()
    for path in sorted((ROOT / "crates/yapfast-iced-kit/fonts").iterdir()):
        if license_file(path) or path.name == "README.md":
            shutil.copy2(path, font_notices / path.name)
    shutil.copy2(ROOT / "src/LICENSE-emojibase.txt", licenses / "LICENSE-emojibase.txt")
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
            copied.append((target / name).relative_to(package).as_posix())
        dependencies.append(
            {
                "name": item["name"],
                "version": item["version"],
                "license": item["license"],
                "source": item["source"],
                "repository": item["repository"],
                "license_files": copied,
            }
        )
    shutil.copy2(webrtc / "LICENSE.md", licenses / "libwebrtc-LICENSE.md")
    # Twemoji's pictures, which the picker's and reactions' emoji are drawn from:
    # CC-BY 4.0, which asks for the attribution its README.yapfast.md carries.
    twemoji = ROOT / "vendor" / "twemoji"
    twemoji_licenses = []
    for name in ["LICENSE-GRAPHICS", "LICENSE", "README.md", "README.yapfast.md"]:
        target = licenses / ("twemoji-" + name)
        shutil.copy2(twemoji / name, target)
        twemoji_licenses.append(target.relative_to(package).as_posix())
    dependencies.append(
        {
            "name": "twemoji",
            "version": (twemoji / "REVISION").read_text().strip(),
            "license": "CC-BY-4.0",
            "source": "https://github.com/discord/twemoji",
            "repository": "https://github.com/discord/twemoji",
            "license_files": twemoji_licenses,
        }
    )
    # Some crates ship a license only their owner may read (fnv's are 0600),
    # and the AppImage's files all belong to root.
    for path in licenses.rglob("*"):
        if path.is_file():
            path.chmod(0o644)
    return dependencies
