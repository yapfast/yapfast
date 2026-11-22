#!/usr/bin/env python3
"""Exercise voice reliability with real participant processes on the local stack.

Each participant is a separate process (LiveKit shares one audio engine per
process). Bob runs in a private network namespace through pasta, so freezing
pasta drops only Bob's traffic, including UDP media, while Alice stays online.
Audio uses temporary null devices; nothing is recorded and no physical device
is selected. Participants stay muted, so no microphone track is published.

Start the stack first: scripts/test-stack up
"""
import argparse
import http.client
import http.server
import json
import math
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import threading
import time
import wave

from voice_harness import provision as voice_demo, test_binary

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--wait-alone', type=float, default=600, help='seconds Alice waits alone (default 600)')
parser.add_argument('--short-outage', type=float, default=20)
parser.add_argument('--long-outage', type=float, default=75)
parser.add_argument('--republish-outage', type=float, default=30, help='longer than LiveKit\'s 20 s departure timeout')
parser.add_argument('--crash-limit', type=float, default=420, help='seconds allowed for crash cleanup')
parser.add_argument('--report', type=Path, default=ROOT / '.local/voice-reliability.json')
parser.add_argument('--skip', action='append', default=[], choices=['outages', 'republish', 'unverified', 'crash', 'capabilities'])
args = parser.parse_args()

for tool in ('pactl', 'pw-play', 'parec', 'pasta', 'nft', 'ip', 'cargo'):
    if not shutil.which(tool):
        raise SystemExit(f'{tool} is required; run inside the development shell on a Linux desktop with PipeWire.')


