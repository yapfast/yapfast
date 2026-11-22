#!/usr/bin/env python3
"""Measure what Yapfast costs in a call with its window open, hidden and destroyed.

Yapfast has no tray yet. This measures the ways one could work: hiding the window,
which keeps the whole interface in memory, or destroying the window and keeping
only the models, the session and the call. Yapfast's test-only window control
(YAPFAST_UI_TEST_CONTROL) hides, destroys and recreates the window while the call
goes on, and runs malloc_trim to show how much freed memory glibc keeps. Each
sample also records glibc's malloc_stats totals and saves Yapfast's smaps.

Alice runs the given Yapfast build with the sample conversation loaded, joins the
voice channel and unmutes. Every other person is a separate headless participant
process (matrix_voice::tests::stress_participant) taking turns to speak; those processes
show the cost of the same session and call with no interface at all.

--heap-snapshot runs Yapfast under heaptrack instead of sampling it. Once the given
state has settled, it kills Yapfast, so every allocation still live then is reported
as leaked: heaptrack_print --print-leaks shows the heap at that moment.

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
import statistics
import subprocess
import time

from voice_harness import (HEIGHT, ROOT, WIDTH, Participant, locate_rail, process_cpu_seconds, process_memory_breakdown,
                           process_memory_mib, provision, wait_for)
import voice_harness

INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/measure/yapfast')
parser.add_argument('--renderer', choices=['gpu', 'software'], default='gpu')
parser.add_argument('--participant-executable', type=Path, required=True,
                    help='yapfast-core test binary, preferably built with --release')
parser.add_argument('--people', type=int, default=10, help='people in the call, including Alice')
parser.add_argument('--warmup', type=int, default=30, help='seconds after each change before sampling')
parser.add_argument('--seconds', type=int, default=60, help='sample length')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--yapfast-env', action='append', default=[], metavar='NAME=VALUE',
                    help='extra environment for Yapfast only, such as an allocator setting')
parser.add_argument('--heap-snapshot', choices=['open', 'destroyed'],
                    help='profile with heaptrack and stop with a heap snapshot of this state')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)


def capture(name):
    return voice_harness.capture(args.output, name)


def yapfast_process(launcher):
    """Yapfast's own PID, which differs from the launcher's under heaptrack."""
    executable = os.path.realpath(args.exe)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        for entry in Path('/proc').iterdir():
            try:
                if (entry.name.isdigit() and os.getsid(int(entry.name)) == launcher.pid
                        and os.path.realpath(entry / 'exe') == executable):
                    return int(entry.name)
            except OSError:
                continue
        time.sleep(.2)
    raise RuntimeError('Yapfast did not start')


def allocator_totals():
    """The latest malloc_stats totals from yapfast.log: arenas, MiB held from the system and in use."""
    text = (args.output / 'yapfast.log').read_text(errors='replace')
    if 'Total (incl. mmap):' not in text:
        return None
    block = text[text.rindex('Arena 0:', 0, text.rindex('Total (incl. mmap):')):]
    total = block[block.index('Total (incl. mmap):'):].splitlines()
    value = lambda name: int(next(line for line in total if line.strip().startswith(name)).split('=')[1])
    return {'arenas': block.count('Arena '), 'system_mib': round(value('system bytes') / 2**20, 1),
            'in_use_mib': round(value('in use bytes') / 2**20, 1)}


def sample(yapfast, people, state):
    """Sample Yapfast with scripts/measure-process and the headless participants over the same span."""
    before = {person.account.name: process_cpu_seconds(person.process.pid) for person in people}
    started = time.monotonic()
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(yapfast),
                                                 '--seconds', str(args.seconds)], text=True))
    elapsed = time.monotonic() - started
    (args.output / f'smaps-{len(results)}.txt').write_bytes(Path(f'/proc/{yapfast}/smaps').read_bytes())
    window('stats')
    cpu = [(process_cpu_seconds(person.process.pid) - before[person.account.name]) / elapsed * 100
           for person in people]
    memory = [process_memory_mib(person.process.pid) for person in people]
    anonymous = [process_memory_breakdown(person.process.pid)['anonymous_mib'] for person in people]
    result.update(state=state, yapfast_memory=process_memory_breakdown(yapfast), allocator=allocator_totals(),
                  headless_participant={
        'anonymous_mib_mean': round(statistics.mean(anonymous), 1),
        'cpu_percent_one_core_mean': round(statistics.mean(cpu), 2),
        'pss_mib_mean': round(statistics.mean(value['pss'] for value in memory), 1),
        'rss_mib_mean': round(statistics.mean(value['rss'] for value in memory), 1),
    })
    print(json.dumps({key: result[key] for key in ('state', 'pss_mib_mean', 'rss_mib_mean', 'cpu_percent_one_core',
                                                    'yapfast_memory', 'allocator', 'headless_participant')}), flush=True)
    return result


results = []
people = []
fixture = provision()
print(f'Fixture {fixture}', flush=True)
executable = str(args.participant_executable.resolve())
try:
    subprocess.run([executable, '--exact', 'matrix_chat::tests::fixture_conversation', '--ignored'], check=True, cwd=ROOT,
                   env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture)), capture_output=True)
    # The load helper registers Bob's companions and joins them to the rooms, then exits.
    count = args.people - 1
    (fixture / 'load.commands').write_text('exit\n')
    with (fixture / 'load.log').open('ab') as log:
        subprocess.run([executable, '--exact', 'matrix_voice::tests::load_participants', '--ignored', '--nocapture'],
                       cwd=ROOT, stdout=log, stderr=log, check=True, timeout=1200,
                       env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                                YAPFAST_TEST_EXTRA_ACCOUNTS=str(count - 1)))
    accounts = [fixture / 'bob'] + [fixture / f'load-{index}' for index in range(count - 1)]
    control = args.output / 'window.commands'
    control.write_text('')
    log = (args.output / 'yapfast.log').open('wb')
    command = [str(args.exe.resolve()), '--renderer', args.renderer, '--data-dir', str(fixture / 'alice')]
    if args.heap_snapshot:
        command = ['heaptrack', '--record-only', '-o', str(args.output / 'heaptrack')] + command
    yapfast = subprocess.Popen(command, env=dict(os.environ, YAPFAST_UI_TEST_SIZE=f'{WIDTH}x{HEIGHT}',
                                              YAPFAST_UI_TEST_CONTROL=str(control),
                                              **dict(setting.split('=', 1) for setting in args.yapfast_env)),
                            stdout=log, stderr=log, start_new_session=True)

    def window(command):
        with control.open('a') as commands:
            commands.write(command + '\n')
        time.sleep(1)
        if command != 'quit' and yapfast.poll() is not None:
            raise RuntimeError(f'Yapfast exited after {command!r}; see yapfast.log')

    def measure(state, name):
        """Sample a settled state, or end the run with a heap snapshot of it."""
        if args.heap_snapshot != state:
            results.append(sample(yapfast_pid, people, name))
            return False
        (args.output / f'smaps-{state}.txt').write_bytes(Path(f'/proc/{yapfast_pid}/smaps').read_bytes())
        os.kill(yapfast_pid, signal.SIGKILL)
        yapfast.wait(timeout=300)
        print(json.dumps({'heap_snapshot': name, 'heaptrack': sorted(path.name for path in
                                                                    args.output.glob('heaptrack*'))}), flush=True)
        return True

    def states():
        click(222, HEIGHT - 30)  # Unmute in the account panel: Alice also publishes the private null-sink monitor.
        time.sleep(args.warmup)
        capture('open')
        if measure('open', 'window open, voice view'):
            return

        window('hide')
        time.sleep(args.warmup)
        capture('hidden')
        results.append(sample(yapfast_pid, people, 'window hidden'))

        window('show')
        window('destroy')
        time.sleep(args.warmup)
        capture('destroyed')
        if measure('destroyed', 'window destroyed'):
            return

        window('trim')
        time.sleep(5)
        results.append(sample(yapfast_pid, people, 'window destroyed, after malloc_trim'))

        window('create')
        time.sleep(args.warmup)
        capture('recreated')
        results.append(sample(yapfast_pid, people, 'window recreated, voice view'))

        yapfast_log = (args.output / 'yapfast.log').read_text(errors='replace')
        print(json.dumps({'yapfast_log_errors': sum('ERROR' in line for line in yapfast_log.splitlines())}), flush=True)
        window('quit')
        yapfast.wait(timeout=20)

    try:
        yapfast_pid = yapfast_process(yapfast)
        # Under heaptrack Yapfast takes much longer to show its window.
        time.sleep(12)
        deadline = time.monotonic() + (300 if args.heap_snapshot else 0)
        while True:
            if yapfast.poll() is not None:
                raise RuntimeError('Yapfast exited during startup; see yapfast.log')
            try:
                origin_x, origin_y = locate_rail(capture('startup'))
                break
            except RuntimeError:
                if time.monotonic() > deadline:
                    raise
                time.sleep(5)
        if args.heap_snapshot:
            time.sleep(60)  # Sign-in and the first sync finish long after the window appears.

        def click(x, y):
            INPUT['click'](origin_x + x, origin_y + y)
            time.sleep(.3)

        click(36, 101)  # Server
        click(141, 148)  # general, so the sample conversation is loaded
        time.sleep(5)
        click(141, 222)  # Lounge
        time.sleep(1)
        click(715, 495)  # Join voice
        people = [Participant(executable, fixture, account, index, count) for index, account in enumerate(accounts)]
        if wait_for(lambda: all(person.status().get('phase') == 'ready' for person in people), 300) is None:
            raise RuntimeError('participants did not finish syncing')
        for person in people:
            person.send('join')
            if wait_for(lambda: person.status().get('phase') == 'joined', 120) is None:
                raise RuntimeError(f'{person.account.name} did not join')
            time.sleep(2)
        states()
    finally:
        if yapfast.poll() is None:
            os.killpg(yapfast.pid, signal.SIGTERM)
            yapfast.wait(timeout=20)
        log.close()
finally:
    for person in people:
        person.stop()
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binary': str(args.exe), 'binary_sha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        'participant_binary': str(args.participant_executable),
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'Private headless Weston and PulseAudio; Yapfast, every participant process, LiveKit and Synapse share the host',
        'renderer': args.renderer, 'window': [WIDTH, HEIGHT], 'people': args.people, 'yapfast_env': args.yapfast_env,
        'history': 'sample conversation: Markdown, 800x500 image, file, reply, reaction',
        'alice': 'unmuted, publishing the private null-sink monitor',
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
