#!/usr/bin/env python3
"""Run Yapfast on the private display and act on commands from scripts/ui-session.

scripts/ui-session starts this inside scripts/run-framework-isolated. It
starts Yapfast (and, with --discord, testing/fake_discord.ts), then reads
"ID<TAB>COMMAND" lines appended to STATE/commands and answers each with
"ID<TAB>ok|error<TAB>JSON_OUTPUT" in STATE/results. See scripts/ui-session for the
commands.
"""
import argparse
import json
import math
import os
from pathlib import Path
import runpy
import shlex
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--state', type=Path, required=True)
parser.add_argument('--bin', type=Path, required=True)
parser.add_argument('--size', default='1120x760')
parser.add_argument('--renderer', choices=['software', 'gpu'], default='software')
parser.add_argument('--discord', action='store_true')
parser.add_argument('--fake-args', default='')
parser.add_argument('--tray', type=Path, help='testing/tray_host.rs, built, to run as the tray')
parser.add_argument('--record', type=Path, help='testing/record_host.rs, built, for record')
parser.add_argument('--cursor', type=int, metavar='SIZE', help="show the pointer, Adwaita's, this many pixels tall")
parser.add_argument('yapfast_args', nargs=argparse.REMAINDER)
args = parser.parse_args()
yapfast_args = args.yapfast_args[1:] if args.yapfast_args[:1] == ['--'] else args.yapfast_args
state = args.state

KEYS = {
    'esc': 1, 'backspace': 14, 'tab': 15, 'enter': 28, 'ctrl': 29, 'shift': 42,
    'alt': 56, 'space': 57, 'home': 102, 'up': 103, 'pageup': 104, 'left': 105,
    'right': 106, 'end': 107, 'down': 108, 'pagedown': 109, 'delete': 111,
    'f10': 68, 'menu': 127,
}
KEYS.update(zip('abcdefghijklmnopqrstuvwxyz',
                [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44]))
KEYS.update(zip('1234567890', range(2, 12)))
# Characters `type` can enter on the US layout, and whether they need Shift.
CHARACTERS = {character: (code, False) for character, code in KEYS.items() if len(character) == 1}
CHARACTERS.update({character.upper(): (code, True) for character, code in KEYS.items()
                   if len(character) == 1 and character.isalpha()})
CHARACTERS.update({' ': (57, False), '-': (12, False), '=': (13, False), ';': (39, False),
                   "'": (40, False), ',': (51, False), '.': (52, False), '/': (53, False),
                   '!': (2, True), '@': (3, True), '#': (4, True), '_': (12, True),
                   ':': (39, True), '?': (53, True), '(': (10, True), ')': (11, True),
                   '`': (41, False), '~': (41, True), '[': (26, False), ']': (27, False),
                   '{': (26, True), '}': (27, True), '\\': (43, False), '|': (43, True),
                   '"': (40, True), '<': (51, True), '>': (52, True), '$': (5, True),
                   '%': (6, True), '^': (7, True), '&': (8, True), '*': (9, True), '+': (13, True)})

fake = None
# Yapfast reads commands for its window from this file; see `test_control` in src/ui.rs.
window_control = state / 'window-control'
environment = dict(os.environ, YAPFAST_UI_TEST_SIZE=args.size, YAPFAST_UI_TEST_CONTROL=str(window_control))
if args.cursor:
    # Weston draws the pointer the window asks for, and winit finds none
    # without a theme; the shell gives one, as the desktop's isn't here.
    environment.update(XCURSOR_THEME='Adwaita', XCURSOR_SIZE=str(args.cursor),
                       XCURSOR_PATH=os.environ['YAPFAST_FRAMEWORK_CURSORS'])
