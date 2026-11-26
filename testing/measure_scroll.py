#!/usr/bin/env python3
"""Measure Yapfast's CPU, memory and frame rate while scrolling a long history.

The fixture posts the sample conversation plus --messages more as Bob. Yapfast opens
the text channel as Alice, settles, and is sampled idle. Then the private seat
sends 60 touchpad scroll events a second, alternating up and down every two seconds, while
the process is sampled again. Every renderer named runs in turn on the same
fixture and display.

Pass the display scale given to scripts/run-framework-isolated with --scale; the
window size is in physical pixels. Frame rates need the compositor's commit log:
set YAPFAST_FRAMEWORK_SURFACE_LOG for run-framework-isolated, then, once it has
exited and flushed the log, run this script again with --frames RESULTS LOG.
They count new buffers Yapfast submitted, not frames presented.

Run inside scripts/run-framework-isolated --no-services, with the local stack
running (scripts/test-stack up).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import signal
import socket
import subprocess
import sys
import time

from voice_harness import ROOT, locate_rail, provision
import voice_harness


def frames(results_path, log_path):
    """Add buffer submissions per second to each scrolling and idle sample."""
    commits = [json.loads(line) for line in Path(log_path).read_text().splitlines() if line.startswith('{')]
    results = json.loads(Path(results_path).read_text())
    for sample in results['samples']:
        start, end = sample['window_ns']
        count = sum(1 for commit in commits if commit['pid'] == sample['yapfast_pid'] and commit['new_buffer']
                    and start <= commit['monotonic_ns'] < end)
        sample['new_buffers_per_second'] = round(count / ((end - start) / 1e9), 1)
        print(json.dumps({key: sample[key] for key in ('renderer', 'profile', 'new_buffers_per_second')}))
    Path(results_path).write_text(json.dumps(results, indent=2) + '\n')


if len(sys.argv) == 4 and sys.argv[1] == '--frames':
    frames(sys.argv[2], sys.argv[3])
    sys.exit()

INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/measure/yapfast')
parser.add_argument('--renderer', action='append', choices=['gpu', 'software'])
parser.add_argument('--fixture-executable', type=Path, required=True, help='yapfast-core test binary')
parser.add_argument('--messages', type=int, default=300, help='history posted before measuring')
parser.add_argument('--size', default='1120x760', help='window size in physical pixels')
parser.add_argument('--scale', type=int, default=1, help='the display scale given to run-framework-isolated')
parser.add_argument('--warmup', type=int, default=15, help='seconds before each sample')
parser.add_argument('--seconds', type=int, default=20, help='sample length')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
renderers = args.renderer or ['gpu', 'software']
width, height = (int(value) for value in args.size.split('x'))
args.output.mkdir(parents=True, exist_ok=True)


def sample(pid, renderer, profile, during=None):
    """Sample the process; run during() alongside, until the sample ends."""
    start = time.monotonic_ns()
    process = subprocess.Popen(['python3', str(ROOT / 'scripts/measure-process'), str(pid),
                                '--seconds', str(args.seconds)], stdout=subprocess.PIPE, text=True)
    if during:
        during(lambda: process.poll() is None)
    result = json.loads(process.communicate()[0])
    result.update(renderer=renderer, profile=profile, yapfast_pid=pid, window_ns=[start, time.monotonic_ns()],
                  size=[width, height], scale=args.scale)
    print(json.dumps({key: result[key] for key in ('renderer', 'profile', 'pss_mib_mean', 'rss_mib_mean',
                                                    'cpu_percent_one_core')}), flush=True)
    return result


results = []
fixture = provision()
print(f'Fixture {fixture}', flush=True)
seeded = subprocess.run([str(args.fixture_executable.resolve()), '--exact', 'matrix_chat::tests::fixture_conversation',
                         '--ignored', '--nocapture'], cwd=ROOT, capture_output=True, text=True, timeout=600,
                        env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture), YAPFAST_TEST_EXTRA_MESSAGES=str(args.messages)))
if seeded.returncode != 0:
    raise SystemExit('Posting the history failed:\n' + seeded.stdout[-3000:] + seeded.stderr[-3000:])
try:
    for renderer in renderers:
        log = (args.output / f'{renderer}.log').open('wb')
        yapfast = subprocess.Popen([str(args.exe.resolve()), '--renderer', renderer, '--data-dir', str(fixture / 'alice')],
                                env=dict(os.environ, YAPFAST_UI_TEST_SIZE=f'{width}x{height}'),
                                stdout=log, stderr=log, start_new_session=True)
        try:
            time.sleep(12)
            if yapfast.poll() is not None:
                raise RuntimeError(f'Yapfast exited during startup; see {renderer}.log')
            # Screenshots are in physical pixels; pointer positions are in logical ones.
            origin_x, origin_y = locate_rail(voice_harness.capture(args.output, f'{renderer}-startup'),
                                             minimum_height=height * 3 // 4)
            origin_x, origin_y = origin_x / args.scale, origin_y / args.scale

            def click(x, y):
                INPUT['click'](origin_x + x, origin_y + y)
                time.sleep(.3)

            click(36, 101)  # Server
            click(141, 148)  # general
            time.sleep(args.warmup)
            voice_harness.capture(args.output, f'{renderer}-idle')
            results.append(sample(yapfast.pid, renderer, 'settled idle, long history open'))

            def scroll(running):
                INPUT['send'](f'move {origin_x + width / args.scale * .6} {origin_y + height / args.scale * .5}')
                # framework-input waits 30 ms after each event, so send directly at 60 Hz.
                seat = str(INPUT['require_private_display']() / 'yapfast-input.sock')
                started = time.monotonic()
                with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
                    while running():
                        elapsed = time.monotonic() - started
                        direction = -1 if int(elapsed / 2) % 2 == 0 else 1
                        client.sendto(f'scroll {25 * direction}'.encode(), seat)
                        time.sleep(max(0, (int(elapsed * 60) + 1) / 60 - elapsed))

            results.append(sample(yapfast.pid, renderer, 'scrolling, 60 touchpad events/s', scroll))
            for _ in range(20):  # Leave the view scrolled up, so the capture shows that scrolling worked.
                INPUT['scroll'](-25)
            voice_harness.capture(args.output, f'{renderer}-scrolled')
            click(width / args.scale - 19, -20)  # Close on the private window's title bar.
            yapfast.wait(timeout=20)
        finally:
            if yapfast.poll() is None:
                os.killpg(yapfast.pid, signal.SIGTERM)
                yapfast.wait(timeout=20)
            log.close()
finally:
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binary': str(args.exe), 'binary_sha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'Private headless Weston (GL renderer, same GPU), private D-Bus without services',
        'history': f'sample conversation plus {args.messages} text messages',
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
