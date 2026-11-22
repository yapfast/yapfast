#!/usr/bin/env python3
"""Measure the connected desktop client on the private compositor.

Alice runs the given Yapfast build against the local stack. A separate load
process hosts Bob and extra fixture accounts: it sends chat messages and joins
voice with generated tones, which need no audio device. Alice's audio uses the
private PulseAudio null sink. Only Alice's process is sampled.

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

from voice_harness import provision, test_binary

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/measure/yapfast')
parser.add_argument('--renderer', action='append', choices=['gpu', 'software'])
parser.add_argument('--warmup', type=int, default=30, help='seconds before each sample')
parser.add_argument('--seconds', type=int, default=60, help='sample length')
parser.add_argument('--extra', type=int, default=8, help='load accounts besides Bob (8 makes ten people)')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--load-executable', type=Path, help='prebuilt yapfast-core test binary for the load process')
parser.add_argument('--seed', action='store_true',
                    help='post the sample conversation (Markdown, image, file, reply, reaction) before measuring')
args = parser.parse_args()
renderers = args.renderer or ['gpu', 'software']
args.output.mkdir(parents=True, exist_ok=True)
WIDTH, HEIGHT = 1120, 760


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


def gpu_memory(pid):
    """Framebuffer MiB from NVIDIA's per-process monitor, when available."""
    try:
        output = subprocess.run(['nvidia-smi', 'pmon', '-c', '1', '-s', 'um'], capture_output=True,
                                text=True, timeout=15).stdout
    except (OSError, subprocess.TimeoutExpired):
        return None
    for line in output.splitlines():
        fields = line.split()
        if len(fields) > 10 and fields[1] == str(pid):
            return {'sm_percent': fields[3], 'fb_mib': fields[9]}
    return None


def sample(pid, name, notes):
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(pid),
                                                 '--seconds', str(args.seconds)], text=True))
    result.update(profile=name, nvidia_pmon=gpu_memory(pid), **notes)
    print(json.dumps({key: result[key] for key in ('profile', 'renderer', 'pss_mib_mean', 'rss_mib_mean',
                                                    'cpu_percent_one_core')}), flush=True)
    return result


class Load:
    def __init__(self, executable, fixture):
        self.fixture = fixture
        self.commands = fixture / 'load.commands'
        self.commands.write_text('')
        self.log = (fixture / 'load.log').open('ab')
        self.process = subprocess.Popen([executable, '--exact', 'matrix_voice::tests::load_participants', '--ignored',
                                         '--nocapture'], cwd=ROOT, stdout=self.log, stderr=self.log,
                                        env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                                                 YAPFAST_TEST_EXTRA_ACCOUNTS=str(args.extra)),
                                        start_new_session=True)

    def send(self, command, expect=None, timeout=240):
        with self.commands.open('a') as commands:
            commands.write(command + '\n')
        if expect:
            self.wait(expect, timeout)

    def wait(self, status, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if (self.fixture / 'load.status').exists() and (self.fixture / 'load.status').read_text() == status:
                return
            if self.process.poll() is not None:
                raise RuntimeError('load process exited; see load.log')
            time.sleep(.2)
        raise RuntimeError(f'load did not reach {status!r}')

    def stop(self):
        if self.process.poll() is None:
            self.send('exit')
            try:
                self.process.wait(timeout=60)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
        self.log.close()


results = []
load = None
executable = str(args.load_executable.resolve()) if args.load_executable else test_binary()
fixture = provision()
print(f'Fixture {fixture}', flush=True)
if args.seed:
    subprocess.run([executable, '--exact', 'matrix_chat::tests::fixture_conversation', '--ignored'], check=True, cwd=ROOT,
                   env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture)), capture_output=True)
try:
    load = Load(executable, fixture)
    load.wait('ready', 600)
    for renderer in renderers:
        environment = dict(os.environ, YAPFAST_UI_TEST_SIZE=f'{WIDTH}x{HEIGHT}')
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
            click(141, 148)  # general
            time.sleep(args.warmup)
            capture(f'{renderer}-idle')
            results.append(sample(process.pid, 'settled idle, encrypted text channel open', notes))

            load.send('chat-start')
            time.sleep(10)
            results.append(sample(process.pid, 'receiving a message every 2 s in the open channel', notes))
            load.send('chat-stop')
            capture(f'{renderer}-chat')

            click(141, 222)  # Lounge
            time.sleep(1)
            click(715, 495)  # Join voice
            time.sleep(args.warmup)
            capture(f'{renderer}-voice-alone')
            results.append(sample(process.pid, 'alone in voice, muted, voice view open', notes))

            load.send('join 1', 'joined 1')
            time.sleep(args.warmup)
            capture(f'{renderer}-voice-two')
            results.append(sample(process.pid, 'two-person call: Bob publishes a tone, Alice muted', notes))

            load.send(f'join {args.extra + 1}', f'joined {args.extra + 1}')
            click(222, HEIGHT - 30)  # Unmute in the account panel: Alice also publishes the private null-sink monitor.
            time.sleep(args.warmup)
            capture(f'{renderer}-voice-ten')
            results.append(sample(process.pid, f'{args.extra + 2}-person call: every remote participant publishes a tone, Alice unmuted', notes))
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
        'isolation': 'Private headless Weston (GL renderer, same GPU), private PulseAudio null sink, private D-Bus without services',
        'remote_load': 'Generated tones from one load process; Bob plus extra fixture accounts on the local stack',
        'history': 'sample conversation: Markdown, 800x500 image, file, reply, reaction' if args.seed else 'empty channel',
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