if args.discord:
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    discord = f'http://127.0.0.1:{port}'
    fake = subprocess.Popen(['bun', str(ROOT / 'testing/fake_discord.ts'), '--port', str(port),
                             *shlex.split(args.fake_args)],
                            stdout=(state / 'fake.log').open('wb'), stderr=subprocess.STDOUT,
                            start_new_session=True)
    deadline = time.monotonic() + 20
    while True:
        try:
            urllib.request.urlopen(f'{discord}/_control/health', timeout=1)
            break
        except OSError:
            if fake.poll() is not None or time.monotonic() > deadline:
                raise SystemExit('The fake Discord did not start; see ' + str(state / 'fake.log'))
            time.sleep(.2)
    environment['YAPFAST_DISCORD_TEST_SERVER'] = discord

data = state / 'data'
yapfast = None
tray = None
# The pointer's position, where Weston puts it until the first move.
pointer = [100., 100.]
# The recording under way: its file, record_host and ffmpeg.
recording = None


def start_tray():
    """The stand-in tray, on this session's private bus, as testing/tray_host.rs says."""
    global tray
    tray = subprocess.Popen([str(args.tray), 'watch'], stdout=(state / 'tray.log').open('ab'),
                            stderr=subprocess.STDOUT, start_new_session=True)
    time.sleep(.5)
    if tray.poll() is not None:
        raise RuntimeError(f'The tray exited with {tray.returncode}; see {state / "tray.log"}')


def tray_command(rest):
    if args.tray is None:
        raise ValueError('tray needs start --tray')
    if rest == 'off':
        stop(tray)
        return ''
    if rest == 'on':
        if tray is None or tray.poll() is not None:
            start_tray()
        return ''
    values = rest.split(' ', 1)
    if values[0] == 'icon':
        shots = state / 'shots'
        shots.mkdir(exist_ok=True)
        picture = shots / f'{values[1]}.pam'
        size = subprocess.run([str(args.tray), 'icon', str(picture)], check=True, capture_output=True,
                              text=True, timeout=10).stdout.strip()
        path = shots / f'{values[1]}.png'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', str(picture), str(path)],
                       check=True, timeout=10)
        picture.unlink()
        return f'{path} {size}'
    result = subprocess.run([str(args.tray), *values], capture_output=True, text=True, timeout=10)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f'tray_host exited with {result.returncode}')
    # The window opens, or the menu acts, as Yapfast hears it.
    time.sleep(.8)
    return result.stdout.strip()


def launch(extra=()):
    global yapfast
    # A new Yapfast would run every line again.
    window_control.write_text('')
    # Yapfast takes --preview only before --data-dir.
    yapfast = subprocess.Popen([str(args.bin), '--renderer', args.renderer, *yapfast_args, *extra,
                                '--data-dir', str(data)],
                            env=environment, stdout=(state / 'yapfast.log').open('ab'),
                            stderr=subprocess.STDOUT, start_new_session=True)
    (state / 'yapfast.pid').write_text(f'{yapfast.pid}\n')
    time.sleep(2)
    if yapfast.poll() is not None:
        raise RuntimeError(f'Yapfast exited with {yapfast.returncode}; see {state / "yapfast.log"}')


def stop(process):
    if process is None or process.poll() is not None:
        return
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(5)


def capture_filters():
    """The ffmpeg filters that stand Weston's GL capture upright: it comes upside
    down with NVIDIA's driver, and upright with Mesa's software llvmpipe, which
    a machine without a GPU uses. Other drivers keep the flip."""
    log = state / 'weston.log'
    software = log.exists() and 'GL renderer: llvmpipe' in log.read_text(errors='replace')
    return [] if software else ['vflip']


def shot(name, *crop):
    shots = state / 'shots'
    shots.mkdir(exist_ok=True)
    path = shots / f'{name}.png'
    time.sleep(.4)
    with tempfile.TemporaryDirectory(prefix='yapfast-shot-') as folder:
        subprocess.run(['weston-screenshooter'], env=dict(os.environ, XDG_PICTURES_DIR=folder),
                       check=True, timeout=10, capture_output=True)
        picture = next(Path(folder).glob('*.png'))
        filters = capture_filters()
        if crop:
            x, y, width, height = map(int, crop)
            filters.append(f'crop={width}:{height}:{x}:{y}')
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', str(picture),
                        *(['-vf', ','.join(filters)] if filters else []), str(path)], check=True, timeout=10)
    return str(path)


