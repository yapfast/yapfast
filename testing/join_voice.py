#!/usr/bin/env python3
"""Measure joining a voice channel as it fills, with one process per person.

Alice runs the given Yapfast build and joins muted first. Every other person is a
separate headless participant process (matrix_voice::tests::stress_participant) using
the same voice stack: it publishes a tone in turn with the others, with digital
silence and Opus DTX otherwise, and decodes every stream it receives.

People join one at a time, with a gap longer than the media key's 10 s freshness
window, so each arrival meets a settled call. Each join records how long the join
call took, when the newcomer held every other participant's key, when every other
participant held the newcomer's, and the CPU time the whole call spent on it.
With the call full, it samples each client's memory and CPU, then repeats a
leave and rejoin at full size.

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

from voice_harness import (HEIGHT, ROOT, WIDTH, Container, Participant, locate_rail, process_cpu_seconds,
                           process_memory_mib, provision, wait_for)
import voice_harness

INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exe', type=Path, default=ROOT / 'target/measure/yapfast')
parser.add_argument('--participant-executable', type=Path, required=True,
                    help='yapfast-core test binary, preferably built with --release')
parser.add_argument('--people', type=int, default=20, help='people in the full call, including Alice')
parser.add_argument('--gap', type=float, default=12, help='seconds between joins')
parser.add_argument('--rejoins', type=int, default=3, help='leave and rejoin cycles at full size')
parser.add_argument('--seconds', type=int, default=45, help='steady-state sample length')
parser.add_argument('--timeout', type=float, default=120, help='longest wait for one join to settle')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
livekit, synapse = Container('yapfast-test-livekit-1'), Container('yapfast-test-synapse-1')


def capture(name):
    return voice_harness.capture(args.output, name)


def cpu_snapshot(yapfast, people):
    return {'yapfast': process_cpu_seconds(yapfast.pid), 'synapse': synapse.read()['cpu_seconds'],
            'livekit': livekit.read()['cpu_seconds'],
            'people': {person.account.name: process_cpu_seconds(person.process.pid) for person in people}}


def join(newcomer, earlier, yapfast, everyone):
    """Join one participant and time its key exchange with everyone already in."""
    earlier_identities = {person.status()['identity'] for person in earlier}
    earlier_members = {person.status()['member_id'] for person in earlier}
    before = cpu_snapshot(yapfast, everyone)
    started = time.monotonic()
    newcomer.send('join')
    joined = wait_for(lambda: newcomer.status().get('phase') == 'joined', args.timeout)
    if joined is None:
        raise RuntimeError(f'{newcomer.account.name} did not join')
    status = newcomer.status()
    identity, member = status['identity'], status['member_id']
    # Alice's muted Yapfast client is not observable here, so she is left out.
    conditions = {
        'newcomer_has_all_keys_s': lambda: earlier_identities <= set(newcomer.status().get('keyed', [])),
        'all_have_newcomer_key_s': lambda: all(identity in person.status().get('keyed', []) for person in earlier),
        'newcomer_hears_all_s': lambda: earlier_members <= set(newcomer.status().get('decrypting', [])),
        'all_hear_newcomer_s': lambda: all(member in person.status().get('decrypting', []) for person in earlier),
    }
    reached = {}
    while len(reached) < len(conditions) and time.monotonic() - started < args.timeout:
        for name, condition in conditions.items():
            if name not in reached and condition():
                reached[name] = round(time.monotonic() - started, 2)
        time.sleep(.1)
    settled = time.monotonic() - started
    after = cpu_snapshot(yapfast, everyone)
    others = [person.account.name for person in earlier]
    result = {
        'participant': newcomer.account.name, 'people_after_join': len(earlier) + 2,
        'join_call_s': round(newcomer.status()['join_ms'] / 1000, 2),
        **{name: reached.get(name) for name in conditions},
        'window_s': round(settled, 2),
        'cpu_s_newcomer': round(after['people'][newcomer.account.name] - before['people'][newcomer.account.name], 2),
        'cpu_s_others_total': round(sum(after['people'][name] - before['people'][name] for name in others), 2),
        'cpu_s_talk': round(after['yapfast'] - before['yapfast'], 2),
        'cpu_s_synapse': round(after['synapse'] - before['synapse'], 2),
        'cpu_s_livekit': round(after['livekit'] - before['livekit'], 2),
    }
    print(json.dumps(result), flush=True)
    return result


def steady(yapfast, people):
    before = cpu_snapshot(yapfast, people)
    started = time.monotonic()
    yapfast_sample = json.loads(subprocess.check_output(
        ['python3', str(ROOT / 'scripts/measure-process'), str(yapfast.pid), '--seconds', str(args.seconds)], text=True))
    elapsed = time.monotonic() - started
    after = cpu_snapshot(yapfast, people)
    clients = []
    for person in people:
        memory = process_memory_mib(person.process.pid)
        cpu = (after['people'][person.account.name] - before['people'][person.account.name]) / elapsed * 100
        clients.append({'participant': person.account.name, 'cpu_percent_one_core': round(cpu, 2),
                        'pss_mib': round(memory['pss'], 1), 'rss_mib': round(memory['rss'], 1),
                        'missing_key_events': person.status().get('missing_key_events', 0),
                        'decryption_failures': person.status().get('decryption_failures', 0)})

    def summary(key):
        values = [client[key] for client in clients]
        return {'mean': round(statistics.mean(values), 2), 'min': min(values), 'max': max(values)}

    result = {
        'yapfast': {key: yapfast_sample[key] for key in ('cpu_percent_one_core', 'pss_mib_mean', 'rss_mib_mean')},
        'headless_participant_cpu_percent_one_core': summary('cpu_percent_one_core'),
        'headless_participant_pss_mib': summary('pss_mib'),
        'headless_participant_rss_mib': summary('rss_mib'),
        'missing_key_events_total': sum(client['missing_key_events'] for client in clients),
        'decryption_failures_total': sum(client['decryption_failures'] for client in clients),
        # Pairs of headless participants where one cannot currently hear the other.
        'unheard_pairs': sum(1 for listener in people for speaker in people if listener is not speaker
                             and speaker.status()['member_id'] not in listener.status().get('decrypting', [])),
        'synapse_cpu_percent_one_core': round((after['synapse'] - before['synapse']) / elapsed * 100, 2),
        'livekit_cpu_percent_one_core': round((after['livekit'] - before['livekit']) / elapsed * 100, 2),
        'clients': clients,
    }
    print(json.dumps({key: value for key, value in result.items() if key != 'clients'}), flush=True)
    return result


joins, rejoins, steady_state = [], [], None
people = []
fixture = provision()
print(f'Fixture {fixture}', flush=True)
try:
    # The load helper registers Bob's companions and joins them to the rooms, then exits.
    count = args.people - 1
    (fixture / 'load.commands').write_text('exit\n')
    with (fixture / 'load.log').open('ab') as log:
        subprocess.run([str(args.participant_executable.resolve()), '--exact', 'matrix_voice::tests::load_participants',
                        '--ignored', '--nocapture'], cwd=ROOT, stdout=log, stderr=log, check=True, timeout=1200,
                       env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                                YAPFAST_TEST_EXTRA_ACCOUNTS=str(count - 1)))
    accounts = [fixture / 'bob'] + [fixture / f'load-{index}' for index in range(count - 1)]
    log = (args.output / 'yapfast.log').open('wb')
    yapfast = subprocess.Popen([str(args.exe.resolve()), '--renderer', 'gpu', '--data-dir', str(fixture / 'alice')],
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
        people = [Participant(str(args.participant_executable.resolve()), fixture, account, index, count)
                  for index, account in enumerate(accounts)]
        if wait_for(lambda: all(person.status().get('phase') == 'ready' for person in people), 300) is None:
            raise RuntimeError('participants did not finish syncing')
        time.sleep(args.gap)
        for position, person in enumerate(people):
            joins.append(join(person, people[:position], yapfast, people))
            time.sleep(args.gap)
        capture('full')
        steady_state = steady(yapfast, people)
        last = people[-1]
        for cycle in range(args.rejoins):
            last.send('leave')
            if wait_for(lambda: last.status().get('phase') == 'ready', args.timeout) is None:
                raise RuntimeError('rejoin participant did not leave')
            leave_s = round(last.status().get('leave_ms', 0) / 1000, 2)
            time.sleep(args.gap)
            result = join(last, people[:-1], yapfast, people)
            result['leave_call_s_before'] = leave_s
            rejoins.append(result)
            time.sleep(args.gap)
        capture('after-rejoins')
        click(WIDTH - 19, -20)  # Close on the private window's title bar.
        yapfast.wait(timeout=20)
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
        'people': args.people, 'gap_seconds': args.gap,
        'joins': joins, 'steady_state': steady_state, 'rejoins': rejoins,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
