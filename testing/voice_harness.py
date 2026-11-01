"""Shared pieces of the voice stress scripts: fixtures, screenshots and counters.

The scripts run inside scripts/run-framework-isolated, with the local stack up.
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
WIDTH, HEIGHT = 1120, 760
TICKS = os.sysconf('SC_CLK_TCK')


# The other people, the fixture and the tests run from the `fast` profile, which
# everyday builds keep current. The Yapfast a script measures comes separately,
# from the `measure` profile, which optimizes the Slint interface too.


def test_binary():
    """The yapfast-core test executable, which runs the other people."""
    output = subprocess.run(['cargo', 'test', '--locked', '--profile', 'fast', '-p', 'yapfast-core', '--features', 'matrix',
                             '--no-run', '--message-format=json'], check=True, capture_output=True, text=True, cwd=ROOT).stdout
    for line in output.splitlines():
        message = json.loads(line)
        if message.get('reason') == 'compiler-artifact' and message.get('executable') \
                and message['target']['name'] == 'yapfast_core' and 'lib' in message['target']['kind']:
            return message['executable']
    raise SystemExit('could not find the yapfast-core test executable')


def provision(env=None):
    """Create a voice-demo fixture: Alice, Bob, a Space, a text and a voice room."""
    output = subprocess.run(['cargo', 'run', '--locked', '--profile', 'fast', '--features', 'matrix', '--', '--voice-demo'],
                            check=True, capture_output=True, text=True, cwd=ROOT, env=env).stdout
    for line in output.splitlines():
        if '--data-dir' in line and line.rstrip().endswith('/alice'):
            return ROOT / Path(line.split('--data-dir', 1)[1].strip()).parent
    raise SystemExit('voice demo did not report its accounts:\n' + output)


def capture(folder, name):
    time.sleep(.3)
    with tempfile.TemporaryDirectory(prefix='yapfast-voice-shot-') as temporary:
        subprocess.run(['weston-screenshooter'], env=dict(os.environ, XDG_PICTURES_DIR=temporary),
                       check=True, timeout=10, capture_output=True)
        picture = next(Path(temporary).glob('*.png'))
        path = Path(folder) / f'{name}.png'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', str(picture),
                        '-vf', 'vflip', str(path)], check=True, timeout=10)
        return path


def locate_rail(picture, minimum_height=HEIGHT // 2):
    """Top-left corner of Yapfast's window content in screenshot pixels, found by the rail's exact colour."""
    # The rail's exact graphite (Theme.rail, #111215) forms the window's long left edge.
    output_width, output_height = (int(value) for value in subprocess.check_output(
        ['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height',
         '-of', 'csv=p=0:s=x', str(picture)], text=True, timeout=10).strip().split('x'))
    pixels = subprocess.check_output(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(picture),
                                      '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=10)
    target = bytes([17, 18, 21])
    columns = [sum(pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target
                   for y in range(output_height)) for x in range(output_width)]
    best = max(columns)
    if best < minimum_height:
        raise RuntimeError(f'Yapfast rail was not found; inspect {picture}')
    x = next(x for x, count in enumerate(columns) if count >= best - 2)
    ys = [y for y in range(output_height)
          if pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target]
    return x, min(ys)


class Container:
    """CPU and network counters of one test-stack container, read from the host."""

    def __init__(self, name):
        details = json.loads(subprocess.check_output(['docker', 'inspect', name]))[0]
        self.cpu = Path(f'/sys/fs/cgroup/system.slice/docker-{details["Id"]}.scope/cpu.stat')
        self.net = Path(f'/proc/{details["State"]["Pid"]}/net/dev')

    def read(self):
        usage = next(int(line.split()[1]) for line in self.cpu.read_text().splitlines()
                     if line.startswith('usage_usec'))
        fields = next(line.split(':', 1)[1].split() for line in self.net.read_text().splitlines()
                      if line.strip().startswith('eth0:'))
        return {'cpu_seconds': usage / 1e6, 'rx_bytes': int(fields[0]), 'rx_packets': int(fields[1]),
                'tx_bytes': int(fields[8]), 'tx_packets': int(fields[9])}


def process_cpu_seconds(pid):
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / TICKS


def process_memory_mib(pid):
    """PSS and RSS from smaps_rollup, in MiB."""
    values = {}
    for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines():
        name, _, rest = line.partition(':')
        if name in ('Rss', 'Pss'):
            values[name.lower()] = int(rest.split()[0]) / 1024
    return values


GPU_DRIVER = ('/dev/dri', 'nvidia', 'libGL', 'libEGL', 'libgbm', 'libdrm', 'libglapi', 'libLLVM', 'gallium',
              'dri/', 'mesa', 'libvulkan')


def process_memory_breakdown(pid):
    """PSS by mapping kind and the anonymous total, in MiB, from smaps."""
    executable = os.path.realpath(f'/proc/{pid}/exe')
    kinds, anonymous, kind = {}, 0, None
    for line in Path(f'/proc/{pid}/smaps').read_text().splitlines():
        fields = line.split()
        if '-' in fields[0] and not fields[0].endswith(':'):
            path = fields[5] if len(fields) > 5 else ''
            kind = ('binary' if path == executable else
                    'gpu_driver' if any(part in path for part in GPU_DRIVER) else
                    'shared_libraries' if '.so' in path else
                    'heap_and_anonymous' if path in ('', '[heap]') or path.startswith('[anon') else 'other')
        elif fields[0] == 'Pss:':
            kinds[kind] = kinds.get(kind, 0) + int(fields[1]) / 1024
        elif fields[0] == 'Anonymous:':
            anonymous += int(fields[1]) / 1024
    return {'pss_mib': {name: round(value, 1) for name, value in sorted(kinds.items())},
            'anonymous_mib': round(anonymous, 1)}


def host_ticks():
    values = [int(value) for value in Path('/proc/stat').read_text().splitlines()[0].split()[1:]]
    return sum(values), values[3] + values[4]


class Participant:
    """One headless person (matrix_voice::tests::stress_participant) in its own process, driven by a command file."""

    def __init__(self, executable, fixture, account, index, count):
        self.account = account
        self.commands = account / 'commands'
        self.commands.write_text('')
        (account / 'status.json').unlink(missing_ok=True)
        self.log = (account / 'participant.log').open('ab')
        self.process = subprocess.Popen(
            [executable, '--exact', 'matrix_voice::tests::stress_participant', '--ignored', '--nocapture'],
            cwd=ROOT, stdout=self.log, stderr=self.log, start_new_session=True,
            env=dict(os.environ, YAPFAST_TEST_VOICE_ROOT=str(fixture), YAPFAST_TEST_ACCOUNT_DIR=str(account),
                     YAPFAST_TEST_INDEX=str(index), YAPFAST_TEST_COUNT=str(count)))

    def send(self, command):
        with self.commands.open('a') as commands:
            commands.write(command + '\n')

    def status(self):
        if self.process.poll() is not None:
            raise RuntimeError(f'participant exited; see {self.account / "participant.log"}')
        try:
            return json.loads((self.account / 'status.json').read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return {}

    def stop(self):
        if self.process.poll() is None:
            self.send('exit')
            try:
                self.process.wait(timeout=60)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
        self.log.close()


def wait_for(condition, timeout):
    """Seconds until condition() held, or None after timeout."""
    started = time.monotonic()
    while time.monotonic() - started < timeout:
        if condition():
            return time.monotonic() - started
        time.sleep(.1)
    return None
