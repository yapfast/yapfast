#!/usr/bin/env python3
"""Check renderer selection and GPU-to-Skia-Software recovery on the private compositor.

Yapfast starts with Skia Software unless the GPU setting or --renderer gpu asks
for Skia GPU. A GPU start is
made to fail by hiding every EGL vendor from libglvnd, which is how a missing or
broken OpenGL driver appears to the window. Each scenario uses its own data
folder and captures the window for visual review.
"""
import argparse
import json
import os
import re
from pathlib import Path
import runpy
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/fast/yapfast')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
BROKEN_EGL = {'__EGL_VENDOR_LIBRARY_FILENAMES': '/nonexistent/yapfast-no-egl-vendor.json'}
results = []


def capture(name):
    with tempfile.TemporaryDirectory(prefix='yapfast-fallback-shot-') as folder:
        subprocess.run(['weston-screenshooter'], env=dict(os.environ, XDG_PICTURES_DIR=folder),
                       check=True, timeout=10, capture_output=True)
        picture = next(Path(folder).glob('*.png'))
        path = args.output / f'{name}.png'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', str(picture),
                        '-vf', 'vflip', str(path)], check=True, timeout=10)
        return path


def launch(name, data, renderer=None, environment=None, wait=2.0, keep=False):
    command = [str(args.exe.resolve()), '--preview', '--data-dir', str(data)]
    if renderer:
        command += ['--renderer', renderer]
    log_path = args.output / f'{name}.log'
    log = log_path.open('wb')
    process = subprocess.Popen(command, env=dict(os.environ, **(environment or {})),
                               stdout=log, stderr=log, start_new_session=True)
    time.sleep(wait)
    running = process.poll() is None
    picture = capture(name) if running else None
    if running and not keep:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=10)
    log.close()
    return process, running, log_path.read_text(), picture


def check(name, condition, detail):
    results.append({'scenario': name, 'passed': bool(condition), 'detail': detail})
    print(('PASS ' if condition else 'FAIL ') + name + ': ' + detail, flush=True)


def opened(log):
    plain = re.sub(r'\x1b\[[0-9;]*m', '', log)
    return [line.split('renderer=')[1].split()[0].strip('"')
            for line in plain.splitlines() if 'starting the Slint desktop window' in line]


base = Path(tempfile.mkdtemp(prefix='yapfast-fallback-data-', dir=args.output))

# 1. Without --renderer, Yapfast starts Skia Software and never tries the GPU.
data = base / 'default'
_, running, log, picture = launch('default', data)
check('default start uses Skia Software', running and opened(log) == ['skia-software'],
      f'renderers={opened(log)} screenshot={picture}')

# 2. A GPU start with a broken driver continues in Skia Software and says why.
data = base / 'broken-driver'
_, running, log, picture = launch('gpu-broken-egl', data, renderer='gpu', environment=BROKEN_EGL, wait=5.5)
check('broken EGL restarts with Skia Software',
      running and opened(log) == ['skia-opengl', 'skia-software'] and 'restarting with Skia Software' in log,
      f'running={running} renderers={opened(log)} screenshot={picture}')

# 3. A working GPU start stays on Skia GPU past the window check.
data = base / 'gpu'
_, running, log, picture = launch('gpu', data, renderer='gpu', wait=4.5)
check('GPU start keeps Skia GPU', running and opened(log) == ['skia-opengl'],
      f'renderers={opened(log)} screenshot={picture}')

# 4. --renderer is for one start; it never changes the setting.
stored = sorted(str(path.relative_to(base)) for path in base.rglob('layout.json')
                if json.loads(path.read_text()).get('gpu_rendering'))
check('--renderer leaves the setting alone', not stored, f'records={stored}')

# 5. A GPU setting with a broken driver falls back once and turns itself off.
data = base / 'setting-broken-driver'
data.mkdir()
(data / 'layout.json').write_text('{"gpu_rendering": true}')
_, running, log, picture = launch('setting-broken-egl', data, environment=BROKEN_EGL, wait=5.5)
saved = json.loads((data / 'layout.json').read_text()).get('gpu_rendering')
check('a failed GPU setting restarts with Skia Software and turns off',
      running and opened(log) == ['skia-opengl', 'skia-software'] and saved is False,
      f'running={running} renderers={opened(log)} saved={saved} screenshot={picture}')
_, running, log, _ = launch('setting-after-failure', data, environment=BROKEN_EGL)
check('the next start goes straight to Skia Software', running and opened(log) == ['skia-software'],
      f'renderers={opened(log)}')

(args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
raise SystemExit(0 if all(result['passed'] for result in results) else 1)
