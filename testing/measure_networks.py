#!/usr/bin/env python3
"""Compare Yapfast's memory with only Matrix, only Discord, or both networks built in.

Each build is a `measure` profile binary with different Cargo features (see
docs/building.md); the comparison is between builds, so LTO isn't needed. All run against test data only: Matrix against the local
stack with a seeded fixture account, Discord against testing/fake_discord.ts.
Both histories hold the same 60 messages with the same image, file and reply.
The combined build is measured twice, once with each network's channel open.

Each profile opens #general, warms up, samples Yapfast's process while idle, then
samples again while Bob posts a message every 2 s in the open channel. Voice
isn't measured: the fake Discord has no voice server.

Start the local stack (scripts/test-stack up). Copy target/measure/yapfast, built
with each feature set, into --builds (target/networks) as yapfast-matrix,
yapfast-discord and yapfast-combined; docs/measurements/README.md gives the builds.
Find the yapfast-core test binary with
`nix develop path:./nix --command cargo test --profile fast --locked -p yapfast-core --features matrix --no-run --message-format short`
(its "Executable unittests src/lib.rs" line), then run from the repository root:

    nix-shell --impure testing/display/shell.nix --run 'scripts/run-framework-isolated
        --audio --no-services --alsa-plugins "$YAPFAST_FRAMEWORK_ALSA_PLUGINS"
        --compositor-log .local/networks.weston.log --
        python3 testing/measure_networks.py --output .local/measure-networks
        --load-executable target/fast/deps/yapfast_core-HASH'

(one line). Without --load-executable it runs `cargo test --no-run` itself,
with the `fast` profile, inside the private session. This script starts
testing/fake_discord.ts itself.
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
import urllib.request

from voice_harness import test_binary

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--builds', type=Path, default=ROOT / 'target/networks',
                    help='folder with yapfast-matrix, yapfast-discord and yapfast-combined')
parser.add_argument('--profile', action='append', choices=['matrix', 'discord', 'combined-discord', 'combined-matrix'])
parser.add_argument('--renderer', default='software', choices=['gpu', 'software'])
parser.add_argument('--warmup', type=int, default=20, help='seconds before each sample')
parser.add_argument('--seconds', type=int, default=30, help='sample length')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--load-executable', type=Path, help='prebuilt yapfast-core test binary for the Matrix load process')
args = parser.parse_args()
profiles = args.profile or ['matrix', 'discord', 'combined-discord', 'combined-matrix']
args.output.mkdir(parents=True, exist_ok=True)
WIDTH, HEIGHT = 1120, 760
# The fake's ids; see testing/fake_discord.ts.
DISCORD_PORT = 18400
DISCORD = f'http://127.0.0.1:{DISCORD_PORT}'
DISCORD_SERVER = 'discord:1400000000000000100'
DISCORD_GENERAL = 'discord:1400000000000000100:1400000000000000102'
# History length shared by both networks: four fixture messages and 56 more.
EXTRA_MESSAGES = 56
# Rail centres: Yapfast lists Matrix servers before Discord ones.
FIRST_SERVER, SECOND_SERVER = (36, 101), (36, 158)
MATRIX_GENERAL = (141, 148)


def provision(executable):
    output = subprocess.run([str(executable), '--voice-demo'], check=True, capture_output=True, text=True,
                            cwd=ROOT).stdout
    for line in output.splitlines():
        if '--data-dir' in line and line.rstrip().endswith('/alice'):
            return ROOT / Path(line.split('--data-dir', 1)[1].strip()).parent
    raise SystemExit('voice demo did not report its accounts:\n' + output)


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
    result.update(profile=name, **notes)
    print(json.dumps({key: result[key] for key in ('profile', 'state', 'pss_mib_mean', 'rss_mib_mean',
                                                    'cpu_percent_one_core')}), flush=True)
    return result


class Load:
    """Bob on the local Matrix stack, posting in #general on command."""

    def __init__(self, executable, fixture):
        self.fixture = fixture
        self.commands = fixture / 'load.commands'
        self.commands.write_text('')
        self.log = (fixture / 'load.log').open('ab')
        self.process = subprocess.Popen([executable, '--exact', 'matrix_voice::tests::load_participants', '--ignored',
                                         '--nocapture'], cwd=ROOT, stdout=self.log, stderr=self.log,
                                        env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture),
                                                 YAPFAST_TEST_EXTRA_ACCOUNTS='0'),
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


class FakeDiscord:
    def __init__(self, log):
        self.log = log.open('ab')
        self.process = subprocess.Popen(['bun', str(ROOT / 'testing/fake_discord.ts'), '--port', str(DISCORD_PORT),
                                         '--extra', str(EXTRA_MESSAGES)],
                                        stdout=self.log, stderr=self.log, start_new_session=True)
        deadline = time.monotonic() + 15
        while True:
            try:
                urllib.request.urlopen(f'{DISCORD}/_control/health', timeout=1)
                return
            except OSError:
                if self.process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('the fake Discord did not start; see fake-discord.log')
                time.sleep(.2)

    def control(self, command):
        urllib.request.urlopen(urllib.request.Request(f'{DISCORD}/_control/{command}', method='POST'), timeout=5)

    def stop(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGTERM)
            self.process.wait(timeout=10)
        self.log.close()


