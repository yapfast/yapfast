#!/usr/bin/env python3
"""Measure the desktop client as its window closes to the tray and opens again.

Alice reads #general on testing/fake_discord.ts, with a few more servers and
DMs, while the window is open and idle, after it closed to the tray (which
drops the window and trims the heap), and after the tray opened it again;
then the same in the Lounge call, and after leaving it. Only Yapfast's
process is sampled.

Memory is anonymous memory plus swap, which is what Yapfast itself holds and
close to the private bytes Windows shows, read as each sample starts and as
it ends. Heap in use is glibc's, from the window control's `stats`. Startup is
the time from launch until Yapfast's CPU stays under 5% of a core for a second,
the CPU it used until then, and the CPU it used in its first 3 s.

Runs Yapfast through scripts/ui-session --tray, which builds the Discord-only
`measure` build, or runs --bin, and runs testing/tray_host.rs as the tray, on a
private display and bus.

--parity GROUP... measures instead what docs/measurements/README.md compares
between the two interfaces, on one fake account at 1400x860, for the build
--bin names (the Slint `yapfast` or the iced `anomi`): `dm`, idle on Bob's DM;
`busy`, #general of a server of 300 members while one goes offline or comes
back ten times a second, someone starts to type twice a second and Bob posts
every second, with the member list shown and hidden, then behind another
window (weston-flower over it) and in the tray; `images`, memory after
scrolling up through #general's hundred pictures and back; `scroll`, scrolling
it with 60 touchpad events a second, up and down by turns every two seconds as
testing/measure_scroll.py does; `call`, the Lounge call at rest, with three
people speaking, and watching Bob's stream. Every run also measures the start:
the time to the window's first frame and to settling, and memory then. Each
sample adds the CPU of the main thread, where the interface runs, each
thread's by name, the window's frames in memory and the new frames a second
it sent the display.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import socket
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
SESSION = ROOT / '.local/ui-session'
# --parity's account: #general with 460 messages, every fourth extra one with
# a picture, and 300 more people in its member list.
PARITY_FAKE = '--servers 4 --dms 20 --members 300 --member-looks --nitro --extra 400 --images 4'
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--fake-args', default='--servers 4 --dms 20 --images 4')
parser.add_argument('--renderer', choices=['gpu', 'software'], default='software')
parser.add_argument('--runs', type=int, default=3)
parser.add_argument('--warmup', type=int, default=15, help='seconds before each sample')
parser.add_argument('--seconds', type=int, default=20, help='sample length while open')
parser.add_argument('--closed-seconds', type=int, default=60, help='sample length while closed')
parser.add_argument('--no-call', action='store_true', help='skip the call')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--no-build', action='store_true', help="run the session's last build")
parser.add_argument('--bin', type=Path, help='run this Yapfast build instead of building one')
parser.add_argument('--env', action='append', default=[], metavar='NAME=VALUE',
                    help="set in Yapfast's environment, such as GLIBC_TUNABLES=glibc.malloc.mmap_threshold=131072")
parser.add_argument('--parity', nargs='+', metavar='GROUP', choices=['dm', 'busy', 'images', 'scroll', 'call'],
                    help='measure these groups of the interfaces\' comparison instead of the tray')
args = parser.parse_args()
if args.parity and args.fake_args == parser.get_default('fake_args'):
    args.fake_args = PARITY_FAKE
args.output.mkdir(parents=True, exist_ok=True)
# ui-session hands its environment on to Yapfast.
os.environ.update(variable.split('=', 1) for variable in args.env)


def session(*arguments):
    subprocess.run([str(ROOT / 'scripts/ui-session'), *arguments], check=True, cwd=ROOT, stdout=subprocess.DEVNULL)


def field(path, key):
    return next(int(line.split()[1]) for line in Path(path).read_text().splitlines()
                if line.startswith(key)) / 1024


def heap():
    """glibc's heap held and in use, in MiB, from the last `stats` in Yapfast's log."""
    log = SESSION / 'yapfast.log'
    before = log.stat().st_size
    session('do', 'window stats')
    text = log.read_text(errors='replace')[before:]
    totals = re.findall(r'Total \(incl\. mmap\):\s*system bytes\s*=\s*(\d+)\s*in use bytes\s*=\s*(\d+)', text)
    mib = lambda value: round(int(value) / 2**20, 2)
    # Each `stats` lists the arenas glibc has made, from Arena 0.
    arenas = re.findall(r'^Arena (\d+):', text, re.M)
    return {'heap_held_mib': mib(totals[-1][0]) if totals else None,
            'heap_in_use_mib': mib(totals[-1][1]) if totals else None,
            'arenas': max(map(int, arenas)) + 1 if arenas else None}


def huge_pages(pid):
    return round(field(f'/proc/{pid}/smaps_rollup', 'AnonHugePages:'), 1)


def anonymous(pid):
    return round(field(f'/proc/{pid}/smaps_rollup', 'Anonymous:') + field(f'/proc/{pid}/status', 'VmSwap:'), 1)


def frames(pid):
    """The window's frames, which the app draws itself into shared memory
    the display reads: the memory README.md counts beside the anonymous."""
    total, name = 0, ''
    for line in Path(f'/proc/{pid}/smaps').read_text().splitlines():
        if line[:1] in '0123456789abcdef' and ' ' in line and '-' in line.split(' ', 1)[0]:
            name = line.split(None, 5)[5] if len(line.split(None, 5)) > 5 else ''
        elif line.startswith('Rss:') and name.startswith('/memfd:') and 'pulseaudio' not in name:
            total += int(line.split()[1])
    return round(total / 1024, 1)


def sample(name, seconds, notes):
    pid = int((SESSION / 'yapfast.pid').read_text())
    first = anonymous(pid)
    started = time.monotonic_ns()
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(pid),
                                                 '--seconds', str(seconds)], text=True))
    result.update(profile=name, anonymous_mib=anonymous(pid), anonymous_mib_at_start=first, **heap(),
                  frames_mib=frames(pid), huge_pages_mib=huge_pages(pid), window_ns=[started, time.monotonic_ns()],
                  threads=len(os.listdir(f'/proc/{pid}/task')), **notes)
    result['memory_mib'] = round(result['anonymous_mib'] + result['frames_mib'], 1)
    keys = ('profile', 'run', 'anonymous_mib', 'frames_mib', 'heap_in_use_mib', 'rss_mib_mean',
            'cpu_percent_one_core', 'cpu_percent_main_thread', 'wakeups_per_second_main_thread',
            'wakeups_per_second_all_threads') if args.parity else (
            'profile', 'run', 'anonymous_mib_at_start', 'anonymous_mib', 'heap_in_use_mib', 'huge_pages_mib',
            'pss_mib_mean', 'cpu_percent_one_core', 'threads', 'arenas')
    print(json.dumps({key: result[key] for key in keys}), flush=True)
    return result


def watch_startup(found):
    """From launch until the CPU stays under 5% of a core for a second: how
    long, and how much CPU. Started before the session, it waits for the PID."""
    pid_file = SESSION / 'yapfast.pid'
    deadline = time.monotonic() + 240
    while not pid_file.exists() or not pid_file.read_text().strip():
        if time.monotonic() > deadline:
            return
        time.sleep(.01)
    pid = int(pid_file.read_text())
    found['pid'] = pid
    hertz = os.sysconf('SC_CLK_TCK')
    stat = lambda: Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    started = int(stat()[19]) / hertz
    # Where the start lies on the display's clock, which the surface log keeps.
    found['started_monotonic_s'] = started - (time.clock_gettime(time.CLOCK_BOOTTIME) - time.monotonic())
    readings = []
    while not readings or readings[-1][0] < 15:
        try:
            fields = stat()
            status = Path(f'/proc/{pid}/status').read_text()
        except FileNotFoundError:
            return
        memory = {key: int(line.split()[1]) / 1024 for line in status.splitlines()
                  for key in ('VmRSS:', 'RssAnon:', 'VmSwap:') if line.startswith(key)}
        readings.append((time.clock_gettime(time.CLOCK_BOOTTIME) - started, (int(fields[11]) + int(fields[12])) / hertz,
                         memory))
        time.sleep(.02)
    found['cpu_seconds_in_first_3_s'] = next(cpu for at, cpu, _ in readings if at >= 3)
    for at, cpu, memory in readings:
        later = [reading for reading in readings if reading[0] >= at + 1]
        if later and later[0][1] - cpu < .05:
            found.update(startup_seconds=round(at, 2), startup_cpu_seconds=round(cpu, 2),
                         rss_mib_at_startup=round(memory.get('VmRSS:', 0), 1),
                         anonymous_mib_at_startup=round(memory.get('RssAnon:', 0) + memory.get('VmSwap:', 0), 1))
            return


def cycle(prefix, notes, results):
    """Open and idle, closed to the tray, and opened again."""
    time.sleep(args.warmup)
    shot = prefix.lstrip('#').replace(' ', '-').lower()
    session('do', f'shot {shot}-open')
    results.append(sample(f'{prefix}, window open', args.seconds, notes))
    session('do', 'window close')
    time.sleep(5)
    results.append(sample(f'{prefix}, closed to the tray', args.closed_seconds, notes))
    session('do', 'tray activate')
    time.sleep(args.warmup)
    session('do', f'shot {shot}-reopened')
    results.append(sample(f'{prefix}, opened again', args.seconds, notes))


def control(*names):
    session('do', *(f'control {name}' for name in names))


def display_environment():
    """The private display's, read from the app's environment, for a window over it."""
    pid = int((SESSION / 'yapfast.pid').read_text())
    entries = Path(f'/proc/{pid}/environ').read_bytes().decode(errors='replace').split('\0')
    found = dict(entry.split('=', 1) for entry in entries if '=' in entry)
    return dict(os.environ, WAYLAND_DISPLAY=found['WAYLAND_DISPLAY'], XDG_RUNTIME_DIR=found['XDG_RUNTIME_DIR'])


def settled(name, notes, results, shot=None):
    time.sleep(args.warmup)
    if shot:
        session('do', f'shot {shot}')
    results.append(sample(name, args.seconds, notes))


def busy(notes, results):
    """#general of a busy server: presence alone with the member list shown and
    hidden, then typing and a message a second too, in front, behind another
    window and in the tray."""
    control('churn-start')
    settled('Presence churn, member list shown', notes, results, 'churn-shown')
    session('do', 'click 1118 25')  # The member list's button.
    settled('Presence churn, member list hidden', notes, results, 'churn-hidden')
    session('do', 'click 1118 25')
    control('typing-start', 'chat-start?every=1000')
    settled('Busy server, in front', notes, results, 'busy-front')
    flower = subprocess.Popen(['weston-flower'], env=display_environment(),
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        settled('Busy server, behind another window', notes, results, 'busy-behind')
    finally:
        flower.terminate()
        flower.wait()
    session('do', 'sleep 2', 'window close')
    settled('Busy server, in the tray', notes, results)
    session('do', 'tray activate', 'sleep 3')
    control('churn-stop', 'typing-stop', 'chat-stop')


def images(notes, results):
    """Up through #general's hundred pictures and back down to the newest, as
    README.md's image-heavy channel, then settled."""
    up = [command for _ in range(45) for command in ('scroll -1500 0.3', 'sleep 0.3')]
    down = [command for _ in range(50) for command in ('scroll 1500 0.3', 'sleep 0.1')]
    session('do', 'move 700 450', *up, 'shot images-top', *down)
    settled('Image-heavy channel, settled', notes, results, 'images-bottom')


