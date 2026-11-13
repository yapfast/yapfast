#!/usr/bin/env python3
"""Prepare the pinned static-CRT Windows Skia archive on a native Windows host."""
import argparse
import base64
import gzip
import hashlib
import http.client
import json
import os
import re
import shutil
import ssl
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path
from typing import NoReturn
from urllib.parse import urljoin, urlsplit

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / '.local/windows/skia'
SOURCE_CACHE = WORK / 'source'
SOURCE_TREE = SOURCE_CACHE / 'tree'
SOURCE_ARCHIVE = SOURCE_CACHE / 'archives/skia-6a8fe80eadfac737c12e64229a1b9530bc78f3d1.tar.gz'
SOURCE_URL = 'https://codeload.github.com/rust-skia/skia/tar.gz/6a8fe80eadfac737c12e64229a1b9530bc78f3d1'
SOURCE_SHA256 = 'dad0161d334409990f6f4409d1a26c1b0389e1668b79c0e175aecef651fd346d'
KEY = '30669978c3f39a73e8a7-x86_64-pc-windows-msvc-d3d-ganesh-gl-jpegd-jpege-pdf-static'
PACKAGE = 'skia-binaries-' + KEY + '.tar.gz'

# These pins mirror windowsSkiaDependencies in nix/flake.nix. Googlesource's
# +archive tarballs have no enclosing directory; GitHub's has one to strip.
DEPENDENCIES = [
    ('d3d12allocator',
     'https://github.com/GPUOpen-LibrariesAndSDKs/D3D12MemoryAllocator/archive/169895d529dfce00390a20e69c2f516066fe7a3b.tar.gz',
     'sha256-5DbR/3Lb0eMBpHNf7MtZOb1C0QrNlx/ZvPVbCxxF9eQ=', True),
    ('expat',
     'https://chromium.googlesource.com/external/github.com/libexpat/libexpat/+archive/a70841d9ffa64ddde29ab824ed6305035ebe4c47.tar.gz',
     'sha256-aRRlOkHq/qYa+7hrJM6LugNkiUz5ki/Sg3mONa5NEzs=', False),
    ('libjpeg-turbo',
     'https://chromium.googlesource.com/chromium/deps/libjpeg_turbo/+archive/640f254ad0fa03f6b1f29f89b7dd9366f2f6e533.tar.gz',
     'sha256-wor4RTF3/5BFL9EWcGEofY+M4HN2+/KJUaOY+u86K5Q=', False),
    ('libpng',
     'https://skia.googlesource.com/third_party/libpng/+archive/3061454d980de7d53608f594194cfac722721d2a.tar.gz',
     'sha256-JSdzPsfRBjnk8DB/fkOCkaiUJ9/neQ2myIK5xnQuSsM=', False),
    ('wuffs',
     'https://skia.googlesource.com/external/github.com/google/wuffs-mirror-release-c/+archive/7411f488fe2e2c205c3d3b3d28638b7356522930.tar.gz',
     'sha256-AMuAaCbNJYnSeac1B1IRSUh4rlE2KphT04/Ak4Pig5M=', False),
    ('zlib',
     'https://chromium.googlesource.com/chromium/src/third_party/zlib/+archive/456ae730017b9b4bd3371927abc82627fd51feef.tar.gz',
     'sha256-K07NY9DN6lsTNmKl7OK3xAZ1vdesSnQVVsN3ZdJG/WU=', False),
    ('spirv-cross',
     'https://chromium.googlesource.com/external/github.com/KhronosGroup/SPIRV-Cross/+archive/b8fcf307f1f347089e3c46eb4451d27f32ebc8d3.tar.gz',
     'sha256-H43M9DXfEuyKuvo6rjb5k0KEbYOSFodbPJh8ZKY4PQg=', False),
]


def say(message):
    print(message, file=sys.stderr, flush=True)


def fail(message: str) -> NoReturn:
    raise SystemExit('error: ' + message)


def remove_tree(path, *, ignore_errors=False):
    try:
        shutil.rmtree(path)
    except FileNotFoundError:
        pass
    except OSError as error:
        if not ignore_errors:
            fail(f'could not remove {path}: {error}')


def move_path(source, destination):
    try:
        shutil.move(str(source), destination)
    except OSError as error:
        fail(f'could not move {source} to {destination}: {error}')


# A rename can lose a race on Windows and win it a moment later, so these
# waits start short and then hold, long enough to outlast a scan.
LOCK_RETRY_DELAYS: tuple[float, ...] = (0.5, 1, 2, 4, 8, 8, 8)


def replace_path(source: Path, destination: Path, *, tree: bool = False) -> None:
    """Rename source onto destination, outlasting a Windows lock on either.

    Windows renames a path only while no other process holds a handle in it,
    and the search indexer and antiviruses open the files of a tree just
    written without the sharing a rename needs: they refuse the move with
    access denied or a sharing violation, and let it through once they have
    read what they wanted. Try again rather than fail on the first refusal.
    A directory cannot be renamed over an existing one, so remove that first.
    """
    delays: tuple[float | None, ...] = (*LOCK_RETRY_DELAYS, None)
    for delay in delays:
        try:
            if tree:
                remove_tree(destination, ignore_errors=True)
            os.replace(source, destination)
            return
        except OSError as error:
            if delay is None:
                cause = error.strerror or str(error)
                fail(f'could not move {source} to {destination} after {len(delays)} attempts: {cause}')
            say(f'{destination.name} is locked ({error.strerror or error}); trying again in {delay:g}s')
            time.sleep(delay)