def remember_discord_channel(data):
    """Clicking the Discord server then opens #general, as after an earlier visit."""
    data.mkdir(parents=True, exist_ok=True)
    path = data / 'layout.json'
    layout = json.loads(path.read_text()) if path.exists() else {}
    layout.setdefault('channels', {})[DISCORD_SERVER] = DISCORD_GENERAL
    path.write_text(json.dumps(layout))


def measure(profile, executable, data, discord, load):
    environment = dict(os.environ, YAPFAST_UI_TEST_SIZE=f'{WIDTH}x{HEIGHT}')
    if profile != 'matrix':
        environment['YAPFAST_DISCORD_TEST_SERVER'] = DISCORD
        remember_discord_channel(data)
    log = (args.output / f'{profile}.log').open('wb')
    process = subprocess.Popen([str(executable), '--renderer', args.renderer, '--data-dir', str(data)],
                               env=environment, stdout=log, stderr=log, start_new_session=True)
    results = []
    try:
        time.sleep(12)
        if process.poll() is not None:
            raise RuntimeError(f'Yapfast exited during startup; see {profile}.log')
        origin_x, origin_y = locate_rail(capture(f'{profile}-startup'))

        def click(point):
            INPUT['click'](origin_x + point[0], origin_y + point[1])
            time.sleep(.5)

        if profile in ('matrix', 'combined-matrix'):
            click(FIRST_SERVER)
            click(MATRIX_GENERAL)
        else:
            click(SECOND_SERVER if profile == 'combined-discord' else FIRST_SERVER)
        notes = {'renderer': args.renderer, 'window': [WIDTH, HEIGHT], 'warmup_seconds': args.warmup,
                 'binary': executable.name, 'binary_bytes': executable.stat().st_size,
                 'open_channel': 'Matrix #general' if profile.endswith('matrix') else 'Discord #general'}
        time.sleep(args.warmup)
        capture(f'{profile}-idle')
        results.append(sample(process.pid, profile, dict(notes, state='settled idle')))

        if profile.endswith('matrix'):
            load.send('chat-start')
        else:
            discord.control('chat-start')
        time.sleep(10)
        results.append(sample(process.pid, profile, dict(notes, state='a message every 2 s')))
        if profile.endswith('matrix'):
            load.send('chat-stop')
        else:
            discord.control('chat-stop')
        capture(f'{profile}-chat')
        click((WIDTH - 19, -20))  # Close on the private window's title bar.
        process.wait(timeout=20)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=20)
        log.close()
    return results


def prewarm(executable, data):
    """The first restore after seeding decrypts and stores the whole history,
    and the heap keeps that memory. One unmeasured run does it for every profile."""
    log = (args.output / 'prewarm.log').open('wb')
    process = subprocess.Popen([str(executable), '--renderer', args.renderer, '--data-dir', str(data)],
                               env=dict(os.environ, YAPFAST_UI_TEST_SIZE=f'{WIDTH}x{HEIGHT}'),
                               stdout=log, stderr=log, start_new_session=True)
    try:
        time.sleep(12)
        origin_x, origin_y = locate_rail(capture('prewarm'))
        for point in (FIRST_SERVER, MATRIX_GENERAL):
            INPUT['click'](origin_x + point[0], origin_y + point[1])
            time.sleep(.5)
        time.sleep(30)
        INPUT['click'](origin_x + WIDTH - 19, origin_y - 20)
        process.wait(timeout=20)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=20)
        log.close()


builds = {name: (args.builds / f'yapfast-{name}').resolve() for name in ('matrix', 'discord', 'combined')}
for name, path in builds.items():
    if not path.exists():
        raise SystemExit(f'missing {path}; copy each feature build of target/measure/yapfast there')
results = []
load = discord = None
executable = str(args.load_executable.resolve()) if args.load_executable else test_binary()
fixture = provision(builds['combined'])
print(f'Fixture {fixture}', flush=True)
subprocess.run([executable, '--exact', 'matrix_chat::tests::fixture_conversation', '--ignored'], check=True,
               cwd=ROOT, capture_output=True,
               env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture), YAPFAST_TEST_EXTRA_MESSAGES=str(EXTRA_MESSAGES)))
try:
    discord = FakeDiscord(args.output / 'fake-discord.log')
    load = Load(executable, fixture)
    load.wait('ready', 600)
    prewarm(builds['matrix'], fixture / 'alice')
    for profile in profiles:
        binary = builds['combined' if profile.startswith('combined') else profile]
        # Discord-only has no Matrix account, so it starts from an empty folder.
        data = fixture / 'discord-only' if profile == 'discord' else fixture / 'alice'
        results += measure(profile, binary, data, discord, load)
        # Restart the fake so every profile starts from the same history.
        discord.stop()
        discord = FakeDiscord(args.output / 'fake-discord.log')
finally:
    if load:
        load.stop()
    if discord:
        discord.stop()
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binaries': {name: {'path': str(path), 'bytes': path.stat().st_size,
                            'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                     for name, path in builds.items()},
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'Private headless Weston (GL renderer, same GPU), private PulseAudio null sink, private D-Bus without services',
        'matrix': 'local stack; Alice with a seeded #general (fixture conversation plus 56 messages); Bob posts from a load process',
        'discord': 'testing/fake_discord.ts; one server with #general (the same 60 messages) and Lounge; Bob posts from the fake',
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
