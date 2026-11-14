#!/usr/bin/env python3
"""Build pinned Skia for Linux from source with Vulkan, then namespace its JPEG table."""
import contextlib
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / '.local/linux/skia-vulkan-production'
# The archive's key, as `skia-bindings` names it: the rust-skia revision, the
# target, then the features the build asked for. `ftembed` and `gl` keep the
# OpenGL fallback and Skia's own FreeType; `vulkan` is what the GL-only prebuilt
# lacks.
KEY = '30669978c3f39a73e8a7-x86_64-unknown-linux-gnu-ftembed-ganesh-gl-jpegd-jpege-pdf-textlayout-vulkan'
PACKAGE = 'skia-binaries-' + KEY + '.tar.gz'


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def parse_message(line: str) -> dict[str, Any] | None:
    """One line of `cargo build --message-format=json`, if it is a message."""
    if not line.startswith('{'):
        return None
    try:
        return json.loads(line)
    except ValueError:
        return None


def tool(name: str) -> str:
    """A command the dev shell provides, or the reason the build cannot run."""
    path = shutil.which(name)
    if not path:
        raise SystemExit('Needs ' + name + ', from the development shell')
    return path


def make_writable(path: Path) -> None:
    """Undo the read-only mode the store's tree has, which Skia's build writes over."""
    if not path.is_dir():
        return
    for entry in [path] + sorted(path.rglob('*')):
        with contextlib.suppress(OSError):
            entry.chmod(entry.stat().st_mode | 0o700)


def main():
    source = Path(os.environ['YAPFAST_SKIA_LINUX_SOURCE'])
    # The pinned `nix build path:./nix#skia-source` tree is read-only and made
    # of symlinks; Skia's build writes into its own source tree.
    skia_source = WORK / 'source'
    jobs = os.environ.get('YAPFAST_SKIA_JOBS', '6')
    inputs = {
        'skia_source': str(source), 'skia_revision': '6a8fe80eadfac737c12e64229a1b9530bc78f3d1',
        'builder_lock_sha256': digest(ROOT / 'nix/skia-linux-builder/Cargo.lock'),
        'builder_manifest_sha256': digest(ROOT / 'nix/skia-linux-builder/Cargo.toml'),
        'helper_sha256': digest(__file__),
        'namespace_helper_sha256': digest(ROOT / 'nix/prepare-skia-native.py'),
        'ninja_jobs': jobs, 'cc': tool('clang'), 'cxx': tool('clang++'),
        'gn': tool('gn'), 'ninja': tool('ninja'),
    }
    stamp = WORK / 'build.json'
    prepared = WORK / 'prepared' / PACKAGE
    if stamp.is_file() and prepared.is_file():
        # A stamp an interrupted run left is no stamp: build it again.
        try:
            old = json.loads(stamp.read_text())
        except (OSError, ValueError):
            old = {}
        if old.get('inputs') == inputs and digest(prepared) == old.get('prepared_sha256'):
            print(prepared)
            return
    WORK.mkdir(parents=True, exist_ok=True)
    copy_stamp = WORK / 'source.json'
    previous_source = ''
    try:
        previous_source = str(json.loads(copy_stamp.read_text()).get('from', ''))
    except (OSError, ValueError):
        previous_source = ''
    if previous_source != str(source):
        print('Copying the pinned Skia source into ' + str(skia_source), file=sys.stderr)
        make_writable(skia_source)
        # copyfile, not copy2: the store's files are read-only, and Skia's
        # build writes its gn and ninja output into its own tree.
        shutil.copytree(source, skia_source, symlinks=False, dirs_exist_ok=True,
                        copy_function=shutil.copyfile)
        make_writable(skia_source)
        copy_stamp.write_text(json.dumps({'from': str(source)}) + '\n')
    extra = ''
    env: dict[str, str] = dict(os.environ, SKIA_SOURCE_DIR=str(skia_source),
                               SKIA_GN_ARGS=extra, SKIA_GN_COMMAND=tool('gn'),
                               SKIA_NINJA_COMMAND=str(WORK / 'ninja'),
                               CARGO_TARGET_DIR=str(WORK / 'target'),
                               # Skia's build passes `--target=x86_64-linux-gnu`,
                               # which clang takes and gcc does not.
                               CC='clang', CXX='clang++')
    for name in ['SKIA_BINARIES_URL', 'SKIA_DEBUG', 'SKIA_LIB_SEARCH_PATH',
                 'SKIA_USE_SYSTEM_LIBRARIES', 'CLANGCC', 'CLANGCXX']:
        env.pop(name, None)
    ninja = WORK / 'ninja'
    ninja.write_text('#!/bin/sh\nexec ' + json.dumps(tool('ninja')) + ' -j' + jobs + ' "$@"\n')
    ninja.chmod(0o755)
    print('Building pinned Linux Skia with Vulkan (' + jobs + ' native jobs)', file=sys.stderr)
    # Release, not the default profile: cargo's OPT_LEVEL of 0 reaches Skia's
    # cflags, and a Skia built at -O0 draws the app's every frame.
    result = subprocess.run(['cargo', 'build', '--release', '--locked', '--manifest-path',
                             str(ROOT / 'nix/skia-linux-builder/Cargo.toml'),
                             '--message-format=json-render-diagnostics'], cwd=ROOT,
                            env=env, stdout=subprocess.PIPE, text=True, check=True)
    messages = [message for message in map(parse_message, result.stdout.splitlines())
                if message is not None]
    outputs = [Path(m['out_dir']) / 'skia' for m in messages
               if m.get('reason') == 'build-script-executed'
               and '#skia-bindings@0.157.0' in m.get('package_id', '')]
    if len(outputs) != 1:
        raise SystemExit('Could not identify the pinned Skia build output')
    args_fn = WORK / 'source/out/Release/args.gn'
    if args_fn.is_file():
        print('GN arguments:\n' + args_fn.read_text(), file=sys.stderr)
    raw = WORK / PACKAGE
    subprocess.run([sys.executable, str(ROOT / 'nix/skia-linux-vulkan-archive.py'),
                    str(outputs[0]), str(skia_source / 'LICENSE'), KEY, str(raw)], check=True)
    env.update(YAPFAST_SKIA_TARGET='linux-vulkan', YAPFAST_SKIA_SOURCE=str(raw),
               YAPFAST_SKIA_SOURCE_SHA256=digest(raw),
               YAPFAST_SKIA_OUTPUT=str(WORK / 'prepared'),
               YAPFAST_SKIA_NM=tool('llvm-nm'),
               YAPFAST_SKIA_OBJCOPY=tool('llvm-objcopy'))
    subprocess.run([sys.executable, str(ROOT / 'nix/prepare-skia-native.py')],
                   env=env, stdout=sys.stderr, check=True)
    stamp.write_text(json.dumps({'inputs': inputs, 'raw_sha256': digest(raw),
                                'prepared_sha256': digest(prepared)}, indent=2) + '\n')
    print(prepared)


if __name__ == '__main__':
    main()