def move(x, y, pause=.03):
    pointer[:] = [x, y]
    INPUT['send'](f'move {x:.2f} {y:.2f}', pause)


def eased(fraction):
    """Ease in and out: slow to start, quick between, slow to stop."""
    return 4 * fraction ** 3 if fraction < .5 else 1 - (2 - 2 * fraction) ** 3 / 2


def steps(seconds):
    """Yields how far through `seconds` each 1/60 s step is, eased, on time."""
    count = max(1, round(seconds * 60))
    begin = time.monotonic()
    for step in range(1, count + 1):
        delay = begin + step / 60 - time.monotonic()
        if delay > 0:
            time.sleep(delay)
        yield eased(step / count)


def glide(x, y, seconds=None):
    """Moves the pointer to X Y along a straight, eased path, 60 steps a second;
    by default in .3 to 1 s, by the distance."""
    from_x, from_y = pointer
    if seconds is None:
        seconds = min(1., .3 + math.hypot(x - from_x, y - from_y) / 2000)
    for fraction in steps(seconds):
        move(from_x + (x - from_x) * fraction, from_y + (y - from_y) * fraction, pause=0)


def smooth_scroll(pixels, seconds):
    sent = 0.
    for fraction in steps(seconds):
        INPUT['send'](f'scroll {pixels * fraction - sent:.3f}', 0)
        sent = pixels * fraction


def record(rest):
    global recording
    action, _, name = rest.partition(' ')
    if action == 'stop' and not name:
        if recording is None:
            raise ValueError('nothing is recording')
        return stop_recording()
    if action != 'start' or not name or '/' in name:
        raise ValueError('record start NAME | record stop')
    if args.record is None:
        raise ValueError('record needs record_host; start the session again')
    if recording is not None:
        raise ValueError(f'already recording {recording["path"]}')
    videos = state / 'videos'
    videos.mkdir(exist_ok=True)
    path = videos / f'{name}.mp4'
    log_path = state / 'record.log'
    log = log_path.open('ab')
    offset = log.tell()
    host = subprocess.Popen([str(args.record)], stdout=subprocess.PIPE, stderr=log, bufsize=0,
                            start_new_session=True)
    # WIDTH HEIGHT PIXEL_FORMAT, then frames; unbuffered, so reading the line
    # leaves every frame to ffmpeg.
    header = host.stdout.readline().decode().split()
    if len(header) != 3:
        host.stdout.close()
        log.close()
        stop(host)
        raise RuntimeError(f'record_host did not start; see {log_path}')
    width, height, pixel_format = header
    # libx264 at its veryfast preset encodes 1080p60 several times faster than
    # real time here; crf 16 keeps text sharp.
    filters = capture_filters()
    encoder = subprocess.Popen(
        ['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'rawvideo', '-pixel_format', pixel_format,
         '-video_size', f'{width}x{height}', '-framerate', '60', '-i', 'pipe:0',
         *(['-vf', ','.join(filters)] if filters else []),
         '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '16', '-pix_fmt', 'yuv420p',
         '-movflags', '+faststart', str(path)],
        stdin=host.stdout, stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
    host.stdout.close()
    log.close()
    recording = {'path': path, 'host': host, 'encoder': encoder, 'log': log_path, 'offset': offset}
    time.sleep(.3)
    if host.poll() is not None or encoder.poll() is not None:
        stop_recording()
        raise RuntimeError(f'The recording stopped; see {log_path}')
    return str(path)


def stop_recording():
    """Ends the recording so its file is whole, and says what it holds."""
    global recording
    if recording is None:
        return None
    current, recording = recording, None
    host, encoder = current['host'], current['encoder']
    if host.poll() is None:
        host.send_signal(signal.SIGINT)
    try:
        host.wait(10)
    except subprocess.TimeoutExpired:
        host.kill()
        host.wait(5)
    # ffmpeg ends at the end of its input, then moves the index to the front.
    try:
        encoder.wait(60)
    except subprocess.TimeoutExpired:
        encoder.kill()
        encoder.wait(5)
    with current['log'].open('rb') as log:
        log.seek(current['offset'])
        report = [line.removeprefix('record_host: ') for line in log.read().decode(errors='replace').splitlines()
                  if line.startswith('record_host: ')]
    probe = subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0',
                            str(current['path'])], capture_output=True, text=True, timeout=20)
    duration = f'{float(probe.stdout):.2f} s' if probe.returncode == 0 and probe.stdout.strip() else 'unreadable'
    if encoder.returncode != 0:
        report.append(f'ffmpeg exited with {encoder.returncode}; see {current["log"]}')
    return '; '.join([f'{current["path"]}, {duration}', *report])


