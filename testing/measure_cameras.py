#!/usr/bin/env python3
"""Measure the desktop client in a Matrix call as cameras turn on.

Alice runs the given Yapfast build against the local stack, in a call with Bob
and three load accounts, which film the test pattern in turn: none, then 1, 2
and 4 cameras. Then Alice turns her own camera on, and everyone's off again.
Only Alice's process is sampled. The load process is the one in
testing/measure_connected.py.

Run inside scripts/run-framework-isolated --audio --no-services, with the local
stack running (scripts/test-stack up).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import signal
import subprocess
import tempfile
import time

from voice_harness import provision

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/measure/yapfast')
parser.add_argument('--renderer', action='append', choices=['gpu', 'software'])
parser.add_argument('--warmup', type=int, default=15, help='seconds before each sample')
parser.add_argument('--seconds', type=int, default=20, help='sample length')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--load-executable', type=Path, required=True, help='yapfast-core test binary for the load process')
args = parser.parse_args()
renderers = args.renderer or ['software', 'gpu']
args.output.mkdir(parents=True, exist_ok=True)
WIDTH, HEIGHT = 1120, 760
EXTRA = 3
CAMERA = dict(os.environ, YAPFAST_CAMERA_TEST_PATTERN='1')


def capture(name):
    time.sleep(.3)
    with tempfile.TemporaryDirectory(prefix='yapfast-measure-shot-') as folder:
        subprocess.run(['weston-screenshooter'], env=dict(os.environ, XDG_PICTURES_DIR=folder),
                       check=True, timeout=10, capture_output=True)
        picture = next(Path(folder).glob('*.png'))
        path = args.output / f'{name}.png'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', str(picture),
                        '-vf', 'vflip', str(path)], check=True, timeout=10)
        return path


def locate_rail(picture):
    # The rail's exact graphite (Theme.rail, #111215) forms the window's long left edge.
    pixels = subprocess.check_output(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(picture),
                                      '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=10)
    output_width, output_height = 1600, 1000
    target = bytes([17, 18, 21])
    columns = [sum(pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target
                   for y in range(output_height)) for x in range(output_width)]
    best = max(columns)
    if best < HEIGHT - 80:
        raise RuntimeError(f'Yapfast rail was not found; inspect {picture}')
    x = next(x for x, count in enumerate(columns) if count >= best - 2)
    ys = [y for y in range(output_height)
          if pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target]
    return x, min(ys)


def sample(pid, name, notes):
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(pid),
                                                 '--seconds', str(args.seconds)], text=True))
    names = {}
    for task in os.listdir(f'/proc/{pid}/task'):
        thread = Path(f'/proc/{pid}/task/{task}/comm').read_text().strip()
        names[thread] = names.get(thread, 0) + 1
    # Anonymous memory is what Yapfast itself holds. On a host short of memory
    # the kernel drops shared-library pages from RSS and PSS mid-run, and
    # swaps some of the rest out.
    # Swapped pages count too: they're Yapfast's, only elsewhere.
    def field(path, key):
        return next(int(line.split()[1]) for line in Path(path).read_text().splitlines()
                    if line.startswith(key)) / 1024
    anonymous = field(f'/proc/{pid}/smaps_rollup', 'Anonymous:') + field(f'/proc/{pid}/status', 'VmSwap:')
    result.update(profile=name, threads=sum(names.values()), thread_names=names,
                  anonymous_mib=round(anonymous, 1), **notes)
    print(json.dumps({key: result[key] for key in ('profile', 'renderer', 'anonymous_mib', 'pss_mib_mean',
                                                    'rss_mib_mean', 'cpu_percent_one_core', 'threads')}), flush=True)
    return result


class Load:
    def __init__(self, fixture):
        self.fixture = fixture
        self.commands = fixture / 'load.commands'
        self.commands.write_text('')
        self.log = (fixture / 'load.log').open('ab')
        self.process = subprocess.Popen([str(args.load_executable.resolve()), '--exact',
                                         'matrix_voice::tests::load_participants', '--ignored', '--nocapture'],
                                        cwd=ROOT, stdout=self.log, stderr=self.log,
                                        env=dict(CAMERA, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                                                 YAPFAST_TEST_EXTRA_ACCOUNTS=str(EXTRA)),
                                        start_new_session=True)

    def send(self, command, expect, timeout=240):
        if command:
            with self.commands.open('a') as commands:
                commands.write(command + '\n')
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if (self.fixture / 'load.status').exists() and (self.fixture / 'load.status').read_text() == expect:
                return
            if self.process.poll() is not None:
                raise RuntimeError('load process exited; see load.log')
            time.sleep(.2)
        raise RuntimeError(f'load did not reach {expect!r}')

    def stop(self):
        if self.process.poll() is None:
            with self.commands.open('a') as commands:
                commands.write('exit\n')
            try:
                self.process.wait(timeout=60)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
        self.log.close()


results = []
load = None
fixture = provision()
print(f'Fixture {fixture}', flush=True)
try:
    load = Load(fixture)
    load.send('', 'ready', 600)  # Waits for the load accounts to sync.
    for renderer in renderers:
        environment = dict(CAMERA, YAPFAST_UI_TEST_SIZE=f'{WIDTH}x{HEIGHT}')
        log = (args.output / f'{renderer}.log').open('wb')
        process = subprocess.Popen([str(args.exe.resolve()), '--renderer', renderer, '--data-dir', str(fixture / 'alice')],
                                   env=environment, stdout=log, stderr=log, start_new_session=True)
        try:
            time.sleep(12)
            if process.poll() is not None:
                raise RuntimeError(f'Yapfast exited during startup; see {renderer}.log')
            origin_x, origin_y = locate_rail(capture(f'{renderer}-startup'))

            def click(x, y):
                INPUT['click'](origin_x + x, origin_y + y)
                time.sleep(.3)

            notes = {'renderer': renderer, 'window': [WIDTH, HEIGHT], 'warmup_seconds': args.warmup}
            click(36, 101)  # Server
            click(141, 222)  # Lounge
            time.sleep(1)
            click(715, 495)  # Join voice
            load.send(f'join {EXTRA + 1}', f'joined {EXTRA + 1}')
            time.sleep(args.warmup)
            capture(f'{renderer}-cameras-0')
            results.append(sample(process.pid, f'{EXTRA + 2}-person call, no cameras', notes))
            for count in (1, 2, 4):
                load.send(f'camera {count}', f'filming {count}')
                time.sleep(args.warmup)
                capture(f'{renderer}-cameras-{count}')
                results.append(sample(process.pid, f'{EXTRA + 2}-person call, {count} remote camera(s)', notes))
            click(736, 718)  # Alice's camera
            time.sleep(args.warmup)
            capture(f'{renderer}-cameras-4-and-own')
            results.append(sample(process.pid, f'{EXTRA + 2}-person call, 4 remote cameras and Alice\'s own', notes))
            click(736, 718)
            load.send('camera 0', 'filming 0')
            time.sleep(args.warmup)
            capture(f'{renderer}-cameras-off')
            results.append(sample(process.pid, f'{EXTRA + 2}-person call, every camera off again', notes))
            # Again, to show that turning cameras on and off doesn't add up.
            load.send('camera 4', 'filming 4')
            click(736, 718)
            time.sleep(args.warmup)
            click(736, 718)
            load.send('camera 0', 'filming 0')
            time.sleep(args.warmup)
            capture(f'{renderer}-cameras-off-again')
            results.append(sample(process.pid, f'{EXTRA + 2}-person call, every camera on and off a second time', notes))
            load.send('leave', 'ready')
            click(WIDTH - 19, -20)  # Close on the private window's title bar.
            process.wait(timeout=20)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=20)
            log.close()
finally:
    if load:
        load.stop()
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binary': str(args.exe), 'binary_sha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'Private headless Weston, private PulseAudio null sink, private D-Bus without services',
        'remote_load': 'Bob and three load accounts in one process; cameras film a 640x360 test pattern',
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