def scrolling(notes, results):
    """60 touchpad events a second over the timeline, up and down by turns
    every two seconds, as testing/measure_scroll.py sends them."""
    session('do', 'move 700 450')
    seat = display_environment()['XDG_RUNTIME_DIR'] + '/yapfast-input.sock'
    done = threading.Event()

    def scroll():
        started = time.monotonic()
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
            while not done.is_set():
                elapsed = time.monotonic() - started
                direction = -1 if int(elapsed / 2) % 2 == 0 else 1
                client.sendto(f'scroll {25 * direction}'.encode(), seat)
                time.sleep(max(0, (int(elapsed * 60) + 1) / 60 - elapsed))

    scroller = threading.Thread(target=scroll)
    scroller.start()
    try:
        results.append(sample('Scrolling, 60 touchpad events/s', args.seconds, notes))
    finally:
        done.set()
        scroller.join()


def call(notes, results):
    """The Lounge call: at rest, three people speaking, Bob's stream watched."""
    session('do', 'click 141 268')  # Lounge, which joins it.
    settled('Call at rest', notes, results, 'call-rest')
    control('speakers-start')
    settled('Call, three people speaking', notes, results, 'call-speaking')
    control('speakers-stop')
    session('do', 'click 141 268', 'sleep 2', 'click 1260 213')  # Its stage, then Bob's stream.
    settled('Call, watching a stream', notes, results, 'call-stream')