def press(name):
    """A key by name or evdev code, with modifiers joined by +, as ctrl+a."""
    *modifiers, key = name.split('+')
    codes = [KEYS[modifier] for modifier in modifiers]
    for code in codes:
        INPUT['send'](f'key {code} 1')
    INPUT['key'](int(key) if key.isdecimal() else KEYS[key])
    for code in reversed(codes):
        INPUT['send'](f'key {code} 0')


def type_text(text):
    for character in text:
        if character not in CHARACTERS:
            raise ValueError(f'type cannot enter {character!r}; paste it or use key')
        code, shift = CHARACTERS[character]
        if shift:
            INPUT['send']('key 42 1')
        INPUT['key'](code)
        if shift:
            INPUT['send']('key 42 0')


def control(method, name):
    if fake is None:
        raise ValueError('control and get need --discord')
    request = urllib.request.Request(f'{discord}/_control/{name}', method=method)
    with urllib.request.urlopen(request, timeout=10) as response:
        return response.read().decode(errors='replace').strip()


def perform(command):
    verb, _, rest = command.partition(' ')
    values = rest.split()
    if verb == 'relaunch':
        stop(yapfast)
        launch(values)
        return f'pid {yapfast.pid}'
    if verb == 'pid':
        return str(yapfast.pid)
    if verb == 'sleep':
        time.sleep(float(rest))
        return ''
    if verb == 'control':
        return control('POST', rest)
    if verb == 'get':
        return control('GET', rest)
    if verb == 'tray':
        return tray_command(rest)
    if verb == 'record':
        return record(rest)
    if yapfast.poll() is not None:
        raise RuntimeError(f'Yapfast exited with {yapfast.returncode}; see {state / "yapfast.log"}, or relaunch')
    if verb == 'click':
        # Then modifiers held down for the click, as shift or ctrl+shift, and
        # glide [SECONDS] to move there as a hand would first.
        x, y, *more = values
        x, y = float(x), float(y)
        seconds = None
        if 'glide' in more:
            at = more.index('glide')
            if more[at + 1:at + 2] and more[at + 1].replace('.', '', 1).isdecimal():
                seconds = float(more.pop(at + 1))
            more.pop(at)
            glide(x, y, seconds)
            time.sleep(.05)
        codes = [KEYS[modifier] for modifier in more[0].split('+')] if more else []
        for code in codes:
            INPUT['send'](f'key {code} 1')
        INPUT['click'](x, y)
        pointer[:] = [x, y]
        for code in reversed(codes):
            INPUT['send'](f'key {code} 0')
        time.sleep(.3)
    elif verb in ('rightclick', 'middleclick'):
        move(*map(float, values))
        button = 273 if verb == 'rightclick' else 274
        INPUT['send'](f'button {button} 1')
        INPUT['send'](f'button {button} 0')
        time.sleep(.3)
    elif verb == 'move':
        move(*map(float, values))
        time.sleep(.2)
    elif verb == 'glide':
        glide(*map(float, values))
        time.sleep(.05)
    elif verb == 'drag':
        # Pressed at the first point and moved in steps, as a hand would,
        # then released at the second.
        x, y, to_x, to_y = map(float, values)
        move(x, y)
        INPUT['send']('button 272 1')
        time.sleep(.1)
        for step in range(1, 11):
            move(x + (to_x - x) * step / 10, y + (to_y - y) * step / 10)
            time.sleep(.02)
        INPUT['send']('button 272 0')
        time.sleep(.3)
    elif verb == 'press':
        # Held until `release`, for what happens while a button is down,
        # such as a window being dragged by its title bar.
        button = 274 if values[2:] == ['middle'] else 272
        move(*map(float, values[:2]))
        INPUT['send'](f'button {button} 1')
        time.sleep(.1)
    elif verb == 'release':
        button = 274 if values == ['middle'] else 272
        INPUT['send'](f'button {button} 0')
        time.sleep(.3)
    elif verb == 'scroll':
        # Over SECONDS, if given, in eased steps 60 times a second.
        if len(values) == 2:
            smooth_scroll(float(values[0]), float(values[1]))
        else:
            INPUT['scroll'](float(rest))
        time.sleep(.1)
    elif verb == 'key':
        press(rest)
        time.sleep(.2)
    elif verb in ('keydown', 'keyup'):
        code = int(rest) if rest.isdecimal() else KEYS[rest]
        INPUT['send'](f'key {code} {int(verb == "keydown")}')
        time.sleep(.1)
    elif verb == 'type':
        type_text(rest)
        time.sleep(.2)
    elif verb == 'shot':
        return shot(*values)
    elif verb == 'pick':
        kind, _, rest = rest.partition(' ')
        path = (ROOT / rest).resolve()
        kinds = ('avatar', 'banner', 'icon', 'server-banner', 'splash', 'role-icon', 'emoji', 'sticker')
        if kind not in kinds or not path.is_file():
            raise ValueError(f'pick {"|".join(kinds)} PATH, with a file at {path}')
        with window_control.open('a') as window:
            window.write(f'pick {kind} {path}\n')
        time.sleep(.8)
    elif verb in ('stage', 'stage-aside'):
        path = (ROOT / rest).resolve()
        if not path.is_file():
            raise ValueError(f'no file at {path}')
        with window_control.open('a') as window:
            window.write(f'{verb} {path}\n')
        # Yapfast reads the file every half second.
        time.sleep(.8)
    elif verb == 'theme':
        with window_control.open('a') as window:
            window.write(f'theme {rest}\n')
        time.sleep(.8)
    elif verb == 'window':
        if rest not in ('close', 'open', 'destroy', 'create', 'hide', 'show', 'trim', 'stats'):
            raise ValueError(f'unknown window command {rest!r}')
        with window_control.open('a') as window:
            window.write(f'{rest}\n')
        time.sleep(1.2)
    else:
        raise ValueError(f'unknown command {verb!r}')
    return ''


def ended(signum, _frame):
    # Through the finally below, so a recording ends whole.
    raise SystemExit(128 + signum)


signal.signal(signal.SIGTERM, ended)
commands = state / 'commands'
results = state / 'results'
try:
    if args.tray:
        start_tray()
    launch()
    (state / 'ready').write_text('ready\n')
    seen = 0
    while True:
        lines = commands.read_text().splitlines() if commands.exists() else []
        for line in lines[seen:]:
            seen += 1
            identifier, _, command = line.partition('\t')
            if command == 'quit':
                with results.open('a') as answers:
                    answers.write(f'{identifier}\tok\t""\n')
                raise SystemExit(0)
            try:
                outcome = 'ok', perform(command)
            except Exception as error:  # Report it to the caller and keep the session.
                outcome = 'error', f'{type(error).__name__}: {error}'
            with results.open('a') as answers:
                answers.write(f'{identifier}\t{outcome[0]}\t{json.dumps(outcome[1])}\n')
        time.sleep(.05)
finally:
    stop_recording()
    stop(yapfast)
    stop(tray)
    stop(fake)
