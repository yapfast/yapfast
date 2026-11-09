#!/usr/bin/env python3
"""Pack skia-bindings' build output into the pinned Skia archive for Linux."""
import gzip
import shutil
import sys
import tarfile
import tempfile
from pathlib import Path


def main() -> None:
    built = Path(sys.argv[1])
    license_file = Path(sys.argv[2])
    key = sys.argv[3]
    output = Path(sys.argv[4])
    for name in ['libskia.a', 'libskia-bindings.a', 'bindings.rs']:
        if not (built / name).is_file():
            raise SystemExit('No ' + name + ' in ' + str(built) + ', was skia-bindings built?')
    with tempfile.TemporaryDirectory(dir=output.parent) as staging:
        native = Path(staging) / 'skia-binaries'
        native.mkdir()
        for name in ['libskia.a', 'libskia-bindings.a', 'bindings.rs']:
            shutil.copyfile(built / name, native / name)
        shutil.copyfile(license_file, native / 'LICENSE_SKIA')
        # skia-bindings writes the key it computed from the checkout's commit,
        # while this archive's key names the features it was built with.
        (native / 'key.txt').write_text(key)
        (native / 'tag.txt').write_text('0.157.0')
        # Fixed metadata and member order keep the archive reproducible.
        with (
            output.open('wb') as out,
            gzip.GzipFile(filename='', mode='wb', fileobj=out, mtime=0) as compressed,
            tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as tar,
        ):
            for path in [native] + sorted(native.rglob('*')):
                info = tar.gettarinfo(str(path), str(path.relative_to(native.parent)))
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                if path.is_file():
                    info.mode = 0o644
                    with path.open('rb') as content:
                        tar.addfile(info, content)
                else:
                    tar.addfile(info)


if __name__ == '__main__':
    main()