def parity(notes, results):
    groups = args.parity
    settled('Idle on a DM', notes, results, 'dm') if 'dm' in groups else time.sleep(args.warmup)
    if {'busy', 'images', 'scroll'} & set(groups):
        session('do', 'click 36 85', 'sleep 3')  # Server, which opens on #general.
    for group, measure in (('busy', busy), ('images', images), ('scroll', scrolling), ('call', call)):
        if group in groups:
            measure(notes, results)


def frames_shown(log, startup, samples):
    """From the display's log of the app's surface commits: the first frame
    after the start, and the new frames a second in each sample."""
    if not log.exists():
        return
    commits = [json.loads(line) for line in log.read_text().splitlines() if line.startswith('{')]
    fresh = [commit['monotonic_ns'] for commit in commits
             if commit['new_buffer'] and commit['pid'] == startup.get('pid')]
    if fresh and 'started_monotonic_s' in startup:
        startup['first_frame_seconds'] = round(fresh[0] / 1e9 - startup['started_monotonic_s'], 3)
    for entry in samples:
        start, end = entry['window_ns']
        entry['new_frames_per_second'] = round(sum(start <= at < end for at in fresh) / ((end - start) / 1e9), 1)


def keep_shots(run):
    # The next session's start empties the folder.
    for shot in (SESSION / 'shots').glob('*.png'):
        shot.replace(args.output / f'run{run}-{shot.name}')


