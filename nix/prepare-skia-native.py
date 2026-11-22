#!/usr/bin/env python3
"""Privately namespace one Skia JPEG table without changing WebRTC or Cargo's cache."""
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

# What the checks below compare before and after the rename: an archive's
# members by name, and for each member its sections by name. A section entry
# holds the section's identity and flags with the digests of what it carries;
# an ELF member and a COFF member describe one differently, and a member the
# helpers do not parse keeps one digest of all its bytes.
Payloads = dict[str, object]
ArchiveMembers = dict[str, Payloads]

native_target: str = os.environ.get('YAPFAST_SKIA_TARGET', 'linux')
pins: dict[str, tuple[str, str | None]] = {
    'linux': ('x86_64-unknown-linux-gnu-ftembed-ganesh-gl-jpegd-jpege-pdf-textlayout',
              'd5ea4666b67f4ff74a2c9dccf9cbb069fc991552caa9ed26fd4379572ddfe3b1'),
    'windows': ('x86_64-pc-windows-msvc-d3d-ganesh-gl-jpegd-jpege-pdf-static',
                os.environ.get('YAPFAST_SKIA_STATIC_SHA256')),
}
suffix, source_sha256 = pins[native_target]
if not source_sha256:
    raise SystemExit('Windows preparation requires the verified source-build archive')
key: str = '30669978c3f39a73e8a7-' + suffix
package: str = 'skia-binaries-' + key + '.tar.gz'
old_symbol: str = 'jpeg_natural_order'
new_symbol: str = 'yapfast_skia_jpeg_natural_order'
target: Path = Path(os.environ['YAPFAST_SKIA_OUTPUT'])
archive_path: Path = target / package
upstream_path: Path = target / 'upstream' / package
stamp_path: Path = target / 'yapfast-skia-native.json'
script_sha256: str = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
objcopy: str | None = os.environ.get('YAPFAST_SKIA_OBJCOPY') or shutil.which('llvm-objcopy')
if not objcopy:
    raise SystemExit('Needs LLVM 21 or later objcopy, from the development shell or YAPFAST_SKIA_OBJCOPY')
objcopy_version: str = subprocess.check_output([objcopy, '--version'], text=True).strip()
# Nix pins 21.1.7, but a native host's LLVM is often newer; the checks for the
# symbols and sections below, not the version string, are what keep the
# rename to Skia's one JPEG table.
version: re.Match[str] | None = re.search(r'LLVM version ([0-9]+)', objcopy_version)
objcopy_major: int
try:
    objcopy_major = int(version.group(1)) if version else 0
except ValueError:
    objcopy_major = 0
if objcopy_major < 21:
    raise SystemExit('Unexpected objcopy version: ' + objcopy_version)
stamp: dict[str, object] = {
    'tag': '0.157.0', 'key': key, 'upstream_sha256': source_sha256,
    'helper_sha256': script_sha256, 'objcopy_version': objcopy_version,
    'rename': {old_symbol: new_symbol},
}
if stamp_path.is_file() and archive_path.is_file() and upstream_path.is_file():
    # A stamp an interrupted run left is no stamp: build it again.
    try:
        previous: dict[str, object] = json.loads(stamp_path.read_text())
    except (OSError, ValueError):
        previous = {}
    if (all(previous.get(k) == v for k, v in stamp.items())
        and hashlib.sha256(archive_path.read_bytes()).hexdigest() == previous.get('private_archive_sha256')
        and hashlib.sha256(upstream_path.read_bytes()).hexdigest() == source_sha256):
        print(archive_path)
        raise SystemExit(0)

def symbols(path: Path) -> list[str]:
    result = subprocess.run([os.environ.get('YAPFAST_SKIA_NM', 'nm'), '-A', str(path)], check=True, text=True, capture_output=True)
    # Ignore the varying archive path; preserve each member and its symbol table.
    return sorted(line[len(str(path)):] for line in result.stdout.splitlines())

