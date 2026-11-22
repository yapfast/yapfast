#!/usr/bin/env python3
"""Stress a voice channel with many people, with and without Opus DTX.

Alice runs the given Yapfast build as a muted listener in the voice view. One
publish-only load process joins Bob and extra fixture accounts in steps, with
generated tones. In the conversation pattern one person speaks at a time and
everyone else sends digital silence, as behind a voice-activation gate; in the
all pattern everyone sends a tone continuously. Silence suppression only
matters for the conversation pattern, so --dtx off measures that pattern only.
Run DTX on and off as separate runs: leaving a large call takes minutes, and
lingering memberships would distort a second pass.

Each sample records Yapfast's memory and CPU, the LiveKit and Synapse containers'
CPU, LiveKit's network traffic, the load process's CPU and the host's CPU.

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
import time

from voice_harness import (HEIGHT, ROOT, WIDTH, Container, host_ticks, locate_rail, process_cpu_seconds,
                           provision)
import voice_harness

INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/measure/yapfast')
parser.add_argument('--renderer', choices=['gpu', 'software'], default='gpu')
parser.add_argument('--sizes', default='10,25,50', help='people in the call, including Alice')
parser.add_argument('--warmup', type=int, default=20, help='seconds after each join step')
parser.add_argument('--settle', type=int, default=10, help='seconds after switching the pattern')
parser.add_argument('--seconds', type=int, default=45, help='sample length')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--load-executable', type=Path, required=True, help='prebuilt yapfast-core test binary')
parser.add_argument('--dtx', choices=['on', 'off'], required=True,
                    help='Opus silence suppression for the load accounts; run each in a fresh process, since a mass leave lingers')
args = parser.parse_args()
sizes = sorted(int(size) for size in args.sizes.split(','))
args.output.mkdir(parents=True, exist_ok=True)


def capture(name):
    return voice_harness.capture(args.output, name)


class Load:
    def __init__(self, executable, fixture, extra):
        self.fixture = fixture
        self.commands = fixture / 'load.commands'
        self.commands.write_text('')
        self.log = (fixture / 'load.log').open('ab')
        self.process = subprocess.Popen([executable, '--exact', 'matrix_voice::tests::load_participants', '--ignored',
                                         '--nocapture'], cwd=ROOT, stdout=self.log, stderr=self.log,
                                        env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                                                 YAPFAST_TEST_EXTRA_ACCOUNTS=str(extra),
                                                 YAPFAST_TEST_PUBLISH_ONLY='1'),
                                        start_new_session=True)

    def send(self, command, expect=None, timeout=600):
        with self.commands.open('a') as commands:
            commands.write(command + '\n')
        if expect:
            started = time.monotonic()
            self.wait(expect, timeout)
            return time.monotonic() - started
        return None

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
                self.process.wait(timeout=120)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
        self.log.close()


livekit, synapse = Container('yapfast-test-livekit-1'), Container('yapfast-test-synapse-1')


def sample(yapfast, load, notes):
    before = {'livekit': livekit.read(), 'synapse': synapse.read(), 'load': process_cpu_seconds(load.process.pid),
              'host': host_ticks(), 'time': time.monotonic()}
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(yapfast.pid),
                                                 '--seconds', str(args.seconds)], text=True))
    after = {'livekit': livekit.read(), 'synapse': synapse.read(), 'load': process_cpu_seconds(load.process.pid),
             'host': host_ticks(), 'time': time.monotonic()}
    elapsed = after['time'] - before['time']

    def delta(service, key):
        return after[service][key] - before[service][key]

    host_total, host_idle = (after['host'][index] - before['host'][index] for index in (0, 1))
    result.update(notes)
    result.update({
        'livekit_cpu_percent_one_core': round(delta('livekit', 'cpu_seconds') / elapsed * 100, 2),
        'livekit_rx_mbit_s': round(delta('livekit', 'rx_bytes') * 8 / elapsed / 1e6, 3),
        'livekit_tx_mbit_s': round(delta('livekit', 'tx_bytes') * 8 / elapsed / 1e6, 3),
        'livekit_rx_packets_s': round(delta('livekit', 'rx_packets') / elapsed),
        'livekit_tx_packets_s': round(delta('livekit', 'tx_packets') / elapsed),
        'synapse_cpu_percent_one_core': round(delta('synapse', 'cpu_seconds') / elapsed * 100, 2),
        'load_cpu_percent_one_core': round((after['load'] - before['load']) / elapsed * 100, 2),
        'host_cpu_busy_percent_all_cores': round(100 * (1 - host_idle / host_total), 1),
    })
    print(json.dumps({key: result[key] for key in (
        'people', 'pattern', 'dtx', 'pss_mib_mean', 'cpu_percent_one_core', 'livekit_cpu_percent_one_core',
        'livekit_rx_mbit_s', 'livekit_tx_mbit_s', 'livekit_rx_packets_s', 'synapse_cpu_percent_one_core',
        'load_cpu_percent_one_core', 'host_cpu_busy_percent_all_cores')}), flush=True)
    return result


results, joins = [], []
load = None
fixture = provision()
print(f'Fixture {fixture}', flush=True)
try:
    load = Load(str(args.load_executable.resolve()), fixture, sizes[-1] - 2)
    load.wait('ready', 1200)
    log = (args.output / 'yapfast.log').open('wb')
    yapfast = subprocess.Popen([str(args.exe.resolve()), '--renderer', args.renderer, '--data-dir', str(fixture / 'alice')],
                            env=dict(os.environ, YAPFAST_UI_TEST_SIZE=f'{WIDTH}x{HEIGHT}'),
                            stdout=log, stderr=log, start_new_session=True)
    try:
        time.sleep(12)
        if yapfast.poll() is not None:
            raise RuntimeError('Yapfast exited during startup; see yapfast.log')
        origin_x, origin_y = locate_rail(capture('startup'))

        def click(x, y):
            INPUT['click'](origin_x + x, origin_y + y)
            time.sleep(.3)

        click(36, 101)  # Server
        click(141, 222)  # Lounge
        time.sleep(1)
        click(715, 495)  # Join voice, muted
        time.sleep(args.warmup)
        capture('alone')
        dtx = args.dtx
        load.send(f'dtx {dtx}')
        for people in sizes:
            seconds = load.send(f'join {people - 1}', f'joined {people - 1}')
            joins.append({'dtx': dtx, 'people': people, 'join_step_seconds': round(seconds, 1)})
            print(json.dumps(joins[-1]), flush=True)
            time.sleep(args.warmup)
            for pattern in ('conversation', 'all') if dtx == 'on' else ('conversation',):
                load.send(f'pattern {pattern}')
                time.sleep(args.settle)
                capture(f'dtx-{dtx}-{people}-{pattern}')
                notes = {'renderer': args.renderer, 'window': [WIDTH, HEIGHT], 'people': people,
                         'pattern': pattern, 'dtx': dtx, 'alice': 'muted listener, voice view open'}
                results.append(sample(yapfast, load, notes))
        seconds = load.send('leave', 'ready')
        joins.append({'dtx': dtx, 'leave_all_seconds': round(seconds, 1)})
        print(json.dumps(joins[-1]), flush=True)
        time.sleep(args.warmup)
        click(WIDTH - 19, -20)  # Close on the private window's title bar.
        yapfast.wait(timeout=20)
    finally:
        if yapfast.poll() is None:
            os.killpg(yapfast.pid, signal.SIGTERM)
            yapfast.wait(timeout=20)
        log.close()
finally:
    if load:
        load.stop()
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binary': str(args.exe), 'binary_sha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'Private headless Weston, private PulseAudio null sink; LiveKit, Synapse and the load process share the host',
        'remote_load': 'One publish-only load process: Bob plus fixture accounts with generated tones',
        'joins': joins,
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
