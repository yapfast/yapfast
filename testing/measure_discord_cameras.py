#!/usr/bin/env python3
"""Measure the desktop client in a Discord call as cameras turn on.

Alice joins Lounge on testing/fake_discord.ts with Bob, Carol and Dave, whose
cameras the fake turns on in turn: none, then 1 and 2, then all 3 and
Alice's own, 4 in all. Then every camera goes off, and on and off once more.
Only Yapfast's process is sampled.

The fake sends each camera as the Annex B file given with --camera, at 30
frames a second; make a 720p one with:

  ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30 -t 10 -c:v libx264 \\
    -profile:v baseline -b:v 1500k -maxrate 1500k -bufsize 1500k -g 60 \\
    -x264-params aud=1 -bsf:v h264_mp4toannexb -f h264 camera-720p.h264

Runs Yapfast through scripts/ui-session, which builds the Discord-only
`measure` build, on a private display with a private PulseAudio.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
SESSION = ROOT / '.local/ui-session'
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--camera', type=Path, required=True, help='H264 Annex B file the fake films with')
parser.add_argument('--renderer', action='append', choices=['gpu', 'software'])
parser.add_argument('--warmup', type=int, default=15, help='seconds before each sample')
parser.add_argument('--seconds', type=int, default=20, help='sample length')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--no-build', action='store_true', help="run the session's last build")
args = parser.parse_args()
renderers = args.renderer or ['software', 'gpu']
args.output.mkdir(parents=True, exist_ok=True)
ENVIRONMENT = dict(os.environ, YAPFAST_CAMERA_TEST_PATTERN='1', FAKE_DISCORD_CAMERA=str(args.camera.resolve()))


def session(*arguments):
    subprocess.run([str(ROOT / 'scripts/ui-session'), *arguments], check=True, env=ENVIRONMENT, cwd=ROOT,
                   stdout=subprocess.DEVNULL)


def sample(name, notes):
    pid = int((SESSION / 'yapfast.pid').read_text())
    result = json.loads(subprocess.check_output(['python3', str(ROOT / 'scripts/measure-process'), str(pid),
                                                 '--seconds', str(args.seconds)], text=True))
    names = {}
    for task in os.listdir(f'/proc/{pid}/task'):
        thread = Path(f'/proc/{pid}/task/{task}/comm').read_text().strip()
        names[thread] = names.get(thread, 0) + 1

    # Anonymous memory, swapped or not, is what Yapfast itself holds; on a host
    # short of memory the kernel drops shared-library pages from RSS and PSS.
    def field(path, key):
        return next(int(line.split()[1]) for line in Path(path).read_text().splitlines()
                    if line.startswith(key)) / 1024
    anonymous = field(f'/proc/{pid}/smaps_rollup', 'Anonymous:') + field(f'/proc/{pid}/status', 'VmSwap:')
    result.update(profile=name, threads=sum(names.values()), thread_names=names,
                  anonymous_mib=round(anonymous, 1), **notes)
    print(json.dumps({key: result[key] for key in ('profile', 'renderer', 'anonymous_mib', 'pss_mib_mean',
                                                    'rss_mib_mean', 'cpu_percent_one_core', 'threads')}), flush=True)
    return result


def keep_shots():
    # The next session's start empties the folder.
    for shot in (SESSION / 'shots').glob('*-cameras-*.png'):
        shot.replace(args.output / shot.name)


def settle(shot):
    time.sleep(args.warmup)
    session('do', f'shot {shot}')


results = []
build = not args.no_build
try:
    for renderer in renderers:
        subprocess.run([str(ROOT / 'scripts/ui-session'), 'stop'], cwd=ROOT, capture_output=True)
        session('start', '--discord', '--profile', 'measure', '--renderer', renderer,
                *([] if build else ['--no-build']))
        build = False
        notes = {'renderer': renderer, 'warmup_seconds': args.warmup}
        session('do', 'sleep 3', 'click 36 101', 'sleep 1', 'click 142 186', 'sleep 3')  # Server, Lounge
        settle(f'{renderer}-cameras-0')
        results.append(sample('4-person call, no cameras', notes))
        session('do', 'control camera?who=bob')
        settle(f'{renderer}-cameras-1')
        results.append(sample('4-person call, 1 remote camera', notes))
        session('do', 'control camera?who=carol')
        settle(f'{renderer}-cameras-2')
        results.append(sample('4-person call, 2 remote cameras', notes))
        session('do', 'control camera?who=dave', 'click 703 718')  # Alice's camera
        settle(f'{renderer}-cameras-4')
        results.append(sample("4-person call, 3 remote cameras and Alice's own", notes))
        off = ['control camera?who=bob&off', 'control camera?who=carol&off', 'control camera?who=dave&off']
        session('do', 'click 703 718', *off)
        settle(f'{renderer}-cameras-off')
        results.append(sample('4-person call, every camera off again', notes))
        # Again, to show that turning cameras on and off doesn't add up.
        session('do', 'control camera?who=bob', 'control camera?who=carol', 'control camera?who=dave',
                'click 703 718', f'sleep {args.warmup}', 'click 703 718', *off)
        settle(f'{renderer}-cameras-off-again')
        results.append(sample('4-person call, every camera on and off a second time', notes))
        session('do', 'click 843 718')  # Leave
        keep_shots()
finally:
    subprocess.run([str(ROOT / 'scripts/ui-session'), 'stop'], cwd=ROOT, capture_output=True)
    keep_shots()
    (args.output / 'results.json').write_text(json.dumps({
        'recorded_on': time.strftime('%Y-%m-%d'),
        'binary': str(ROOT / 'target/measure/yapfast'),
        'os': f'{platform.system()} {platform.release()}', 'machine': platform.machine(),
        'isolation': 'scripts/ui-session: private headless Weston and PulseAudio',
        'remote_load': f'testing/fake_discord.ts; Bob, Carol and Dave film {args.camera.name} at 30 frames '
                       "a second; Alice films a 640x360 test pattern",
        'samples': results,
    }, indent=2) + '\n')
    print(f'Results {args.output / "results.json"}', flush=True)