def digest(path):
    value = hashlib.sha256()
    try:
        with Path(path).open('rb') as source:
            for block in iter(lambda: source.read(1024 * 1024), b''):
                value.update(block)
    except OSError as error:
        fail(f'could not hash {path}: {error}')
    return value.hexdigest()


def pinned_hex(value):
    if value.startswith('sha256-'):
        return base64.b64decode(value[len('sha256-'):], validate=True).hex()
    return value


def open_https(url):
    for _ in range(6):
        try:
            parsed = urlsplit(url)
            port = parsed.port or 443
        except ValueError as error:
            fail(f'invalid pinned archive URL: {error}')
        if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password:
            fail('pinned archive URLs and redirects must use HTTPS')
        connection = http.client.HTTPSConnection(parsed.hostname, port,
                                                 timeout=60, context=ssl.create_default_context())
        request_path = parsed.path or '/'
        if parsed.query:
            request_path += '?' + parsed.query
        try:
            connection.request('GET', request_path,
                               headers={'User-Agent': 'Yapfast Skia preparation'})
            response = connection.getresponse()
        except (OSError, http.client.HTTPException):
            connection.close()
            raise
        if response.status in (301, 302, 303, 307, 308):
            location = response.getheader('Location')
            connection.close()
            if not location:
                fail(f'HTTPS redirect from {url} had no Location header')
            url = urljoin(url, location)
            continue
        if response.status != 200:
            status = f'{response.status} {response.reason}'
            connection.close()
            fail(f'download from {url} failed with HTTP {status}')
        return connection, response
    fail(f'too many HTTPS redirects while downloading {url}')


def fetch_to_file(url, temporary, label):
    value = hashlib.sha256()
    connection = None
    try:
        connection, response = open_https(url)
        with temporary.open('wb') as output:
            while True:
                block = response.read(1024 * 1024)
                if not block:
                    break
                value.update(block)
                output.write(block)
    except (OSError, http.client.HTTPException) as error:
        temporary.unlink(missing_ok=True)
        fail(f'could not download {label} from {url}: {error}')
    finally:
        if connection is not None:
            connection.close()
    return value.hexdigest()


def download(url, destination, expected_hash, label):
    destination.parent.mkdir(parents=True, exist_ok=True)
    expected_hash = pinned_hex(expected_hash)
    if destination.is_file():
        actual = digest(destination)
        if actual == expected_hash:
            say('Verified cached ' + label)
            return actual
        say('Cached ' + label + ' has the wrong SHA-256; replacing it')
        destination.unlink()

    say('Downloading ' + label)
    temporary = destination.with_name(destination.name + '.part')
    temporary.unlink(missing_ok=True)
    actual = fetch_to_file(url, temporary, label)
    if actual != expected_hash:
        temporary.unlink(missing_ok=True)
        fail(f'{label} SHA-256 mismatch: expected {expected_hash}, got {actual}')
    replace_path(temporary, destination)
    say('Verified SHA-256 for ' + label)
    return actual


class NarNode:
    def __init__(self, kind=None, member=None):
        self.kind: str | None = kind
        self.member: tarfile.TarInfo | None = member
        self.children: dict[bytes, NarNode] = {}
        self.link_target: bytes | None = None


def nar_string(hasher, data):
    if isinstance(data, str):
        data = data.encode('utf-8')
    hasher.update(struct.pack('<Q', len(data)))
    hasher.update(data)
    hasher.update(bytes(-len(data) % 8))


def tar_path_parts(name):
    if name.startswith('/') or re.match(r'^[A-Za-z]:', name):
        fail(f'archive contains an absolute path: {name!r}')
    parts = [part for part in name.replace('\\', '/').split('/') if part not in ('', '.')]
    if any(part == '..' for part in parts):
        fail(f'archive contains a parent path component: {name!r}')
    return parts


def tar_nar_hash(archive_path, *, strip_root=False):
    try:
        with tarfile.open(archive_path, 'r:gz') as archive:
            return hash_tar_contents(archive, strip_root=strip_root)
    except (OSError, tarfile.TarError) as error:
        fail(f'could not read archive {archive_path}: {error}')