# Without a session bus the fixture uses Yapfast's private-file credential fallback.
# This keeps disposable secrets out of the desktop keyring, and works inside
# Bob's namespace, where D-Bus would see a different user ID.
NO_BUS = dict(os.environ, DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent/yapfast-test-bus')


def provision():
    return voice_demo(env=NO_BUS)


NAMESPACE_MEDIA = """
gateway=$(ip -4 route show default | awk '{print $3; exit}')
nft -f - <<RULES
table ip yapfast_test {
  chain output {
    type nat hook output priority -100;
    ip daddr 127.0.0.1 udp dport 50100-50120 dnat to $gateway
    ip daddr 127.0.0.1 tcp dport 17881 dnat to $gateway
  }
}
RULES
exec "$@"
"""


class Participant:
    def __init__(self, name, executable, fixture, environment, isolated, test='matrix_voice::tests::scripted_participant'):
        self.name = name
        self.fixture = fixture
        self.commands = fixture / f'{name}.commands'
        self.state_path = fixture / f'{name}.state.json'
        self.commands.write_text('')
        self.state_path.unlink(missing_ok=True)
        command = [executable, '--exact', test, '--ignored', '--nocapture']
        if isolated:
            # Matrix, authorization and LiveKit signalling use loopback sockets,
            # which pasta forwards to the host. WebRTC binds the namespace's LAN
            # address, which pasta cannot splice to loopback, so the SFU's media
            # ports go to pasta's gateway; pasta maps that to the host loopback.
            # IPv4 only: the stack listens on 127.0.0.1, and a forwarded ::1 connection
            # is reset rather than refused, so clients would not fall back to IPv4.
            command = ['pasta', '--config-net', '--quiet', '--ipv4-only', '-T', '18008,16080,17880', '--',
                       'sh', '-ec', NAMESPACE_MEDIA, 'namespace'] + command
        self.log = (fixture / f'{name}.log').open('ab')
        self.process = subprocess.Popen(command, cwd=ROOT, stdout=self.log, stderr=self.log,
                                        env=dict(environment, YAPFAST_TEST_PARTICIPANT=name),
                                        start_new_session=True)
        self.isolated = isolated

    def send(self, command):
        with self.commands.open('a') as commands:
            commands.write(command + '\n')

    def state(self):
        try:
            return json.loads(self.state_path.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return None

    def wait(self, description, predicate, timeout, watch=None):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            exited = self.process.poll() is not None
            state = self.state()
            if watch and state:
                watch(state)
            if state and predicate(state):
                return state
            if exited:
                raise AssertionError(f'{self.name} exited while waiting for {description}')
            time.sleep(.1)
        raise AssertionError(f'{self.name}: {description} not reached in {timeout:.0f}s; last state {self.state()}')

    def network(self, available):
        assert self.isolated
        os.kill(self.process.pid, signal.SIGCONT if available else signal.SIGSTOP)

    def kill(self):
        os.killpg(self.process.pid, signal.SIGKILL)
        self.process.wait(timeout=10)

    def stop(self):
        if self.process.poll() is None:
            if self.isolated:
                self.network(True)
            self.send('exit')
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.kill()
        self.log.close()


def start_proxy(edit_features):
    """Forward to the local homeserver, editing its advertised unstable features."""
    class Proxy(http.server.BaseHTTPRequestHandler):
        protocol_version = 'HTTP/1.1'

        def log_message(self, *_):
            pass

        def forward(self):
            body = self.rfile.read(int(self.headers.get('Content-Length') or 0))
            upstream = http.client.HTTPConnection('127.0.0.1', 18008, timeout=90)
            # Ask for identity encoding so the versions response can be edited.
            headers = {key: value for key, value in self.headers.items()
                       if key.lower() not in ('host', 'connection', 'accept-encoding')}
            upstream.request(self.command, self.path, body=body, headers=headers)
            response = upstream.getresponse()
            payload = response.read()
            if self.path.split('?')[0] == '/_matrix/client/versions' and response.status == 200:
                versions = json.loads(payload)
                versions['unstable_features'] = edit_features(versions.get('unstable_features', {}))
                payload = json.dumps(versions).encode()
            self.send_response(response.status)
            for key, value in response.getheaders():
                if key.lower() not in ('content-length', 'transfer-encoding', 'connection'):
                    self.send_header(key, value)
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass  # The client stopped, for example during a long sync request.
            upstream.close()

        do_GET = do_POST = do_PUT = do_DELETE = forward

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Proxy)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def tone_file(fixture):
    """Three minutes of a generated 440 Hz tone; nothing is recorded."""
    path = fixture / 'tone.wav'
    second = b''.join(struct.pack('<h', round(4000 * math.sin(2 * math.pi * 440 * i / 48000))) for i in range(48000))
    with wave.open(str(path), 'wb') as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(48000)
        for _ in range(180):
            output.writeframesraw(second)
    return path


def output_energy():
    """440 Hz amplitude reaching Alice's virtual speakers over three seconds."""
    samples = subprocess.run(['timeout', '3s', 'parec', '--device=yapfast_reliability_output.monitor',
                              '--format=s16le', '--rate=48000', '--channels=1', '--latency-msec=50'],
                             capture_output=True).stdout
    values = struct.unpack(f'<{len(samples) // 2}h', samples[:len(samples) // 2 * 2])
    if len(values) < 48000:
        raise AssertionError('the virtual speaker monitor produced too few samples')
    real = sum(value * math.cos(2 * math.pi * 440 * i / 48000) for i, value in enumerate(values))
    imaginary = sum(value * math.sin(2 * math.pi * 440 * i / 48000) for i, value in enumerate(values))
    return math.hypot(real, imaginary) * 2 / len(values)


def roster(state):
    return sorted(state['participants'])


modules = []
participants = []
report = {'date': time.strftime('%Y-%m-%d'), 'physical_audio': False, 'muted_participants': True}
try:
    for module in (
        ['module-null-sink', 'sink_name=yapfast_reliability_output', 'sink_properties=device.description=Yapfast_reliability_speakers'],
        ['module-null-sink', 'sink_name=yapfast_reliability_input', 'sink_properties=device.description=Yapfast_reliability_input'],
    ):
        modules.append(subprocess.check_output(['pactl', 'load-module', *module], text=True).strip())
    executable = test_binary()
    fixture = provision()
    manifest = json.loads((fixture / 'rooms.json').read_text())
    alice_id, bob_id = manifest['alice'], manifest['bob']
    environment = dict(NO_BUS, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                       PULSE_SINK='yapfast_reliability_output', PULSE_SOURCE='yapfast_reliability_input.monitor')
    print(f'Fixture {fixture}', flush=True)

    alice = Participant('alice', executable, fixture, environment, isolated=False)
    participants.append(alice)
    alice.wait('ready', lambda state: state['phase'] == 'Disconnected', 60)
    alice.send('join')
    joined = alice.wait('connected alone', lambda state: state['phase'] == 'Connected' and roster(state) == [alice_id], 60)

    def steady_alone(state):
        if state['phase'] != 'Connected' or roster(state) != [alice_id] or state['error'] or state['warning']:
            raise AssertionError(f'waiting alone changed state: {state}')

    started = time.monotonic()
    print(f'Alice waits alone for {args.wait_alone:.0f}s', flush=True)
    while time.monotonic() - started < args.wait_alone:
        steady_alone(alice.state())
        time.sleep(1)
    report['wait_alone_seconds'] = round(time.monotonic() - started)
    report['wait_alone_state_changes'] = alice.state()['sequence'] - joined['sequence']

    bob = Participant('bob', executable, fixture, environment, isolated=True)
    participants.append(bob)
    bob.wait('ready', lambda state: state['phase'] == 'Disconnected', 60)
    bob.send('join')
    both = sorted([alice_id, bob_id])
    bob.wait('connected with Alice', lambda state: state['phase'] == 'Connected' and roster(state) == both, 60)
    alice.wait('sees Bob', lambda state: roster(state) == both, 30)
    report['late_join_after_wait'] = True
    print('PASS late join after waiting alone', flush=True)

    if 'outages' not in args.skip:
        phases = set()
        bob.network(False)
        paused = time.monotonic()
        while time.monotonic() - paused < args.short_outage:
            alice_state = alice.state()
            if alice_state['phase'] != 'Connected':
                raise AssertionError(f'Alice lost voice during Bob\'s outage: {alice_state}')
            time.sleep(.2)
        bob.network(True)
        resumed = time.monotonic()
        recovered = bob.wait('recovered after short outage',
                             lambda state: state['phase'] == 'Connected' and roster(state) == both, 90,
                             watch=lambda state: phases.add(state['phase']))
        alice.wait('sees Bob after short outage', lambda state: roster(state) == both, 60)
        report['short_outage'] = {'seconds': args.short_outage, 'recovered': True,
                                  'recovery_seconds_after_restore': round(time.monotonic() - resumed, 1),
                                  'bob_phases': sorted(phases), 'bob_error': recovered['error']}
        print(f'PASS short outage: {report["short_outage"]}', flush=True)

        bob.network(False)
        time.sleep(args.long_outage)
        bob.network(True)
        resumed = time.monotonic()
        ended = bob.wait('ends after long outage', lambda state: state['phase'] == 'Disconnected', 60)
        if not ended['error']:
            raise AssertionError(f'long outage ended without an explanation: {ended}')
        bob.send('join')
        bob.wait('rejoined after long outage',
                 lambda state: state['phase'] == 'Connected' and bob_id in roster(state), 90)
        rejoined = alice.wait('sees Bob after rejoin', lambda state: bob_id in roster(state), 60)
        report['long_outage'] = {'seconds': args.long_outage, 'ended_with_error': ended['error'],
                                 'rejoined': True,
                                 'rejoin_seconds_after_restore': round(time.monotonic() - resumed, 1),
                                 'alice_entries_for_bob_after_rejoin': roster(rejoined).count(bob_id)}
        print(f'PASS long outage: {report["long_outage"]}', flush=True)
        stale = time.monotonic()
        alice.wait('drops Bob\'s stale membership', lambda state: roster(state) == both, args.crash_limit)
        report['long_outage']['stale_entry_seconds_after_rejoin'] = round(time.monotonic() - stale, 1)

    if 'republish' not in args.skip:
        # Bob publishes a generated tone. Alice leaving rotates Bob's key, and
        # her rejoin holds only the new index. A full LiveKit reconnect then
        # republishes Bob's microphone with a new frame cryptor at index 0.
        tone = subprocess.Popen(['pw-play', '--volume=1.0', '--target=yapfast_reliability_input', str(tone_file(fixture))])
        try:
            bob.send('unmute')
            bob.wait('unmuted', lambda state: not state['muted'], 30)
            time.sleep(2)
            before = output_energy()
            if before < 50:
                raise AssertionError(f'Alice did not hear Bob before the rotation: {before:.1f}')
            alice.send('leave')
            alice.wait('left for rotation', lambda state: state['phase'] == 'Disconnected', 30)
            time.sleep(3)
            alice.send('join')
            alice.wait('rejoined after rotation', lambda state: state['phase'] == 'Connected' and roster(state) == both, 60)
            time.sleep(3)
            rotated = output_energy()
            if rotated < 50:
                raise AssertionError(f'Alice did not hear Bob after the key rotation: {rotated:.1f}')
            bob.network(False)
            time.sleep(args.republish_outage)
            bob.network(True)
            bob.wait('recovered after full reconnect', lambda state: state['phase'] == 'Connected', 90)
            time.sleep(4)
            after = output_energy()
            republished = 'microphone republished after reconnecting' in (fixture / 'bob.log').read_text(errors='replace')
            report['republish'] = {'outage_seconds': args.republish_outage, 'republished': republished,
                                   'amplitude_before': round(before, 1), 'amplitude_after_rotation': round(rotated, 1),
                                   'amplitude_after_reconnect': round(after, 1)}
            if not republished:
                raise AssertionError(f'the outage did not force a full reconnect: {report["republish"]}')
            if after < 50:
                raise AssertionError(f'Alice lost Bob\'s audio after the full reconnect: {report["republish"]}')
            bob.send('mute')
            bob.wait('muted again', lambda state: state['muted'], 30)
            print(f'PASS audio survives a full reconnect after key rotation: {report["republish"]}', flush=True)
        finally:
            tone.terminate()
            tone.wait(timeout=5)

    if 'unverified' not in args.skip:
        # A peer whose device is not cross-signed gets no media key under the
        # identity-based policy. Alice must say so, without muting herself.
        unverified = Participant('unverified', executable, fixture, dict(environment, YAPFAST_TEST_LOGIN_USER=bob_id),
                                 isolated=False, test='matrix_voice::tests::unverified_participant')
        participants.append(unverified)
        unverified.wait('joined', lambda state: state['phase'] == 'Connected', 90)
        warned = alice.wait('warns about the unverified device',
                            lambda state: any(person['user'] == bob_id and person['problem']
                                              and 'Unverified' in person['problem'] for person in state['people']), 40)
        if warned['error'] or warned['phase'] != 'Connected':
            raise AssertionError(f"an unverified peer disturbed Alice's call: {warned}")
        report['unverified_peer'] = {'warning': warned['warning'], 'alice_error': warned['error']}
        unverified.send('exit')
        unverified.wait('left', lambda state: state['phase'] == 'Disconnected', 30)
        unverified.stop()
        alice.wait('drops the unverified device', lambda state: roster(state) == both, 60)
        print(f'PASS unverified peer warning: {warned["warning"]}', flush=True)

    if 'crash' not in args.skip:
        # Killed, then restarted: startup removes the membership it left.
        bob.wait('connected before crash', lambda state: state['phase'] == 'Connected', 30)
        bob.kill()
        participants.remove(bob)
        bob = Participant('bob', executable, fixture, environment, isolated=True)
        participants.append(bob)
        bob.wait('ready after restart', lambda state: state['phase'] == 'Disconnected', 60)
        restarted = time.monotonic()
        bob.send('leave-stale')
        alice.wait('drops the stale membership after restart', lambda state: roster(state) == [alice_id], 60)
        report['crash_then_restart_cleanup_seconds'] = round(time.monotonic() - restarted, 1)
        print(f'PASS restart removes the crashed membership in {report["crash_then_restart_cleanup_seconds"]}s', flush=True)

        # Killed for good: the membership expires; meanwhile Alice shows it as
        # not connected rather than as a muted listener.
        # Bob publishes first: LiveKit lists only publishing participants, so a
        # lost media connection is visible only for someone who unmuted.
        bob.send('join')
        bob.wait('rejoined', lambda state: state['phase'] == 'Connected' and roster(state) == both, 90)
        bob.send('unmute')
        bob.wait('unmuted before the second crash', lambda state: not state['muted'], 30)
        alice.wait('sees Bob connected before the second crash',
                   lambda state: roster(state) == both and all(person['connected'] for person in state['people']), 30)
        bob.kill()
        participants.remove(bob)
        killed = time.monotonic()
        marked = {}

        def watch_crash(state):
            if state['phase'] != 'Connected':
                raise AssertionError(f'Alice disconnected after Bob crashed: {state}')
            if 'not_connected' not in marked and any(person['user'] == bob_id and not person['connected']
                                                      for person in state['people']):
                marked['not_connected'] = round(time.monotonic() - killed, 1)

        alice.wait('drops crashed Bob', lambda state: roster(state) == [alice_id], args.crash_limit, watch=watch_crash)
        report['crash_cleanup_seconds'] = round(time.monotonic() - killed, 1)
        report['crash_marked_connection_lost_seconds'] = marked.get('not_connected')
        if marked.get('not_connected') is None:
            raise AssertionError('Alice never showed the crashed participant as having lost their connection')
        print(f'PASS crash cleanup after {report["crash_cleanup_seconds"]}s; shown as connection lost after '
              f'{report["crash_marked_connection_lost_seconds"]}s', flush=True)

    if 'capabilities' not in args.skip:
        # A homeserver without sticky events gets an actionable refusal.
        proxy = start_proxy(lambda features: {key: value for key, value in features.items() if 'msc4354' not in key})
        nosticky = Participant('nosticky', executable, fixture,
                               dict(environment, YAPFAST_TEST_LOGIN=f'http://127.0.0.1:{proxy.server_port} {alice_id}'),
                               isolated=False)
        participants.append(nosticky)
        nosticky.wait('ready', lambda state: state['phase'] == 'Disconnected', 90)
        nosticky.send('join')
        refused = nosticky.wait('refuses voice', lambda state: state['phase'] == 'Disconnected' and state['error'], 60)
        if 'MSC4354' not in refused['error']:
            raise AssertionError(f'missing sticky events were not explained: {refused}')
        report['missing_sticky_events_error'] = refused['error']
        nosticky.stop()
        participants.remove(nosticky)
        proxy.shutdown()
        print(f'PASS missing sticky events: {refused["error"]}', flush=True)

    alice.send('leave')
    left = alice.wait('left cleanly', lambda state: state['phase'] == 'Disconnected', 30)
    if left['error']:
        raise AssertionError(f'leave reported an error: {left}')
    report['clean_leave'] = True
    report['passed'] = True
except BaseException as error:
    report['passed'] = False
    report['failure'] = str(error)
    raise
finally:
    for participant in participants:
        participant.stop()
    for module in reversed(modules):
        subprocess.run(['pactl', 'unload-module', module], check=False)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Report {args.report}', flush=True)
