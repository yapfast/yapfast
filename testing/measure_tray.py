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
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
SESSION = ROOT / '.local/ui-session'
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
args = parser.parse_args()
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


def sample(name, seconds, notes):
    pid = int((SESSION / 'yapfast.pid').read_text())
    first = anonymous(pid)
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(pid),
                                                 '--seconds', str(seconds)], text=True))
    result.update(profile=name, anonymous_mib=anonymous(pid), anonymous_mib_at_start=first, **heap(),
                  huge_pages_mib=huge_pages(pid),
                  threads=len(os.listdir(f'/proc/{pid}/task')), **notes)
    print(json.dumps({key: result[key] for key in ('profile', 'run', 'anonymous_mib_at_start', 'anonymous_mib',
                                                    'heap_in_use_mib', 'huge_pages_mib',
                                                    'pss_mib_mean', 'cpu_percent_one_core', 'threads', 'arenas')}), flush=True)
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
    hertz = os.sysconf('SC_CLK_TCK')
    stat = lambda: Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    started = int(stat()[19]) / hertz
    readings = []
    while not readings or readings[-1][0] < 15:
        try:
            fields = stat()
        except FileNotFoundError:
            return
        readings.append((time.clock_gettime(time.CLOCK_BOOTTIME) - started, (int(fields[11]) + int(fields[12])) / hertz))
        time.sleep(.02)
    found['cpu_seconds_in_first_3_s'] = next(cpu for at, cpu in readings if at >= 3)
    for at, cpu in readings:
        later = [reading for reading in readings if reading[0] >= at + 1]
        if later and later[0][1] - cpu < .05:
            found.update(startup_seconds=round(at, 2), startup_cpu_seconds=round(cpu, 2))
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
        watcher = threading.Thread(target=watch_startup, args=(startup,))
        watcher.start()
        session('start', '--discord', '--tray', '--profile', 'measure', '--fake-args', args.fake_args,
                '--renderer', args.renderer,
                *(['--bin', str(args.bin)] if args.bin else [] if build else ['--no-build']))
        build = False
        notes = {'renderer': args.renderer, 'run': run}
        watcher.join()
        print(json.dumps(startup), flush=True)
        startups.append(startup)
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