def elf_payloads(data: bytes) -> Payloads:
    """Executable/data/relocation bytes must remain identical after the rename."""
    if data[:6] != b'\x7fELF\x02\x01':
        raise SystemExit('Expected little-endian ELF64 archive member')
    header = struct.unpack_from('<16sHHIQQQIHHHHHH', data)
    table_offset, entry_size, count, string_index = header[6], header[11], header[12], header[13]
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, table_offset + i * entry_size) for i in range(count)]
    strings = sections[string_index]
    names = data[strings[4]:strings[4] + strings[5]]
    result: Payloads = {}
    for section in sections:
        name = names[section[0]:].split(b'\0', 1)[0].decode()
        # Only symbol and string tables may change. NOBITS has no file bytes.
        if section[1] not in (0, 2, 3):
            payload = b'' if section[1] == 8 else data[section[4]:section[4] + section[5]]
            result[name] = (section[1:4], section[5:10], hashlib.sha256(payload).hexdigest())
    return result

def coff_payloads(data: bytes) -> Payloads:
    """COFF code, data and relocations must also survive the symbol rename."""
    if data[:4] == b'\x00\x00\xff\xff' and struct.unpack_from('<H', data, 4)[0] >= 2:
        header = struct.unpack_from('<HHHHI16sIIIIIII', data)
        section_offset, count, symbols_offset, symbols_count, symbol_size = 56, header[10], header[11], header[12], 20
    elif data[:2] == b'\x64\x86':
        header = struct.unpack_from('<HHIIIHH', data)
        section_offset, count, symbols_offset, symbols_count, symbol_size = 20 + header[5], header[1], header[3], header[4], 18
    else:
        return {'opaque': hashlib.sha256(data).hexdigest()}
    strings_offset = symbols_offset + symbols_count * symbol_size
    strings_length = struct.unpack_from('<I', data, strings_offset)[0] if symbols_offset else 0
    strings = data[strings_offset:strings_offset + strings_length]
    result: Payloads = {}
    for index in range(count):
        section = struct.unpack_from('<8sIIIIIIHHI', data, section_offset + index * 40)
        name = section[0].rstrip(b'\0')
        if name.startswith(b'/'):
            name = strings[int(name[1:]):].split(b'\0', 1)[0]
        raw = data[section[4]:section[4] + section[3]] if section[4] else b''
        # COFF BSS is uninitialized data. LLVM may materialize zero padding when
        # rewriting an archive; its allocation and flags must remain identical.
        if section[9] & 0x80:
            if any(raw):
                raise SystemExit('Unexpected nonzero COFF BSS bytes')
            raw = b''
        relocations = data[section[5]:section[5] + section[7] * 10] if section[5] else b''
        lines = data[section[6]:section[6] + section[8] * 6] if section[6] else b''
        result[f'{index}:{name.decode()}'] = (section[1:4], section[7:10],
                                hashlib.sha256(raw).hexdigest(),
                                hashlib.sha256(relocations).hexdigest(),
                                hashlib.sha256(lines).hexdigest())
    return result

def archive_members(path: Path) -> ArchiveMembers:
    data = path.read_bytes()
    if data[:8] != b'!<arch>\n':
        raise SystemExit('Expected a regular native archive')
    members: ArchiveMembers = {}
    names: bytes = b''
    position: int = 8
    while position < len(data):
        header = data[position:position + 60]
        if len(header) != 60 or header[58:] != b'`\n':
            raise SystemExit('Malformed native archive member')
        size = int(header[48:58])
        name = header[:16].decode().strip()
        payload = data[position + 60:position + 60 + size]
        if name == '//':
            names = payload
        elif name not in ('/', '/SYM64/'):
            if name.startswith('/'):
                name = names[int(name[1:]):].split(b'/\n', 1)[0].split(b'\0', 1)[0].decode()
            else:
                name = name.rstrip('/')
            if name in members:
                raise SystemExit('Duplicate archive member: ' + name)
            members[name] = elf_payloads(payload) if native_target == 'linux' else coff_payloads(payload)
        position += 60 + size + size % 2
    return members