def hash_tar_contents(archive, *, strip_root):
    members = archive.getmembers()
    root = NarNode('directory')
    hardlinks = []
    for member in members:
        parts = tar_path_parts(member.name)
        if strip_root:
            if not parts:
                continue
            parts = parts[1:]
        if not parts:
            continue
        node = root
        for part in parts[:-1]:
            name = part.encode('utf-8', 'surrogateescape')
            child = node.children.setdefault(name, NarNode('directory'))
            if child.kind not in (None, 'directory'):
                fail(f'archive path is nested beneath a non-directory: {member.name!r}')
            child.kind = 'directory'
            node = child
        name = parts[-1].encode('utf-8', 'surrogateescape')
        child = node.children.get(name)
        if child is None:
            child = NarNode()
            node.children[name] = child
        if member.isdir():
            if child.kind not in (None, 'directory'):
                fail(f'archive has conflicting entries for {member.name!r}')
            child.kind = 'directory'
        elif member.isfile():
            if child.kind is not None or child.children:
                fail(f'archive has conflicting entries for {member.name!r}')
            child.kind, child.member = 'regular', member
        elif member.issym():
            if child.kind is not None or child.children:
                fail(f'archive has conflicting entries for {member.name!r}')
            child.kind = 'symlink'
            child.link_target = member.linkname.encode('utf-8', 'surrogateescape')
        elif member.islnk():
            if child.kind is not None or child.children:
                fail(f'archive has conflicting entries for {member.name!r}')
            child.kind = 'hardlink'
            hardlinks.append((child, tar_path_parts(member.linkname)))
        else:
            fail(f'archive contains an unsupported entry type: {member.name!r}')

    for child, parts in hardlinks:
        if strip_root and parts:
            parts = parts[1:]
        target = root
        for part in parts:
            target = target.children.get(part.encode('utf-8', 'surrogateescape'))
            if target is None:
                fail(f'archive hard link target is missing: {parts!r}')
        if target.kind != 'regular':
            fail(f'archive hard link does not point to a regular file: {parts!r}')
        child.kind, child.member = 'regular', target.member

    hasher = hashlib.sha256()

    def write_node(node):
        kind = node.kind or 'directory'
        nar_string(hasher, '(')
        nar_string(hasher, 'type')
        if kind == 'directory':
            nar_string(hasher, 'directory')
            for name in sorted(node.children):
                nar_string(hasher, 'entry')
                nar_string(hasher, '(')
                nar_string(hasher, 'name')
                nar_string(hasher, name)
                nar_string(hasher, 'node')
                write_node(node.children[name])
                nar_string(hasher, ')')
        elif kind == 'regular':
            member = node.member
            if member is None:
                fail('archive has a regular NAR node with no file contents')
            nar_string(hasher, 'regular')
            if member.mode & 0o111:
                nar_string(hasher, 'executable')
                nar_string(hasher, '')
            nar_string(hasher, 'contents')
            hasher.update(struct.pack('<Q', member.size))
            content = archive.extractfile(member)
            if content is None:
                fail(f'could not read archive member {member.name!r}')
            with content:
                remaining = member.size
                while remaining:
                    block = content.read(min(1024 * 1024, remaining))
                    if not block:
                        fail(f'truncated archive member {member.name!r}')
                    hasher.update(block)
                    remaining -= len(block)
            hasher.update(bytes(-member.size % 8))
        elif kind == 'symlink':
            if node.link_target is None:
                fail('archive has a symlink NAR node with no target')
            nar_string(hasher, 'symlink')
            nar_string(hasher, 'target')
            nar_string(hasher, node.link_target)
        else:
            fail(f'unsupported NAR node type: {kind}')
        nar_string(hasher, ')')

    nar_string(hasher, 'nix-archive-1')
    write_node(root)
    return hasher.hexdigest()