results = []
startups = []
build = not args.no_build
try:
    for run in range(1, args.runs + 1):
        subprocess.run([str(ROOT / 'scripts/ui-session'), 'stop'], cwd=ROOT, capture_output=True)
        (SESSION / 'yapfast.pid').unlink(missing_ok=True)
        startup = {'run': run}
        surfaces = args.output / f'surfaces-run{run}.log'
        os.environ['YAPFAST_FRAMEWORK_SURFACE_LOG'] = str(surfaces)
        watcher = threading.Thread(target=watch_startup, args=(startup,))
        watcher.start()
        session('start', '--discord', '--tray', '--profile', 'measure', '--fake-args', args.fake_args,
                '--renderer', args.renderer, *(['--size', '1400x860'] if args.parity else []),
                *(['--bin', str(args.bin)] if args.bin else [] if build else ['--no-build']))
        build = False
        notes = {'renderer': args.renderer, 'run': run}
        watcher.join()
        startups.append(startup)
        if args.parity:
            first = len(results)
            parity(notes, results)
            keep_shots(run)
            # The display writes its log out as it stops.
            subprocess.run([str(ROOT / 'scripts/ui-session'), 'stop'], cwd=ROOT, capture_output=True)
            frames_shown(surfaces, startup, results[first:])
            surfaces.unlink(missing_ok=True)
            print(json.dumps(startup), flush=True)
            continue
        print(json.dumps(startup), flush=True)
        session('do', 'sleep 3', 'click 36 101', 'sleep 2', 'click 142 149')  # Server, #general
        cycle('#general', notes, results)
        if not args.no_call:
            session('do', 'click 142 257')  # Lounge, which joins it
            cycle('Lounge call', notes, results)
            session('do', 'tray click Disconnect')
            # The call trims its heap 3 s after it ends.
            time.sleep(args.warmup)
            results.append(sample('After leaving the call', args.seconds, notes))
        keep_shots(run)
finally:
    subprocess.run([str(ROOT / 'scripts/ui-session'), 'stop'], cwd=ROOT, capture_output=True)
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binary': str(args.bin or ROOT / 'target/measure/yapfast'),
        'environment': args.env,
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'scripts/ui-session --tray: private headless Weston, PulseAudio and D-Bus, '
                     'with testing/tray_host.rs as the tray',
        'remote_load': f'testing/fake_discord.ts {args.fake_args}',
        'startups': startups,
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