target.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='skia-native-prepare-', dir=target.parent) as temporary_directory:
    temporary: Path = Path(temporary_directory)
    # Download from the upstream release, rather than modifying a Cargo build output.
    source: bytes = Path(os.environ['YAPFAST_SKIA_SOURCE']).read_bytes()
    if hashlib.sha256(source).hexdigest() != source_sha256:
        raise SystemExit('Pinned Skia native archive checksum mismatch')
    with tarfile.open(fileobj=io.BytesIO(source)) as archive:
        archive.extractall(temporary, filter='data')
    native_root = temporary / 'skia-binaries'
    if (native_root / 'key.txt').read_text().strip() != key or (native_root / 'tag.txt').read_text().strip() != '0.157.0':
        raise SystemExit('Pinned Skia native archive key/tag mismatch')
    original_symbols: dict[str, list[str]] = {}
    original_payloads: dict[str, ArchiveMembers] = {}
    hashes: dict[str, dict[str, str]] = {}
    libraries = sorted(native_root.glob('*.a' if native_target == 'linux' else '*.lib'))
    if native_target == 'windows':
        for library in libraries:
            directives = subprocess.check_output(['llvm-readobj', '--coff-directives', str(library)], text=True)
            if 'MD_Dynamic' in directives or 'MT_StaticRelease' not in directives:
                raise SystemExit('Skia must use the same static release CRT as WebRTC')
    for library in libraries:
        original_symbols[library.name] = symbols(library)
        original_payloads[library.name] = archive_members(library)
        hashes[library.name] = {'upstream_sha256': hashlib.sha256(library.read_bytes()).hexdigest()}
    references: list[str] = [line for lines in original_symbols.values() for line in lines if line.split()[-1:] == [old_symbol]]
    if len(references) != 9 or sum(' R ' in line for line in references) != 1:
        raise SystemExit('Unexpected pinned JPEG table definitions/references: ' + repr(references))
    for library in libraries:
        expected = [line[:-len(old_symbol)] + new_symbol if line.split()[-1:] == [old_symbol] else line for line in original_symbols[library.name]]
        if expected != original_symbols[library.name]:
            subprocess.run([objcopy, '--redefine-sym', old_symbol + '=' + new_symbol, str(library)], check=True)
        if symbols(library) != sorted(expected):
            raise SystemExit('Native archive symbol change exceeded the one-table rename')
        actual_payloads = archive_members(library)
        if actual_payloads != original_payloads[library.name]:
            for member, sections in original_payloads[library.name].items():
                for name, expected_section in sections.items():
                    actual_section = actual_payloads.get(member, {}).get(name)
                    if expected_section != actual_section:
                        raise SystemExit('Native section changed: ' + repr((member, name, expected_section, actual_section)))
            raise SystemExit('Native executable/data/relocation sections changed')
        hashes[library.name]['private_sha256'] = hashlib.sha256(library.read_bytes()).hexdigest()
    prepared = temporary / package
    # Deterministic tar/gzip metadata keeps the private archive reproducible.
    with (
        prepared.open('wb') as output,
        gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=0) as compressed,
        tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as archive,
    ):
        for path in [native_root] + sorted(native_root.rglob('*')):
            info = archive.gettarinfo(str(path), str(path.relative_to(temporary)))
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ''
            if path.is_file():
                with path.open('rb') as content:
                    archive.addfile(info, content)
            else:
                archive.addfile(info)
    stamp.update({'libraries': hashes, 'renamed_references': len(references), 'private_archive_sha256': hashlib.sha256(prepared.read_bytes()).hexdigest()})
    target.mkdir(parents=True, exist_ok=True)
    upstream_path.parent.mkdir(parents=True, exist_ok=True)
    upstream_path.write_bytes(source)
    os.replace(prepared, archive_path)
    stamp_path.write_text(json.dumps(stamp, indent=2) + '\n')
print('Prepared private Skia JPEG table namespace; native code/data unchanged', file=sys.stderr)
print(archive_path)