def download_nix_archive(url, destination, expected_hash, label, *, hash_strip_root=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    expected = pinned_hex(expected_hash)
    if destination.is_file():
        actual = tar_nar_hash(destination, strip_root=hash_strip_root)
        if actual == expected:
            say('Verified cached unpacked hash for ' + label)
            return {'archive_sha256': digest(destination), 'nix_unpacked_sha256': actual}
        say('Cached ' + label + ' does not match its pinned unpacked hash; replacing it')
        destination.unlink()

    say('Downloading ' + label)
    temporary = destination.with_name(destination.name + '.part')
    temporary.unlink(missing_ok=True)
    archive_hash = fetch_to_file(url, temporary, label)
    actual = tar_nar_hash(temporary, strip_root=hash_strip_root)
    if actual != expected:
        temporary.unlink(missing_ok=True)
        fail(f'{label} unpacked SHA-256 mismatch: expected {expected}, got {actual}')
    replace_path(temporary, destination)
    say('Verified pinned unpacked SHA-256 for ' + label)
    return {'archive_sha256': archive_hash, 'nix_unpacked_sha256': actual}


def safe_extract(archive_path, destination):
    destination.mkdir(parents=True, exist_ok=True)
    try:
        with tarfile.open(archive_path, 'r:gz') as archive:
            archive.extractall(destination, filter='data')
    except (OSError, tarfile.TarError, ValueError) as error:
        fail(f'could not safely extract {archive_path.name}: {error}')


def strip_single_root(archive_path, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='skia-extract-', dir=destination.parent) as temporary:
        unpacked = Path(temporary) / 'unpacked'
        safe_extract(archive_path, unpacked)
        entries = list(unpacked.iterdir())
        if len(entries) != 1 or not entries[0].is_dir():
            fail(f'{archive_path.name} should contain exactly one top-level directory')
        root = entries[0]
        destination.mkdir(parents=True, exist_ok=True)
        for entry in root.iterdir():
            move_path(entry, destination / entry.name)


def source_manifest(source_hash, dependency_hashes):
    return {
        'revision': '6a8fe80eadfac737c12e64229a1b9530bc78f3d1',
        'source_sha256': source_hash,
        'dependencies': dependency_hashes,
        'prepare_script_sha256': digest(__file__),
    }


def source_tree_is_usable():
    required = [SOURCE_TREE / 'DEPS', SOURCE_TREE / 'include/core/SkCanvas.h']
    required.extend(SOURCE_TREE / 'third_party/externals' / name for name, *_ in DEPENDENCIES)
    return all(path.exists() for path in required)


def ensure_source():
    source_hash = download(SOURCE_URL, SOURCE_ARCHIVE, SOURCE_SHA256, 'pinned Skia source')
    dependency_hashes = {}
    archives = SOURCE_CACHE / 'archives'
    for name, url, nix_hash, has_root in DEPENDENCIES:
        dependency_hashes[name] = download_nix_archive(
            url, archives / f'{name}.tar.gz', nix_hash, f'pinned {name} archive',
            hash_strip_root=has_root)

    expected = source_manifest(source_hash, dependency_hashes)
    manifest_path = SOURCE_CACHE / 'source.json'
    previous = None
    if manifest_path.is_file():
        try:
            previous = json.loads(manifest_path.read_text(encoding='utf-8'))
        except (OSError, json.JSONDecodeError):
            previous = None
    if previous == expected and source_tree_is_usable():
        say('Pinned source tree is already laid out')
        return expected

    say('Extracting the pinned Skia source and its seven externals')
    staging = SOURCE_CACHE / 'tree.new'
    remove_tree(staging, ignore_errors=True)
    staging.mkdir(parents=True)
    try:
        strip_single_root(SOURCE_ARCHIVE, staging)
        external = staging / 'third_party/externals'
        for name, _, _, has_root in DEPENDENCIES:
            archive_path = archives / f'{name}.tar.gz'
            target = external / name
            if has_root:
                strip_single_root(archive_path, target)
            else:
                safe_extract(archive_path, target)
    except BaseException:
        remove_tree(staging, ignore_errors=True)
        raise

    if not (staging / 'DEPS').is_file() or not (staging / 'include/core/SkCanvas.h').is_file():
        remove_tree(staging, ignore_errors=True)
        fail('the pinned Skia source archive did not extract to the expected source layout')
    replace_path(staging, SOURCE_TREE, tree=True)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    write_json(manifest_path, expected)
    say('Skia source is ready at ' + str(SOURCE_TREE))
    return expected


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    os.replace(temporary, path)


def find_executable(name, advice):
    found = shutil.which(name)
    if not found:
        fail(f'{name} was not found on PATH; {advice}')
    # Keep the executable's filename: Rustup's cargo.exe/rustc.exe proxies are
    # hard links to rustup.exe and dispatch by the invoked name.
    return Path(os.path.abspath(found))


def run_output(command, label):
    try:
        result = subprocess.run(command, cwd=ROOT, check=True, text=True,
                                capture_output=True, encoding='utf-8', errors='replace')
    except (OSError, subprocess.CalledProcessError) as error:
        detail = (getattr(error, 'stdout', '') or getattr(error, 'stderr', '') or str(error))
        fail(f'could not query {label}: {detail.strip()}')
    return result.stdout.strip()


def find_vswhere():
    candidates = []
    on_path = shutil.which('vswhere.exe') or shutil.which('vswhere')
    if on_path:
        candidates.append(Path(on_path))
    program_files = os.environ.get('PROGRAMFILES(X86)', r'C:\Program Files (x86)')
    candidates.append(Path(program_files) / 'Microsoft Visual Studio/Installer/vswhere.exe')
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    fail('vswhere.exe is missing; install Visual Studio Build Tools with the C++ x64 tools')


def find_visual_studio(vswhere):
    output = run_output([
        str(vswhere), '-latest', '-products', '*', '-requires',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'],
        'Visual Studio installation using vswhere')
    if not output:
        fail('vswhere found no Visual Studio installation with the x64 C++ tools; install the Desktop development with C++ workload')
    install = Path(output.splitlines()[-1].strip()).resolve()
    vc = install / 'VC'
    version_file = vc / 'Auxiliary/Build/Microsoft.VCToolsVersion.default.txt'
    version = version_file.read_text(encoding='utf-8').strip() if version_file.is_file() else ''
    msvc_root = vc / 'Tools/MSVC'
    if not version or not (msvc_root / version).is_dir():
        versions = [p.name for p in msvc_root.iterdir() if p.is_dir() and re.fullmatch(r'\d+\.\d+\.\d+', p.name)] if msvc_root.is_dir() else []
        if not versions:
            fail(f'no MSVC toolset version was found under {msvc_root}; install the VS x64 C++ tools')
        version = max(versions, key=lambda v: tuple(int(n) for n in v.split('.')))
    vcvars = vc / 'Auxiliary/Build/vcvarsall.bat'
    if not vcvars.is_file():
        fail(f'vcvarsall.bat is missing at {vcvars}; repair the Visual Studio C++ tools installation')
    return install, vc, version, vcvars


def visual_studio_environment(vcvars):
    command = shutil.which('cmd.exe') or str(Path(os.environ.get('SYSTEMROOT', r'C:\Windows')) / 'System32/cmd.exe')
    if not Path(command).is_file():
        fail('cmd.exe was not found; a native Windows command environment is required')
    try:
        with tempfile.TemporaryDirectory(prefix='vcvars-', dir=WORK) as temporary:
            batch = Path(temporary) / 'environment.cmd'
            batch.write_text(
                '@echo off\r\n'
                f'call "{vcvars}" x64 >nul\r\n'
                'if errorlevel 1 exit /b %ERRORLEVEL%\r\n'
                'set\r\n',
                encoding='utf-8')
            result = subprocess.run([command, '/d', '/c', str(batch)], cwd=ROOT,
                                    capture_output=True, encoding='mbcs', errors='replace')
    except OSError as error:
        fail(f'could not initialize the Visual Studio x64 environment: {error}')
    if result.returncode:
        fail('vcvarsall.bat x64 failed: ' + result.stderr.strip())
    environment = {}
    for line in result.stdout.splitlines():
        name, separator, value = line.partition('=')
        if separator and name and not name.startswith('='):
            environment[name] = value
    return environment


def set_env(environment, name, value):
    for existing in list(environment):
        if existing.upper() == name.upper():
            del environment[existing]
    environment[name] = str(value)


def find_sdk(vs_environment):
    sdk_root_value = next((value for name, value in vs_environment.items()
                           if name.lower() == 'windowssdkdir'), '')
    sdk_version_value = next((value for name, value in vs_environment.items()
                              if name.lower() == 'windowssdkversion'), '')
    program_files = os.environ.get('PROGRAMFILES(X86)', r'C:\Program Files (x86)')
    root = Path(sdk_root_value or (Path(program_files) / 'Windows Kits/10')).resolve()
    version = sdk_version_value.strip('\\/')
    include_root = root / 'Include'
    if not version or not (include_root / version / 'um/Windows.h').is_file():
        versions = [p.name for p in include_root.iterdir()
                    if p.is_dir() and re.fullmatch(r'\d+\.\d+\.\d+\.\d+', p.name)
                    and (p / 'um/Windows.h').is_file()] if include_root.is_dir() else []
        if not versions:
            fail(f'Windows SDK headers were not found under {include_root}; install a Windows 10/11 SDK through Visual Studio Installer')
        version = max(versions, key=lambda v: tuple(int(n) for n in v.split('.')))
    required = [root / 'Lib' / version / 'um/x64/kernel32.lib',
                root / 'Lib' / version / 'ucrt/x64/ucrt.lib']
    if not all(path.is_file() for path in required):
        fail(f'Windows SDK {version} is incomplete under {root}; repair it through Visual Studio Installer')
    return root, version


def find_llvm():
    clang = find_executable('clang-cl.exe', 'install LLVM for Windows and add its bin directory to PATH')
    llvm_bin = clang.parent
    root = llvm_bin.parent
    required = ['lld-link.exe', 'llvm-nm.exe', 'llvm-objcopy.exe', 'llvm-readobj.exe', 'libclang.dll']
    missing = [name for name in required if not (llvm_bin / name).is_file()]
    if missing:
        fail(f'LLVM at {llvm_bin} is missing {", ".join(missing)}; install a complete LLVM package')
    clang_dirs = root / 'lib/clang'
    versions = [p.name for p in clang_dirs.iterdir() if p.is_dir() and re.fullmatch(r'\d+(?:\.\d+)*', p.name)] if clang_dirs.is_dir() else []
    if not versions:
        fail(f'Clang resource headers were not found under {clang_dirs}; install the complete LLVM package')
    version = max(versions, key=lambda v: tuple(int(n) for n in v.split('.')))
    if not (clang_dirs / version / 'include').is_dir():
        fail(f'Clang resource headers are missing at {clang_dirs / version / "include"}')
    return {
        'root': root.resolve(),
        'bin': llvm_bin.resolve(),
        'clang': clang,
        'lld': (llvm_bin / 'lld-link.exe').resolve(),
        'nm': (llvm_bin / 'llvm-nm.exe').resolve(),
        'objcopy': (llvm_bin / 'llvm-objcopy.exe').resolve(),
        'readobj': (llvm_bin / 'llvm-readobj.exe').resolve(),
        'version': version.split('.')[0],
    }


def find_ninja(vs_install):
    vs_ninja = vs_install / 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe'
    if vs_ninja.is_file():
        return vs_ninja.resolve()
    return find_executable('ninja.exe', 'install Ninja with Visual Studio or add ninja.exe to PATH')


def find_gn():
    pinned = SOURCE_TREE / 'bin/gn.exe'
    if pinned.is_file():
        return pinned.resolve()

    fetch_script = SOURCE_TREE / 'bin/fetch-gn'
    if not fetch_script.is_file():
        fail(f'the pinned source has no GN fetch script at {fetch_script}; install GN and put gn.exe on PATH')
    say('Fetching GN pinned by the Skia source')
    try:
        result = subprocess.run([sys.executable, str(fetch_script)], cwd=SOURCE_TREE,
                                text=True, capture_output=True, encoding='utf-8', errors='replace')
    except OSError as error:
        fail(f'could not start the pinned GN fetch: {error}; install GN and put gn.exe on PATH')
    if result.returncode or not pinned.is_file():
        detail = (result.stderr or result.stdout).strip()
        if detail:
            detail = ': ' + detail
        fail(f'could not fetch the pinned GN from chrome-infra-packages{detail}; install GN and put gn.exe on PATH')
    return pinned.resolve()


def version_of(path, *args):
    return run_output([str(path), *args], path.name)


def gn_quote(path):
    return json.dumps(str(path))


def make_gn_args(vc, toolset_version, sdk, sdk_version, llvm):
    return ' '.join([
        'target_os="win"',
        'target_cpu="x64"',
        'win_vc=' + gn_quote(vc),
        'win_toolchain_version=' + json.dumps(toolset_version),
        'win_sdk=' + gn_quote(sdk),
        'win_sdk_version=' + json.dumps(sdk_version),
        'clang_win=' + gn_quote(llvm['root']),
        'clang_win_version=' + json.dumps(llvm['version']),
        'is_trivial_abi=false',
        'extra_cflags+=["/MT", "-D__RTMINTRIN_H"]',
    ])


def build_inputs(source, jobs, tools, gn_args, ninja_wrapper):
    builder = ROOT / 'nix/skia-windows-builder'
    return {
        'skia_source_revision': '6a8fe80eadfac737c12e64229a1b9530bc78f3d1',
        'source_sha256': source['source_sha256'],
        'dependency_sha256': source['dependencies'],
        'tool_versions': tools,
        'gn_args': gn_args,
        'jobs': jobs,
        'rustflags': '-C target-feature=+crt-static',
        'prepare_script_sha256': digest(__file__),
        'native_prepare_script_sha256': digest(ROOT / 'nix/prepare-skia-native.py'),
        'builder_manifest_sha256': digest(builder / 'Cargo.toml'),
        'builder_lock_sha256': digest(builder / 'Cargo.lock'),
        'builder_source_sha256': digest(builder / 'src/lib.rs'),
        'ninja_wrapper_sha256': digest(ninja_wrapper),
    }


def make_ninja_wrapper(path, ninja, jobs):
    # The builder invokes its Ninja command as an executable and supplies no -j
    # option, so use this small batch launcher to apply the requested bound.
    content = ('@echo off\r\n'
               'setlocal\r\n'
               f'"{ninja}" -j{jobs} %*\r\n'
               'exit /b %ERRORLEVEL%\r\n')
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file() or path.read_text(encoding='utf-8') != content:
        path.write_text(content, encoding='utf-8', newline='')
    return path


def prepare_toolchain(jobs):
    vswhere = find_vswhere()
    install, vc, toolset_version, vcvars = find_visual_studio(vswhere)
    vs_environment = visual_studio_environment(vcvars)
    sdk, sdk_version = find_sdk(vs_environment)
    llvm = find_llvm()
    cargo = find_executable('cargo.exe', 'install Rust using rustup and add cargo to PATH')
    rustc = find_executable('rustc.exe', 'install Rust using rustup and add rustc to PATH')
    ninja = find_ninja(install)
    gn = find_gn()
    ninja_wrapper = make_ninja_wrapper(WORK / f'ninja-jobs-{jobs}.bat', ninja, jobs)
    gn_args = make_gn_args(vc, toolset_version, sdk, sdk_version, llvm)

    environment = dict(os.environ)
    for name, value in vs_environment.items():
        set_env(environment, name, value)
    old_path = next((value for name, value in environment.items() if name.upper() == 'PATH'), '')
    set_env(environment, 'PATH', str(llvm['bin']) + os.pathsep + old_path)
    set_env(environment, 'LIBCLANG_PATH', llvm['bin'])
    # skia-bindings reads LLVM_HOME before C:\Program Files\LLVM, C:\LLVM and
    # %USERPROFILE%\scoop\apps\llvm\current, and an LLVM that is only on PATH
    # is none of those.
    set_env(environment, 'LLVM_HOME', str(llvm['root']))
    set_env(environment, 'CC', llvm['clang'])
    set_env(environment, 'CXX', llvm['clang'])
    set_env(environment, 'SKIA_SOURCE_DIR', SOURCE_TREE)
    set_env(environment, 'SKIA_GN_ARGS', gn_args)
    set_env(environment, 'SKIA_GN_COMMAND', gn)
    set_env(environment, 'SKIA_NINJA_COMMAND', ninja_wrapper)
    set_env(environment, 'BUILD_ARTIFACTSTAGINGDIRECTORY', WORK / 'export')
    set_env(environment, 'CARGO_TARGET_DIR', '.local/windows/skia/target')
    set_env(environment, 'RUSTFLAGS', '-C target-feature=+crt-static')
    # Source mode builds locally regardless of a user-level Cargo cache URL.
    for name in ['SKIA_BINARIES_URL', 'SKIA_DEBUG', 'SKIA_LIBRARY_SEARCH_PATH', 'SKIA_USE_SYSTEM_LIBRARIES']:
        for existing in list(environment):
            if existing.upper() == name.upper():
                del environment[existing]

    tools = {
        'vswhere': {'path': str(vswhere)},
        'visual_studio': {'path': str(install), 'msvc_version': toolset_version},
        'windows_sdk': {'path': str(sdk), 'version': sdk_version},
        'llvm': {'path': str(llvm['root']),
                 'clang_cl': version_of(llvm['clang'], '--version'),
                 'lld_link': version_of(llvm['lld'], '--version'),
                 'llvm_nm': version_of(llvm['nm'], '--version'),
                 'llvm_objcopy': version_of(llvm['objcopy'], '--version'),
                 'llvm_readobj': version_of(llvm['readobj'], '--version')},
        'gn': {'path': str(gn), 'version': version_of(gn, '--version')},
        'ninja': {'path': str(ninja), 'version': version_of(ninja, '--version')},
        'cargo': {'path': str(cargo), 'version': version_of(cargo, '--version')},
        'rustc': {'path': str(rustc), 'version': version_of(rustc, '--version')},
        'python': {'path': sys.executable, 'version': sys.version.splitlines()[0]},
    }
    return environment, cargo, llvm, gn_args, ninja_wrapper, tools


def cargo_builder(cargo, environment):
    log_path = WORK / 'builder-cargo.jsonl'
    command = [str(cargo), 'build', '--release', '--locked',
               '--manifest-path', str(ROOT / 'nix/skia-windows-builder/Cargo.toml'),
               '--message-format=json-render-diagnostics']
    say('Building pinned Windows Skia with cargo build --release (' + str(environment['SKIA_NINJA_COMMAND']) + ')')
    outputs = []
    with log_path.open('w', encoding='utf-8') as log:
        try:
            process = subprocess.Popen(command, cwd=ROOT, env=environment,
                                       stdout=subprocess.PIPE, stderr=None,
                                       text=True, encoding='utf-8', errors='replace',
                                       bufsize=1)
        except OSError as error:
            fail(f'could not start cargo for the Skia builder: {error}')
        if process.stdout is None:
            fail('cargo builder output was not captured')
        for line in process.stdout:
            log.write(line)
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if message.get('reason') == 'compiler-message':
                rendered = message.get('message', {}).get('rendered')
                if rendered:
                    print(rendered, file=sys.stderr, end='', flush=True)
            if (message.get('reason') == 'build-script-executed'
                    and 'skia-bindings@0.157.0' in message.get('package_id', '')):
                outputs.append(Path(message['out_dir']) / 'skia')
        return_code = process.wait()
    if return_code:
        fail(f'the Skia builder cargo build failed with exit code {return_code}; see {log_path}')
    if len(outputs) != 1:
        fail(f'could not identify the pinned skia-bindings build output in Cargo messages; see {log_path}')
    return outputs[0]


def create_raw_archive(native_output, source_tree):
    native = WORK / 'export/skia-binaries'
    remove_tree(native, ignore_errors=True)
    native.mkdir(parents=True, exist_ok=True)
    for name in ['skia.lib', 'skia-bindings.lib', 'bindings.rs']:
        origin = native_output / name
        if not origin.is_file():
            fail(f'the builder did not produce {origin}; check the Cargo build output')
        shutil.copy2(origin, native / name)
    license_file = source_tree / 'LICENSE'
    if not license_file.is_file():
        fail(f'pinned Skia license is missing at {license_file}')
    shutil.copy2(license_file, native / 'LICENSE_SKIA')
    (native / 'key.txt').write_text(KEY + '\n', encoding='utf-8')
    (native / 'tag.txt').write_text('0.157.0\n', encoding='utf-8')

    raw = WORK / PACKAGE
    temporary = raw.with_name(raw.name + '.tmp')
    with (temporary.open('wb') as output,
          gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=0) as compressed,
          tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as archive):
        archive.add(native, arcname='skia-binaries', recursive=True, filter=normalized_tar_info)
    replace_path(temporary, raw)
    return raw


def normalized_tar_info(info):
    info.uid = info.gid = info.mtime = 0
    info.uname = info.gname = ''
    return info


def run_native_prepare(raw, raw_hash, llvm):
    output = WORK / 'prepared'
    helper = ROOT / 'nix/prepare-skia-native.py'
    environment = dict(os.environ)
    environment.update({
        'YAPFAST_SKIA_TARGET': 'windows',
        'YAPFAST_SKIA_SOURCE': str(raw),
        'YAPFAST_SKIA_OUTPUT': str(output),
        'YAPFAST_SKIA_STATIC_SHA256': raw_hash,
        'YAPFAST_SKIA_NM': str(llvm['nm']),
        'YAPFAST_SKIA_OBJCOPY': str(llvm['objcopy']),
        'PATH': str(llvm['bin']) + os.pathsep + os.environ.get('PATH', ''),
    })
    say('Renaming the JPEG table and checking the native archive with nix/prepare-skia-native.py')
    result = subprocess.run([sys.executable, str(helper)], cwd=ROOT, env=environment)
    if result.returncode:
        fail(f'nix/prepare-skia-native.py failed with exit code {result.returncode}; inspect the helper diagnostics above')
    prepared = output / PACKAGE
    helper_stamp = output / 'yapfast-skia-native.json'
    if not prepared.is_file() or not helper_stamp.is_file():
        fail('the native Skia helper returned without producing its archive and stamp')
    return prepared, helper_stamp


def read_json(path):
    try:
        return json.loads(Path(path).read_text(encoding='utf-8'))
    except (OSError, json.JSONDecodeError):
        return None


def existing_build_is_current(stamp_path, inputs, raw, prepared, helper_stamp):
    previous = read_json(stamp_path)
    if not (previous and previous.get('inputs') == inputs
            and raw.is_file() and prepared.is_file() and helper_stamp.is_file()):
        return False
    try:
        native_stamp = read_json(helper_stamp)
        return (digest(raw) == previous.get('raw_sha256')
                and digest(prepared) == previous.get('prepared_sha256')
                and digest(helper_stamp) == previous.get('native_prepare_stamp_sha256')
                and native_stamp is not None
                and native_stamp.get('upstream_sha256') == previous.get('raw_sha256')
                and native_stamp.get('private_archive_sha256') == previous.get('prepared_sha256'))
    except OSError:
        return False


def archive_uri():
    """The prepared archive as skia-bindings reads a `file:` URL.

    It strips `file://` and opens what is left as a path, so the drive letter
    has to follow directly: `as_uri()` gives `file:///C:/...`, whose third
    slash leaves `/C:/...`, which cannot be opened. A quote in the path is
    percent-encoded, since `--print-env` output is pasted into a shell.
    """
    return 'file://' + str((WORK / 'prepared' / PACKAGE).resolve().as_posix()).replace("'", '%27')


def print_environment():
    # Only the archive: the build scripts pick clang-cl for MSVC by themselves
    # when nothing set CC/CXX, and a compiler the person chose must win.
    print("$env:SKIA_BINARIES_URL = '" + archive_uri() + "'")


def run_application_build(cargo):
    # SKIA_BINARIES_URL is this process's only addition: .cargo/config.toml
    # already asks for the static CRT, and the build scripts choose the
    # compiler, so the build sees the environment a plain `cargo build` does.
    # Not the Skia builder's: skia-bindings would take its SKIA_SOURCE_DIR over
    # the archive and compile Skia again, without the renamed JPEG table.
    environment = dict(os.environ)
    environment['SKIA_BINARIES_URL'] = archive_uri()
    say('Building the application with the prepared archive (SKIA_BINARIES_URL is process-local)')
    subprocess.run([str(cargo), 'build', '--release'], cwd=ROOT, env=environment, check=True)


def main():
    parser = argparse.ArgumentParser(description='Build Yapfast\'s pinned static-CRT Windows Skia archive on native Windows.')
    parser.add_argument('--jobs', type=int, default=os.cpu_count() or 1,
                        help='maximum Ninja jobs (default: host CPU count)')
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--fetch-only', action='store_true',
                       help='download and lay out the pinned source and externals, then stop')
    modes.add_argument('--print-env', action='store_true',
                       help='print the PowerShell line that sets SKIA_BINARIES_URL to the prepared archive')
    modes.add_argument('--build', action='store_true',
                       help='prepare Skia, then run cargo build --release with its archive')
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be at least 1')
    if args.print_env:
        print_environment()
        return
    if os.name != 'nt':
        fail('this preparation script must run on a native Windows host')

    source = ensure_source()
    if args.fetch_only:
        find_gn()
        say('Fetch-only complete; no toolchain or build was started')
        print(SOURCE_TREE)
        return

    environment, cargo, llvm, gn_args, ninja_wrapper, tools = prepare_toolchain(args.jobs)
    inputs = build_inputs(source, args.jobs, tools, gn_args, ninja_wrapper)
    raw = WORK / PACKAGE
    prepared = WORK / 'prepared' / PACKAGE
    helper_stamp = WORK / 'prepared/yapfast-skia-native.json'
    stamp_path = WORK / 'build.json'
    if existing_build_is_current(stamp_path, inputs, raw, prepared, helper_stamp):
        say('The prepared Skia archive matches every stamped input; nothing to rebuild')
        print(prepared)
    else:
        if stamp_path.is_file():
            say('Build inputs changed; clearing the isolated Skia Cargo target')
            remove_tree(WORK / 'target', ignore_errors=True)
        remove_tree(WORK / 'export', ignore_errors=True)
        output = cargo_builder(cargo, environment)
        raw = create_raw_archive(output, SOURCE_TREE)
        raw_hash = digest(raw)
        prepared, helper_stamp = run_native_prepare(raw, raw_hash, llvm)
        stamp = {
            'inputs': inputs,
            'raw_sha256': raw_hash,
            'prepared_sha256': digest(prepared),
            'native_prepare_stamp_sha256': digest(helper_stamp),
        }
        write_json(stamp_path, stamp)
        say('Prepared and stamped ' + str(prepared))
        print(prepared)

    if args.build:
        run_application_build(cargo)


if __name__ == '__main__':
    main()
