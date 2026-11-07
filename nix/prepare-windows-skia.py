#!/usr/bin/env python3
"""Build pinned Skia with WebRTC's static Windows CRT, then namespace its JPEG table."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / '.local/windows/skia-static-production'
KEY = '30669978c3f39a73e8a7-x86_64-pc-windows-msvc-d3d-ganesh-gl-jpegd-jpege-pdf-static'
PACKAGE = 'skia-binaries-' + KEY + '.tar.gz'


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def link(target, source):
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_symlink():
        target.unlink()
    target.symlink_to(source)


def main():
    source = Path(os.environ['YAPFAST_SKIA_WINDOWS_SOURCE'])
    clang = Path(os.environ['YAPFAST_SKIA_CLANG'])
    lld = Path(os.environ['YAPFAST_SKIA_LLD'])
    headers = Path(os.environ['YAPFAST_SKIA_CLANG_HEADERS'])
    xwin = Path(os.environ.get('XWIN_CACHE_DIR', Path.home() / '.cache/cargo-xwin')) / 'xwin'
    compiler = subprocess.check_output([clang, '--version'], text=True).splitlines()[0]
    if '21.1.7' not in compiler:
        raise SystemExit('Windows Skia requires pinned Clang 21.1.7')
    inputs = {
        'skia_source': str(source), 'skia_revision': '6a8fe80eadfac737c12e64229a1b9530bc78f3d1',
        'compiler': compiler, 'clang': str(clang), 'lld': str(lld), 'headers': str(headers),
        'xwin': str(xwin), 'helper_sha256': digest(__file__),
        'namespace_helper_sha256': digest(ROOT / 'nix/prepare-skia-native.py'),
        'builder_lock_sha256': digest(ROOT / 'nix/skia-windows-builder/Cargo.lock'),
        'builder_manifest_sha256': digest(ROOT / 'nix/skia-windows-builder/Cargo.toml'),
        'crt': 'MT_StaticRelease', 'trivial_abi': False, 'ninja_jobs': 2,
    }
    sdk_files = [(str(p.relative_to(xwin)), p.stat().st_size, p.stat().st_mtime_ns)
                 for p in sorted(xwin.rglob('*')) if p.is_file()]
    inputs['sdk_fingerprint'] = hashlib.sha256(json.dumps(sdk_files).encode()).hexdigest()
    stamp = WORK / 'build.json'
    prepared = WORK / 'prepared' / PACKAGE
    if stamp.is_file() and prepared.is_file():
        old = json.loads(stamp.read_text())
        if old['inputs'] == inputs and digest(prepared) == old['prepared_sha256']:
            print(prepared)
            return
        if old['inputs'].get('sdk_fingerprint') != inputs['sdk_fingerprint']:
            shutil.rmtree(WORK / 'target', ignore_errors=True)
    WORK.mkdir(parents=True, exist_ok=True)
    vc = WORK / 'toolchain/vc'
    sdk = WORK / 'toolchain/sdk'
    llvm = WORK / 'toolchain/llvm'
    link(vc / 'Tools/MSVC/1/include', xwin / 'crt/include')
    for group in ['um', 'shared', 'ucrt', 'winrt']:
        link(sdk / 'Include/1' / group, xwin / 'sdk/include' / group)
    for group in ['um', 'ucrt']:
        link(sdk / 'Lib/1' / group, xwin / 'sdk/lib' / group)
    link(llvm / 'bin/clang-cl', clang)
    link(llvm / 'bin/lld-link', lld)
    link(llvm / 'lib/clang/21/include', headers)
    ninja = WORK / 'ninja'
    ninja.write_text('#!/bin/sh\nexec "' + shutil.which('ninja') + '" -j2 "$@"\n')
    ninja.chmod(0o755)
    args = ' '.join([
        'target_os="win"', 'target_cpu="x64"',
        'win_vc=' + json.dumps(str(vc)), 'win_toolchain_version="1"',
        'win_sdk=' + json.dumps(str(sdk)), 'win_sdk_version="1"',
        'clang_win=' + json.dumps(str(llvm)), 'clang_win_version="21"',
        'is_trivial_abi=false', 'extra_cflags+=["/MT", "-D__RTMINTRIN_H"]',
    ])
    env = dict(os.environ, SKIA_SOURCE_DIR=str(source), SKIA_GN_ARGS=args,
               SKIA_GN_COMMAND=shutil.which('gn'), SKIA_NINJA_COMMAND=str(ninja),
               BUILD_ARTIFACTSTAGINGDIRECTORY=str(WORK / 'export'),
               CARGO_TARGET_DIR=str(WORK / 'target'))
    for name in ['SKIA_BINARIES_URL', 'SKIA_DEBUG', 'SKIA_LIB_SEARCH_PATH',
                 'SKIA_USE_SYSTEM_LIBRARIES']:
        env.pop(name, None)
    print('Building pinned Windows Skia with /MT (two native jobs)', file=sys.stderr)
    result = subprocess.run(['cargo', 'xwin', 'build', '--locked', '--manifest-path',
                             str(ROOT / 'nix/skia-windows-builder/Cargo.toml'),
                             '--target', 'x86_64-pc-windows-msvc',
                             '--message-format=json-render-diagnostics'], cwd=ROOT,
                            env=env, stdout=subprocess.PIPE, text=True, check=True)
    messages = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
    outputs = [Path(m['out_dir']) / 'skia' for m in messages
               if m.get('reason') == 'build-script-executed'
               and '#skia-bindings@0.157.0' in m.get('package_id', '')]
    if len(outputs) != 1:
        raise SystemExit('Could not identify the pinned Skia build output')
    native = WORK / 'export/skia-binaries'
    native.mkdir(parents=True, exist_ok=True)
    for name in ['skia.lib', 'skia-bindings.lib', 'bindings.rs', 'LICENSE_SKIA']:
        (native / name).unlink(missing_ok=True)
    for name in ['skia.lib', 'skia-bindings.lib', 'bindings.rs']:
        shutil.copy2(outputs[0] / name, native / name)
    shutil.copy2(source / 'LICENSE', native / 'LICENSE_SKIA')
    (native / 'key.txt').write_text(KEY)
    (native / 'tag.txt').write_text('0.157.0')
    raw = WORK / PACKAGE
    with raw.open('wb') as out:
        with gzip.GzipFile(filename='', mode='wb', fileobj=out, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as tar:
                for path in [native] + sorted(native.rglob('*')):
                    info = tar.gettarinfo(str(path), str(path.relative_to(native.parent)))
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    if path.is_file():
                        with path.open('rb') as f:
                            tar.addfile(info, f)
                    else:
                        tar.addfile(info)
    env.update(YAPFAST_SKIA_TARGET='windows', YAPFAST_SKIA_SOURCE=str(raw),
               YAPFAST_SKIA_OUTPUT=str(WORK / 'prepared'), YAPFAST_SKIA_STATIC_SHA256=digest(raw),
               YAPFAST_SKIA_NM=shutil.which('llvm-nm'), YAPFAST_SKIA_OBJCOPY=shutil.which('llvm-objcopy'))
    subprocess.run([sys.executable, str(ROOT / 'nix/prepare-skia-native.py')],
                   env=env, stdout=sys.stderr, check=True)
    sdk_files = [(str(p.relative_to(xwin)), p.stat().st_size, p.stat().st_mtime_ns)
                 for p in sorted(xwin.rglob('*')) if p.is_file()]
    inputs['sdk_fingerprint'] = hashlib.sha256(json.dumps(sdk_files).encode()).hexdigest()
    stamp.write_text(json.dumps({'inputs': inputs, 'raw_sha256': digest(raw),
                                'prepared_sha256': digest(prepared)}, indent=2) + '\n')
    print(prepared)


if __name__ == '__main__':
    main()
